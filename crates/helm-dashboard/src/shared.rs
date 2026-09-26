use std::sync::{Arc, Mutex};

use crate::snapshot::TickSnapshot;
use crate::wire::{SessionInfo, HISTORY_CAP};

#[derive(Clone)]
pub struct DashboardShared {
    inner: Arc<Inner>,
}

struct Inner {
    session: SessionInfo,
    history: Mutex<Vec<TickSnapshot>>,
}

impl DashboardShared {
    pub fn new(session: SessionInfo) -> Self {
        Self {
            inner: Arc::new(Inner {
                session,
                history: Mutex::new(Vec::with_capacity(HISTORY_CAP.min(64))),
            }),
        }
    }

    pub fn session(&self) -> &SessionInfo {
        &self.inner.session
    }

    pub fn push_history(&self, snapshot: TickSnapshot) {
        let mut guard = self.inner.history.lock().expect("history lock");
        guard.push(snapshot);
        if guard.len() > HISTORY_CAP {
            let drop = guard.len() - HISTORY_CAP;
            guard.drain(0..drop);
        }
    }

    pub fn history(&self) -> Vec<TickSnapshot> {
        self.inner.history.lock().expect("history lock").clone()
    }
}
