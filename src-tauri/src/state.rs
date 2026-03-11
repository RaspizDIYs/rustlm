use std::sync::Mutex;

pub struct AppState {
    pub initialized: Mutex<bool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            initialized: Mutex::new(false),
        }
    }
}
