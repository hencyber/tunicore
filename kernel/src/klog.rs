//! Kernel Message Log - ring buffer for system events
//!
//! Captures kernel events like boot, agent lifecycle, errors.
//! Readable via `dmesg` intent command.

use spin::Mutex;
use crate::interrupts;

/// Max log entries
const LOG_SIZE: usize = 128;
/// Max message length
const MSG_LEN: usize = 96;

/// A kernel log entry
pub struct KlogEntry {
    pub tick: u64,
    pub level: LogLevel,
    msg: [u8; MSG_LEN],
    msg_len: usize,
    pub valid: bool,
}

/// Log severity
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Boot,
    Info,
    Warn,
    Agent,
    Error,
}

impl LogLevel {
    pub fn tag(&self) -> &'static str {
        match self {
            LogLevel::Boot => "BOOT",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Agent => "AGNT",
            LogLevel::Error => "ERR!",
        }
    }
}

impl KlogEntry {
    const fn empty() -> Self {
        Self {
            tick: 0,
            level: LogLevel::Info,
            msg: [0; MSG_LEN],
            msg_len: 0,
            valid: false,
        }
    }

    pub fn message(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.msg_len]).unwrap_or("?")
    }
}

/// Kernel ring buffer log
pub struct Klog {
    entries: [KlogEntry; LOG_SIZE],
    head: usize,
    total: u64,
}

impl Klog {
    const fn new() -> Self {
        const EMPTY: KlogEntry = KlogEntry::empty();
        Self {
            entries: [EMPTY; LOG_SIZE],
            head: 0,
            total: 0,
        }
    }

    /// Write a log entry
    pub fn log(&mut self, level: LogLevel, msg: &str) {
        let tick = interrupts::ticks();
        let entry = &mut self.entries[self.head];
        entry.tick = tick;
        entry.level = level;
        let bytes = msg.as_bytes();
        let n = bytes.len().min(MSG_LEN);
        entry.msg[..n].copy_from_slice(&bytes[..n]);
        entry.msg_len = n;
        entry.valid = true;

        self.head = (self.head + 1) % LOG_SIZE;
        self.total += 1;
    }

    /// Get last N entries (newest first)
    pub fn recent(&self, n: usize) -> impl Iterator<Item = &KlogEntry> {
        let count = n.min(LOG_SIZE).min(self.total as usize);
        let start = if self.total as usize >= LOG_SIZE {
            (self.head + LOG_SIZE - count) % LOG_SIZE
        } else {
            self.total as usize - count
        };

        (0..count).map(move |i| {
            let idx = (start + i) % LOG_SIZE;
            &self.entries[idx]
        })
    }

    /// Total entries ever logged
    pub fn total(&self) -> u64 {
        self.total
    }
}

/// Global kernel log
pub static KLOG: Mutex<Klog> = Mutex::new(Klog::new());

/// Convenience: log from anywhere
pub fn log(level: LogLevel, msg: &str) {
    KLOG.lock().log(level, msg);
}

pub fn boot(msg: &str) { log(LogLevel::Boot, msg); }
pub fn info(msg: &str) { log(LogLevel::Info, msg); }
pub fn warn(msg: &str) { log(LogLevel::Warn, msg); }
pub fn agent(msg: &str) { log(LogLevel::Agent, msg); }
pub fn error(msg: &str) { log(LogLevel::Error, msg); }
