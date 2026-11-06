use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Lifecycle {
    closing: AtomicBool,
}

impl Lifecycle {
    pub fn begin_close(&self) -> bool {
        !self.closing.swap(true, Ordering::SeqCst)
    }

    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_rejects_new_work_and_repeated_close_without_affecting_other_instances() {
        let lifecycle = Lifecycle::default();
        let other = Lifecycle::default();
        assert!(!lifecycle.is_closing());
        assert!(lifecycle.begin_close());
        assert!(lifecycle.is_closing());
        assert!(!lifecycle.begin_close());
        assert!(!other.is_closing());
        assert!(other.begin_close());
    }
}
