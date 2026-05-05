//! VirtFS — In-memory virtual filesystem
//!
//! Capability-gated file storage for agents.
//! Files live in kernel memory — no disk required.
//! Every file operation is auditable via the capability system.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Max files in the filesystem
const MAX_FILES: usize = 256;

/// Max file size (64 KiB)
const MAX_FILE_SIZE: usize = 64 * 1024;

/// A file in VirtFS
pub struct VirtFile {
    pub name: String,
    pub data: Vec<u8>,
    pub created_at: u64,
    pub modified_at: u64,
}

impl VirtFile {
    pub fn new(name: &str, tick: u64) -> Self {
        Self {
            name: String::from(name),
            data: Vec::new(),
            created_at: tick,
            modified_at: tick,
        }
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// The virtual filesystem
pub struct VirtFs {
    files: Vec<VirtFile>,
}

impl VirtFs {
    pub const fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// List all files
    pub fn list(&self) -> &[VirtFile] {
        &self.files
    }

    /// Get file count
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Check if a file exists
    pub fn exists(&self, name: &str) -> bool {
        self.files.iter().any(|f| f.name == name)
    }

    /// Read file contents
    pub fn read(&self, name: &str) -> Option<&[u8]> {
        self.files.iter().find(|f| f.name == name).map(|f| f.data.as_slice())
    }

    /// Create or overwrite a file
    pub fn write(&mut self, name: &str, data: &[u8], tick: u64) -> Result<(), &'static str> {
        if data.len() > MAX_FILE_SIZE {
            return Err("file too large (max 64 KiB)");
        }

        // Update existing
        if let Some(f) = self.files.iter_mut().find(|f| f.name == name) {
            f.data = Vec::from(data);
            f.modified_at = tick;
            return Ok(());
        }

        // Create new
        if self.files.len() >= MAX_FILES {
            return Err("filesystem full (max 256 files)");
        }

        let mut file = VirtFile::new(name, tick);
        file.data = Vec::from(data);
        self.files.push(file);
        Ok(())
    }

    /// Create empty file (touch)
    pub fn touch(&mut self, name: &str, tick: u64) -> Result<(), &'static str> {
        if self.exists(name) {
            // Update modified time
            if let Some(f) = self.files.iter_mut().find(|f| f.name == name) {
                f.modified_at = tick;
            }
            return Ok(());
        }
        if self.files.len() >= MAX_FILES {
            return Err("filesystem full");
        }
        self.files.push(VirtFile::new(name, tick));
        Ok(())
    }

    /// Append data to a file (creates if not exists)
    pub fn append(&mut self, name: &str, data: &[u8], tick: u64) -> Result<(), &'static str> {
        if let Some(f) = self.files.iter_mut().find(|f| f.name == name) {
            if f.data.len() + data.len() > MAX_FILE_SIZE {
                return Err("file would exceed max size");
            }
            f.data.extend_from_slice(data);
            f.modified_at = tick;
            return Ok(());
        }
        // Create new
        self.write(name, data, tick)
    }

    /// Delete a file
    pub fn remove(&mut self, name: &str) -> Result<(), &'static str> {
        let pos = self.files.iter().position(|f| f.name == name);
        match pos {
            Some(i) => {
                self.files.remove(i);
                Ok(())
            }
            None => Err("file not found"),
        }
    }

    /// Total bytes used
    pub fn total_size(&self) -> usize {
        self.files.iter().map(|f| f.data.len()).sum()
    }
}

/// Global filesystem
pub static FS: Mutex<VirtFs> = Mutex::new(VirtFs::new());
