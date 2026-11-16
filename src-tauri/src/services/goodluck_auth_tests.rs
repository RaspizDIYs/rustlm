use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::Duration;

struct MockRefresh {
    url: String,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    release: mpsc::Sender<()>,
    started: Option<tokio::sync::oneshot::Receiver<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl MockRefresh {
    fn new(status: u16, delayed: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (release, wait) = mpsc::channel();
        let (started_tx, started) = tokio::sync::oneshot::channel();
        let (requests, stopping) = (count.clone(), stop.clone());
        let worker = thread::spawn(move || {
            let mut started_tx = Some(started_tx);
            while !stopping.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("Mock accept: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                let body_start = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0, "Request closed before headers");
                    request.extend_from_slice(&buffer[..n]);
                    if let Some(end) = request.windows(4).position(|s| s == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                assert!(request.starts_with(b"POST /api/auth/refresh HTTP/1.1\r\n"));
                let content_length = std::str::from_utf8(&request[..body_start])
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .expect("Refresh request must have Content-Length")
                    .1
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                while request.len() < body_start + content_length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0, "Request closed before complete body");
                    request.extend_from_slice(&buffer[..n]);
                }
                let json: serde_json::Value =
                    serde_json::from_slice(&request[body_start..body_start + content_length])
                        .unwrap();
                assert_eq!(json["refreshToken"], "alice-refresh");
                let first = requests.fetch_add(1, Ordering::SeqCst) == 0;
                if let Some(tx) = started_tx.take() {
                    let _ = tx.send(());
                }
                if first && delayed {
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                let body = r#"{"token":"new-jwt","refreshToken":"new-refresh","expiresIn":3600,"user":null}"#;
                let response = format!("HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            url,
            count,
            stop,
            release,
            started: Some(started),
            worker: Some(worker),
        }
    }

    async fn wait_for_request(&mut self) {
        tokio::time::timeout(Duration::from_secs(3), self.started.take().unwrap())
            .await
            .unwrap()
            .unwrap();
    }
}

impl Drop for MockRefresh {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.release.send(());
        let joined = self.worker.take().unwrap().join();
        if !thread::panicking() {
            joined.expect("Mock HTTP server failed");
        }
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rustlm-auth-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn state(id: &str, jwt: &str, expired: bool) -> AuthState {
    AuthState {
        user: GoodLuckUser {
            user_id: id.into(),
            display_name: id.into(),
            avatar_url: String::new(),
            riot_accounts: vec![],
            local_avatar_path: None,
        },
        jwt: jwt.into(),
        refresh_token: format!("{id}-refresh"),
        expires_at: chrono::Utc::now()
            + chrono::Duration::seconds(if expired { -60 } else { 3600 }),
    }
}

fn service(server: &MockRefresh, scratch: &Scratch) -> Arc<GoodLuckAuthService> {
    Arc::new(GoodLuckAuthService {
        http_client: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
        api_base: server.url.clone(),
        auth_state: Mutex::new(None),
        pending_flow: Mutex::new(None),
        refresh_lock: tokio::sync::Mutex::new(()),
        tokens_path: scratch.0.join("auth.json"),
    })
}

fn persist(service: &GoodLuckAuthService, auth: &AuthState) {
    service
        .write_auth_to_disk(&auth.jwt, &auth.refresh_token, &auth.user, auth.expires_at)
        .unwrap();
}

#[tokio::test]
async fn expired_restored_session_survives_temporary_refresh_errors() {
    for status in [429, 503] {
        let server = MockRefresh::new(status, false);
        let scratch = Scratch::new();
        let auth = service(&server, &scratch);
        persist(&auth, &state("alice", "old-jwt", true));
        let saved = fs::read(&auth.tokens_path).unwrap();
        auth.try_restore_auth();
        assert!(auth.get_token().await.is_err());
        assert_eq!(server.count.load(Ordering::SeqCst), 1);
        assert_eq!(auth.get_user().unwrap().user_id, "alice");
        assert_eq!(
            auth.auth_state.lock().unwrap().as_ref().unwrap().jwt,
            "old-jwt"
        );
        assert_eq!(fs::read(&auth.tokens_path).unwrap(), saved);
    }
}

#[tokio::test]
async fn invalid_refresh_401_clears_restored_session() {
    let server = MockRefresh::new(401, false);
    let scratch = Scratch::new();
    let auth = service(&server, &scratch);
    persist(&auth, &state("alice", "old-jwt", true));
    auth.try_restore_auth();
    assert!(auth.get_token().await.is_err());
    assert_eq!(server.count.load(Ordering::SeqCst), 1);
    assert!(auth.get_user().is_none());
    assert!(!auth.tokens_path.exists());
}

#[tokio::test]
async fn concurrent_rejected_jwt_refreshes_once() {
    let mut server = MockRefresh::new(200, true);
    let scratch = Scratch::new();
    let auth = service(&server, &scratch);
    *auth.auth_state.lock().unwrap() = Some(state("alice", "old-jwt", false));
    let first_auth = auth.clone();
    let first = tokio::spawn(async move { first_auth.refresh_rejected_token("old-jwt").await });
    server.wait_for_request().await;
    let second = auth.refresh_rejected_token("old-jwt");
    tokio::pin!(second);
    // Poll while the first response is blocked, proving the calls overlap.
    assert!(futures_util::poll!(&mut second).is_pending());
    server.release.send(()).unwrap();
    first.await.unwrap().unwrap();
    second.await.unwrap();
    assert_eq!(auth.get_token().await.unwrap(), "new-jwt");
    assert_eq!(server.count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn delayed_refresh_cannot_replace_a_new_users_session() {
    for status in [200, 401] {
        let mut server = MockRefresh::new(status, true);
        let scratch = Scratch::new();
        let auth = service(&server, &scratch);
        *auth.auth_state.lock().unwrap() = Some(state("alice", "old-jwt", true));
        let refreshing = auth.clone();
        let request = tokio::spawn(async move { refreshing.get_token().await });
        server.wait_for_request().await;
        let replacement = state("bob", "bob-jwt", false);
        persist(&auth, &replacement);
        *auth.auth_state.lock().unwrap() = Some(replacement);
        let saved = fs::read(&auth.tokens_path).unwrap();
        server.release.send(()).unwrap();
        let _ = request.await.unwrap();
        assert_eq!(auth.get_user().unwrap().user_id, "bob");
        assert_eq!(auth.get_token().await.unwrap(), "bob-jwt");
        assert_eq!(fs::read(&auth.tokens_path).unwrap(), saved);
    }
}
