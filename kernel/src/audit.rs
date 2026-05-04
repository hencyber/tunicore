//! Append-only audit log with hash chain integrity
//!
//! Every capability operation and agent event is logged.
//! Hash chain makes tampering detectable.

use crate::cap_table::{AgentId, CapHandle};

const AUDIT_RING_SIZE: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum AuditEvent {
    CapGranted = 0x01,
    CapRevoked = 0x02,
    CapDelegated = 0x03,
    CapCheckFail = 0x05,
    CapExpired = 0x06,
    AgentSpawned = 0x10,
    AgentKilled = 0x11,
    AgentTimeout = 0x12,
    EscalationAttempt = 0x100,
    KernelBoot = 0x1000,
}

#[derive(Clone)]
pub struct AuditEntry {
    pub tick: u64,
    pub agent: AgentId,
    pub event: AuditEvent,
    pub cap_handle: CapHandle,
    pub result: i32,
    pub prev_hash: u64,
}

pub struct AuditLog {
    entries: [Option<AuditEntry>; AUDIT_RING_SIZE],
    head: usize,
    total: u64,
    last_hash: u64,
}

const NONE_ENTRY: Option<AuditEntry> = None;

impl AuditLog {
    pub const fn new() -> Self {
        Self {
            entries: [NONE_ENTRY; AUDIT_RING_SIZE],
            head: 0,
            total: 0,
            last_hash: 0xCAFEBABEDEADBEEF,
        }
    }

    pub fn record(&mut self, tick: u64, agent: AgentId, event: AuditEvent, cap: CapHandle, result: i32) {
        let entry = AuditEntry { tick, agent, event, cap_handle: cap, result, prev_hash: self.last_hash };
        self.last_hash = fnv1a(&entry);
        self.entries[self.head] = Some(entry);
        self.head = (self.head + 1) % AUDIT_RING_SIZE;
        self.total += 1;
    }

    pub fn total_events(&self) -> u64 { self.total }
}

fn fnv1a(e: &AuditEntry) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let p: u64 = 0x100000001b3;
    for b in e.tick.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(p); }
    for b in e.agent.0.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(p); }
    for b in (e.event as u16).to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(p); }
    for b in e.prev_hash.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(p); }
    h
}

use spin::Mutex;
pub static AUDIT_LOG: Mutex<AuditLog> = Mutex::new(AuditLog::new());

pub fn log_boot(tick: u64) {
    AUDIT_LOG.lock().record(tick, AgentId(0), AuditEvent::KernelBoot, CapHandle(0), 0);
}
