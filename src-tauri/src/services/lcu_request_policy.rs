use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

const CAPACITY: usize = 128;
type Response = Result<String, String>;
type Route = (u64, reqwest::Method, String);

#[derive(Default)]
pub(super) struct RequestPolicy {
    reads: Mutex<HashMap<(u64, String), Weak<ReadFlight>>>,
    cooldowns: Mutex<HashMap<Route, Instant>>,
}

pub(super) enum Read {
    Lead(ReadLeader),
    Wait(Arc<ReadFlight>),
    Unshared,
}

#[derive(Default)]
pub(super) struct ReadFlight {
    result: Mutex<Option<Arc<Response>>>,
    done: Notify,
}

pub(super) struct ReadLeader(Arc<ReadFlight>);

impl ReadFlight {
    pub async fn wait(&self) -> Response {
        loop {
            let notified = self.done.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let result = self
                .result
                .lock()
                .map_err(|_| "LCU shared read unavailable")?
                .clone();
            if let Some(result) = result {
                return result.as_ref().clone();
            }
            notified.await;
        }
    }
}

impl ReadLeader {
    pub fn complete(&self, result: Response) {
        if let Ok(mut state) = self.0.result.lock() {
            *state = Some(Arc::new(result));
        }
        self.0.done.notify_waiters();
    }
}

impl Drop for ReadLeader {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.result.lock() {
            if state.is_none() {
                *state = Some(Arc::new(Err("LCU shared read cancelled".into())));
            }
        }
        self.0.done.notify_waiters();
    }
}

impl RequestPolicy {
    pub fn begin_read(&self, epoch: u64, endpoint: &str) -> Result<Read, String> {
        let mut reads = self
            .reads
            .lock()
            .map_err(|_| "LCU shared read unavailable")?;
        reads.retain(|_, value| value.strong_count() > 0);
        let key = (epoch, endpoint.to_owned());
        if let Some(flight) = reads.get(&key).and_then(Weak::upgrade) {
            let pending = flight
                .result
                .lock()
                .map_err(|_| "LCU shared read unavailable")?
                .is_none();
            if pending {
                return Ok(Read::Wait(flight));
            }
        }
        if reads.len() >= CAPACITY {
            return Ok(Read::Unshared);
        }
        let flight = Arc::new(ReadFlight::default());
        reads.insert(key, Arc::downgrade(&flight));
        Ok(Read::Lead(ReadLeader(flight)))
    }

    pub fn cooldown(
        &self,
        epoch: u64,
        method: &reqwest::Method,
        endpoint: &str,
        now: Instant,
    ) -> Option<Duration> {
        let mut routes = self.cooldowns.lock().ok()?;
        routes.retain(|_, until| *until > now);
        routes
            .get(&(epoch, method.clone(), endpoint.to_owned()))
            .map(|until| until.saturating_duration_since(now))
    }

    pub fn rate_limited(
        &self,
        epoch: u64,
        method: &reqwest::Method,
        endpoint: &str,
        delay: Duration,
        now: Instant,
    ) {
        let Ok(mut routes) = self.cooldowns.lock() else {
            return;
        };
        routes.retain(|_, until| *until > now);
        let key = (epoch, method.clone(), endpoint.to_owned());
        if routes.len() >= CAPACITY && !routes.contains_key(&key) {
            if let Some(oldest) = routes
                .iter()
                .min_by_key(|(_, until)| **until)
                .map(|(key, _)| key.clone())
            {
                routes.remove(&oldest);
            }
        }
        let until = now + delay.min(Duration::from_secs(3600));
        routes
            .entry(key)
            .and_modify(|old| *old = (*old).max(until))
            .or_insert(until);
    }
}

