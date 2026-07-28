use crate::domain::engine::HaloEngine;
use std::sync::Mutex;

pub const ROUND_COMPLETE_HOLD_MS: u64 = 300_000;

pub struct AppState {
    pub(crate) engine: Mutex<HaloEngine>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            engine: Mutex::new(HaloEngine::new(ROUND_COMPLETE_HOLD_MS)),
        }
    }
}
