//! Kernel Environment Store - key-value configuration
//!
//! Persistent (in-memory) config store for the OS.
//! Readable by agents via tc.env_get, settable via shell.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Max entries
const MAX_ENTRIES: usize = 64;
/// Max key length
const MAX_KEY: usize = 32;
/// Max value length
const MAX_VAL: usize = 128;

/// A key-value entry
struct EnvEntry {
    key: String,
    value: String,
}

/// Environment store
pub struct EnvStore {
    entries: Vec<EnvEntry>,
}

impl EnvStore {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Set a key-value pair (insert or update)
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), &'static str> {
        if key.len() > MAX_KEY { return Err("key too long"); }
        if value.len() > MAX_VAL { return Err("value too long"); }

        // Update existing
        for entry in self.entries.iter_mut() {
            if entry.key == key {
                entry.value = String::from(value);
                return Ok(());
            }
        }

        // Insert new
        if self.entries.len() >= MAX_ENTRIES {
            return Err("env full");
        }
        self.entries.push(EnvEntry {
            key: String::from(key),
            value: String::from(value),
        });
        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }

    /// Remove a key
    pub fn unset(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.key != key);
        self.entries.len() < before
    }

    /// Iterate all entries
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|e| (e.key.as_str(), e.value.as_str()))
    }

    /// Number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Global environment store
pub static ENV: Mutex<EnvStore> = Mutex::new(EnvStore::new());

/// Initialize default environment variables
pub fn init_defaults() {
    let mut env = ENV.lock();
    env.set("hostname", "tunicore").ok();
    env.set("version", "0.6.0").ok();
    env.set("arch", "x86_64").ok();
    env.set("lang", "sv-SE").ok();
    env.set("shell", "intent/v2").ok();
    env.set("heap_mb", "32").ok();
}