pub(super) fn retry_after(value: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> Duration {
    let seconds = value
        .and_then(|raw| {
            let raw = raw.trim();
            raw.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(raw).ok().map(|date| {
                    let delay = date.signed_duration_since(now).to_std().unwrap_or_default();
                    delay.as_secs() + u64::from(delay.subsec_nanos() > 0)
                })
            })
        })
        .unwrap_or(1);
    Duration::from_secs(seconds.min(3600))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_reads_and_rate_limits_remain_bounded_under_many_distinct_routes() {
        let p = RequestPolicy::default();
        let leaders: Vec<_> = (0..CAPACITY)
            .map(|i| match p.begin_read(1, &format!("/{i}")).unwrap() {
                Read::Lead(leader) => leader,
                _ => panic!("new route must lead"),
            })
            .collect();
        for i in CAPACITY..CAPACITY * 10 {
            assert!(matches!(
                p.begin_read(1, &format!("/{i}")).unwrap(),
                Read::Unshared
            ));
        }
        assert!(matches!(p.begin_read(1, "/0").unwrap(), Read::Wait(_)));
        assert_eq!(p.reads.lock().unwrap().len(), CAPACITY);
        drop(leaders);
        assert!(matches!(p.begin_read(1, "/new").unwrap(), Read::Lead(_)));
        assert_eq!(p.reads.lock().unwrap().len(), 1);
        let now = Instant::now();
        for i in 0..CAPACITY * 10 {
            p.rate_limited(
                1,
                &reqwest::Method::GET,
                &format!("/{i}"),
                Duration::from_secs(60),
                now,
            );
        }
        assert_eq!(p.cooldowns.lock().unwrap().len(), CAPACITY);
        assert!(p
            .cooldown(
                1,
                &reqwest::Method::GET,
                "/none",
                now + Duration::from_secs(60)
            )
            .is_none());
        assert!(p.cooldowns.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn leader_cancellation_wakes_followers_and_finished_reads_are_not_cached() {
        let p = RequestPolicy::default();
        let Read::Lead(leader) = p.begin_read(1, "/route").unwrap() else {
            panic!()
        };
        let Read::Wait(follower) = p.begin_read(1, "/route").unwrap() else {
            panic!()
        };
        drop(leader);
        assert!(follower.wait().await.unwrap_err().contains("cancelled"));
        let Read::Lead(next) = p.begin_read(1, "/route").unwrap() else {
            panic!()
        };
        let Read::Wait(waiter) = p.begin_read(1, "/route").unwrap() else {
            panic!()
        };
        next.complete(Ok("fresh".into()));
        assert_eq!(waiter.wait().await.unwrap(), "fresh");
        assert!(matches!(p.begin_read(1, "/route").unwrap(), Read::Lead(_)));
        assert!(matches!(p.begin_read(2, "/route").unwrap(), Read::Lead(_)));
    }
    #[test]
    fn cooldown_is_scoped_and_does_not_shorten_a_concurrent_retry_deadline() {
        let p = RequestPolicy::default();
        let now = Instant::now();
        let get = reqwest::Method::GET;
        p.rate_limited(1, &get, "/route", Duration::from_secs(5), now);
        p.rate_limited(1, &get, "/route", Duration::from_secs(1), now);
        assert_eq!(
            p.cooldown(1, &get, "/route", now),
            Some(Duration::from_secs(5))
        );
        assert!(p
            .cooldown(1, &reqwest::Method::POST, "/route", now)
            .is_none());
        assert!(p.cooldown(1, &get, "/other", now).is_none());
        assert!(p
            .cooldown(1, &get, "/route", now + Duration::from_secs(5))
            .is_none());
        p.rate_limited(1, &get, "/route", Duration::from_secs(5), now);
        assert!(p.cooldown(2, &get, "/route", now).is_none());
    }
    #[test]
    fn old_generation_cannot_remove_new_generation_flights_or_cooldowns() {
        let p = RequestPolicy::default();
        let Read::Lead(_new) = p.begin_read(2, "/route").unwrap() else {
            panic!()
        };
        let Read::Lead(_old) = p.begin_read(1, "/route").unwrap() else {
            panic!()
        };
        assert!(matches!(p.begin_read(2, "/route").unwrap(), Read::Wait(_)));
        let now = Instant::now();
        p.rate_limited(
            2,
            &reqwest::Method::GET,
            "/route",
            Duration::from_secs(5),
            now,
        );
        assert!(p
            .cooldown(1, &reqwest::Method::GET, "/route", now)
            .is_none());
        assert_eq!(
            p.cooldown(2, &reqwest::Method::GET, "/route", now),
            Some(Duration::from_secs(5))
        );
    }
    #[test]
    fn retry_after_handles_seconds_dates_and_malformed_values_with_a_bound() {
        let now = chrono::DateTime::parse_from_rfc2822("Wed, 21 Oct 2015 07:28:00 GMT")
            .unwrap()
            .with_timezone(&chrono::Utc);
        for (value, seconds) in [
            (Some("2"), 2),
            (Some("Wed, 21 Oct 2015 07:28:03 GMT"), 3),
            (Some("nonsense"), 1),
            (None, 1),
            (Some("0"), 0),
            (Some("999999999"), 3600),
        ] {
            assert_eq!(retry_after(value, now), Duration::from_secs(seconds));
        }
        assert_eq!(
            retry_after(
                Some("Wed, 21 Oct 2015 07:28:03 GMT"),
                now + chrono::Duration::milliseconds(500)
            ),
            Duration::from_secs(3)
        );
    }
}
