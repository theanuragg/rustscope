use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::output::schema::LockRecord;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub struct LockId(u64);

static NEXT_LOCK_ID: AtomicU64 = AtomicU64::new(1);

impl LockId {
    pub fn new() -> Self {
        Self(NEXT_LOCK_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug)]
struct LockState {
    name: String,
    wait_ns: u64,
    hold_ns: u64,
    acquisitions: u64,
    contended_acquisitions: u64,
}

static LOCKS: Lazy<Mutex<HashMap<LockId, LockState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub struct WaitToken {
    id: LockId,
    start_ns: std::time::Instant,
}

pub struct HoldToken {
    id: LockId,
    start_ns: std::time::Instant,
    contended: bool,
}

pub fn register_lock(name: &str) -> LockId {
    let id = LockId::new();
    let mut m = LOCKS.lock();
    m.insert(
        id,
        LockState {
            name: name.to_owned(),
            wait_ns: 0,
            hold_ns: 0,
            acquisitions: 0,
            contended_acquisitions: 0,
        },
    );
    id
}

pub fn record_lock_wait_start(id: LockId) -> WaitToken {
    WaitToken {
        id,
        start_ns: std::time::Instant::now(),
    }
}

pub fn record_lock_acquired(token: WaitToken) -> HoldToken {
    let elapsed = token.start_ns.elapsed().as_nanos() as u64;
    let mut m = LOCKS.lock();
    if let Some(s) = m.get_mut(&token.id) {
        s.acquisitions += 1;
        if elapsed > 0 {
            s.wait_ns += elapsed;
            s.contended_acquisitions += 1;
        }
    }
    HoldToken {
        id: token.id,
        start_ns: std::time::Instant::now(),
        contended: elapsed > 0,
    }
}

pub fn record_lock_released(token: HoldToken) {
    let elapsed = token.start_ns.elapsed().as_nanos() as u64;
    let mut m = LOCKS.lock();
    if let Some(s) = m.get_mut(&token.id) {
        s.hold_ns += elapsed;
    }
}

pub fn snapshot() -> Vec<LockRecord> {
    let m = LOCKS.lock();
    m.values()
        .map(|s| LockRecord {
            name: s.name.clone(),
            contention_count: s.contended_acquisitions,
            total_wait_ns: s.wait_ns,
            max_wait_ns: 0, // Not tracked in LockState
            wait_ns: s.wait_ns,
            hold_ns: s.hold_ns,
            acquisitions: s.acquisitions,
            contended_acquisitions: s.contended_acquisitions,
        })
        .collect()
}

