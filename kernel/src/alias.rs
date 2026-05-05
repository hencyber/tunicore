//! Command Alias System — teach your OS new tricks
//!
//! Users define custom commands that expand to existing commands.
//! Stored in-kernel, persistent during session.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Max aliases
const MAX_ALIASES: usize = 32;

/// An alias definition
struct Alias {
    name: String,
    expansion: String,
}

/// Alias table
pub struct AliasTable {
    aliases: Vec<Alias>,
}

impl AliasTable {
    pub const fn new() -> Self {
        Self {
            aliases: Vec::new(),
        }
    }

    /// Define or update an alias
    pub fn define(&mut self, name: &str, expansion: &str) -> Result<(), &'static str> {
        if name.is_empty() { return Err("empty name"); }
        if expansion.is_empty() { return Err("empty expansion"); }
        if name.len() > 16 { return Err("name too long (max 16)"); }

        // Update existing
        for alias in self.aliases.iter_mut() {
            if alias.name == name {
                alias.expansion = String::from(expansion);
                return Ok(());
            }
        }

        if self.aliases.len() >= MAX_ALIASES {
            return Err("alias table full");
        }

        self.aliases.push(Alias {
            name: String::from(name),
            expansion: String::from(expansion),
        });
        Ok(())
    }

    /// Look up an alias
    pub fn resolve(&self, name: &str) -> Option<&str> {
        self.aliases.iter()
            .find(|a| a.name == name)
            .map(|a| a.expansion.as_str())
    }

    /// Remove an alias
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.aliases.len();
        self.aliases.retain(|a| a.name != name);
        self.aliases.len() < before
    }

    /// Iterate all aliases
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases.iter().map(|a| (a.name.as_str(), a.expansion.as_str()))
    }

    /// Count
    pub fn len(&self) -> usize {
        self.aliases.len()
    }
}

/// Global alias table
pub static ALIASES: Mutex<AliasTable> = Mutex::new(AliasTable::new());
