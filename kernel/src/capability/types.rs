//! Capability type definitions
//!
//! Defines the type-safe capability model using Rust's type system.
//! Each capability type is a zero-sized marker type, ensuring that
//! capabilities for different resource kinds cannot be confused.

use core::marker::PhantomData;
use core::sync::atomic::{AtomicU64, Ordering};

use bitflags::bitflags;

// ─── Capability ID ──────────────────────────────────────────────

/// Globally unique capability identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapId(u64);

/// Monotonic counter for generating unique capability IDs
static NEXT_CAP_ID: AtomicU64 = AtomicU64::new(1);

impl CapId {
    /// Generate a new unique capability ID
    pub fn new() -> Self {
        Self(NEXT_CAP_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the raw ID value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

// ─── Rights ─────────────────────────────────────────────────────

bitflags! {
    /// Access rights that can be granted via a capability
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Rights: u32 {
        /// Permission to read data
        const READ    = 0b0000_0001;
        /// Permission to write/modify data
        const WRITE   = 0b0000_0010;
        /// Permission to execute code
        const EXECUTE = 0b0000_0100;
        /// Permission to delegate this capability to a sub-agent
        const GRANT   = 0b0000_1000;
        /// Permission to create new resources of this type
        const CREATE  = 0b0001_0000;
        /// Permission to destroy/delete resources
        const DELETE  = 0b0010_0000;
    }
}

// ─── Capability Marker Types ────────────────────────────────────

/// Memory access capability — controls allocation, read, write
pub struct CapMemory;

/// Port I/O capability — controls access to hardware ports
pub struct CapIO;

/// Code execution capability — controls what code an agent can run
pub struct CapExec;

/// Network capability — controls network socket creation and I/O
pub struct CapNet;

/// Filesystem capability — controls file/directory access
pub struct CapFS;

/// Agent meta-capability — controls what an AI agent is allowed to do
/// This is the highest-level capability, governing agent behavior
pub struct CapAgent;

/// IRQ capability — controls which interrupts an agent can handle
pub struct CapIRQ;

/// Device capability — controls access to hardware devices (GPU, NPU, etc.)
pub struct CapDevice;

// ─── Capability Token ───────────────────────────────────────────

/// A capability token granting specific rights over a typed resource.
///
/// The type parameter `T` determines what kind of resource this
/// capability controls, preventing confusion between e.g. a memory
/// capability and a network capability at compile time.
///
/// ## Example (future usage)
///
/// ```rust,ignore
/// // Agent gets a read-only memory capability
/// let mem_cap = Capability::<CapMemory>::new(Rights::READ);
///
/// // Agent gets a network capability with read+write
/// let net_cap = Capability::<CapNet>::new(Rights::READ | Rights::WRITE);
///
/// // Attempting to use mem_cap as a net_cap is a compile error!
/// ```
#[derive(Debug)]
pub struct Capability<T> {
    /// Unique identifier for this capability
    id: CapId,
    /// Rights granted by this capability
    rights: Rights,
    /// Zero-sized marker for the capability type
    _type: PhantomData<T>,
}

impl<T> Capability<T> {
    /// Create a new capability with the given rights
    pub fn new(rights: Rights) -> Self {
        Self {
            id: CapId::new(),
            rights,
            _type: PhantomData,
        }
    }

    /// Get this capability's unique ID
    pub fn id(&self) -> CapId {
        self.id
    }

    /// Get the rights granted by this capability
    pub fn rights(&self) -> Rights {
        self.rights
    }

    /// Check if this capability grants a specific right
    pub fn has_right(&self, right: Rights) -> bool {
        self.rights.contains(right)
    }

    /// Create an attenuated (reduced-rights) copy for delegation.
    /// The new capability can only have equal or fewer rights.
    /// Returns None if the requested rights exceed our own.
    pub fn attenuate(&self, new_rights: Rights) -> Option<Capability<T>> {
        if self.rights.contains(new_rights) {
            Some(Capability {
                id: CapId::new(),
                rights: new_rights,
                _type: PhantomData,
            })
        } else {
            None // Cannot escalate privileges
        }
    }
}
