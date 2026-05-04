//! Kernel Capability Table
//!
//! The kernel owns all capabilities. Agents only see opaque handles.
//! This is the enforcement layer — the "guard" in "the kernel is the guard."
//!
//! Key properties:
//! - Capabilities have expiry times (auto-revocation)
//! - Delegation creates child capabilities that are revoked with parents
//! - Every check is O(1) via handle indexing

use crate::capability::types::{CapId, Rights};
use crate::resource::ResourceRef;

/// Maximum number of active capabilities in the system
const MAX_CAPABILITIES: usize = 4096;

/// Opaque handle that agents use to reference capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapHandle(pub u32);

/// Unique agent identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentId(pub u32);

/// Kernel-internal capability entry
#[derive(Debug)]
pub struct CapEntry {
    /// Unique capability ID (globally unique, monotonic)
    pub id: CapId,
    /// What resource this grants access to
    pub resource: ResourceRef,
    /// What rights are granted
    pub rights: Rights,
    /// Which agent owns this capability
    pub owner: AgentId,
    /// Parent capability (for delegation chain revocation)
    pub parent: Option<CapHandle>,
    /// Tick at which this capability expires (0 = never)
    pub expires_at: u64,
    /// Has this capability been revoked?
    pub revoked: bool,
    /// Tick when this capability was created (for audit)
    pub created_at: u64,
}

/// Error types for capability operations
#[derive(Debug)]
pub enum CapError {
    /// No free slots in the capability table
    TableFull,
    /// Handle does not reference a valid capability
    InvalidHandle,
    /// Capability has been revoked
    Revoked,
    /// Capability has expired
    Expired,
    /// Insufficient rights for the requested operation
    InsufficientRights,
    /// Cannot escalate: requested rights exceed parent
    EscalationDenied,
    /// Agent does not own this capability
    NotOwner,
}

/// The kernel capability table
pub struct CapTable {
    /// Capability entries (None = free slot)
    entries: [Option<CapEntry>; MAX_CAPABILITIES],
    /// Number of active (non-revoked) capabilities
    active_count: usize,
}

impl CapTable {
    /// Create a new empty capability table
    pub const fn new() -> Self {
        // const-init: array of None
        const NONE_ENTRY: Option<CapEntry> = None;
        Self {
            entries: [NONE_ENTRY; MAX_CAPABILITIES],
            active_count: 0,
        }
    }

    /// Grant a new capability to an agent
    pub fn grant(
        &mut self,
        agent: AgentId,
        resource: ResourceRef,
        rights: Rights,
        expires_at: u64,
        current_tick: u64,
    ) -> Result<CapHandle, CapError> {
        // Find a free slot
        let slot = self
            .entries
            .iter()
            .position(|e| e.is_none())
            .ok_or(CapError::TableFull)?;

        self.entries[slot] = Some(CapEntry {
            id: CapId::new(),
            resource,
            rights,
            owner: agent,
            parent: None,
            expires_at,
            revoked: false,
            created_at: current_tick,
        });

        self.active_count += 1;
        Ok(CapHandle(slot as u32))
    }

    /// Check if a handle is valid and has the required rights
    pub fn check(
        &self,
        handle: CapHandle,
        required: Rights,
        current_tick: u64,
    ) -> Result<&CapEntry, CapError> {
        let entry = self
            .entries
            .get(handle.0 as usize)
            .and_then(|e| e.as_ref())
            .ok_or(CapError::InvalidHandle)?;

        if entry.revoked {
            return Err(CapError::Revoked);
        }

        if entry.expires_at > 0 && current_tick >= entry.expires_at {
            return Err(CapError::Expired);
        }

        if !entry.rights.contains(required) {
            return Err(CapError::InsufficientRights);
        }

        Ok(entry)
    }

    /// Revoke a capability and ALL its delegated children
    pub fn revoke(&mut self, handle: CapHandle) -> Result<(), CapError> {
        // Check the entry exists
        let entry = self
            .entries
            .get(handle.0 as usize)
            .and_then(|e| e.as_ref())
            .ok_or(CapError::InvalidHandle)?;

        if entry.revoked {
            return Ok(()); // Already revoked
        }

        // Revoke this entry
        if let Some(entry) = self.entries[handle.0 as usize].as_mut() {
            entry.revoked = true;
            self.active_count = self.active_count.saturating_sub(1);
        }

        // Cascade: revoke all children (capabilities whose parent is this handle)
        for i in 0..MAX_CAPABILITIES {
            if let Some(child) = &self.entries[i] {
                if child.parent == Some(handle) && !child.revoked {
                    // Recursive revocation via iterative cascade
                    let child_handle = CapHandle(i as u32);
                    let _ = self.revoke(child_handle);
                }
            }
        }

        Ok(())
    }

    /// Delegate: create a child capability with equal or fewer rights
    pub fn delegate(
        &mut self,
        parent_handle: CapHandle,
        new_owner: AgentId,
        new_rights: Rights,
        current_tick: u64,
    ) -> Result<CapHandle, CapError> {
        // Check parent is valid
        let parent = self.check(parent_handle, new_rights, current_tick)?;

        // Cannot escalate
        if !parent.rights.contains(new_rights) {
            return Err(CapError::EscalationDenied);
        }

        let resource = parent.resource.clone();
        let expires_at = parent.expires_at; // Inherit parent's expiry

        // Find free slot
        let slot = self
            .entries
            .iter()
            .position(|e| e.is_none())
            .ok_or(CapError::TableFull)?;

        self.entries[slot] = Some(CapEntry {
            id: CapId::new(),
            resource,
            rights: new_rights,
            owner: new_owner,
            parent: Some(parent_handle),
            expires_at,
            revoked: false,
            created_at: current_tick,
        });

        self.active_count += 1;
        Ok(CapHandle(slot as u32))
    }

    /// Revoke ALL capabilities owned by an agent (used when killing agents)
    pub fn revoke_all_for_agent(&mut self, agent: AgentId) {
        for i in 0..MAX_CAPABILITIES {
            if let Some(entry) = &self.entries[i] {
                if entry.owner == agent && !entry.revoked {
                    let _ = self.revoke(CapHandle(i as u32));
                }
            }
        }
    }

    /// Get current number of active capabilities
    pub fn active_count(&self) -> usize {
        self.active_count
    }

    /// Garbage collect expired capabilities
    pub fn gc_expired(&mut self, current_tick: u64) {
        for i in 0..MAX_CAPABILITIES {
            if let Some(entry) = &self.entries[i] {
                if entry.expires_at > 0 && current_tick >= entry.expires_at && !entry.revoked {
                    let _ = self.revoke(CapHandle(i as u32));
                }
            }
        }
    }
}

/// Global capability table (kernel-owned, spinlock-protected)
use spin::Mutex;
pub static CAP_TABLE: Mutex<CapTable> = Mutex::new(CapTable::new());
