//! Inter-agent communication channels
//!
//! Capability-gated message passing between agents.
//! A channel is a fixed-size ring buffer — sender needs WRITE cap,
//! receiver needs READ cap. This is TuniCore's IPC primitive.

use alloc::vec::Vec;
use spin::Mutex;

use crate::cap_table::AgentId;

/// Maximum message size in bytes
const MAX_MSG_SIZE: usize = 256;

/// Maximum messages in a channel buffer
const CHANNEL_CAPACITY: usize = 64;

/// Maximum channels in the system
const MAX_CHANNELS: usize = 128;

/// A single message in a channel
#[derive(Clone)]
pub struct Message {
    /// Sender agent ID
    pub sender: AgentId,
    /// Message payload
    pub data: [u8; MAX_MSG_SIZE],
    /// Actual length of data
    pub len: usize,
    /// Tick when sent
    pub tick: u64,
}

impl Message {
    pub fn new(sender: AgentId, payload: &[u8], tick: u64) -> Self {
        let mut data = [0u8; MAX_MSG_SIZE];
        let len = payload.len().min(MAX_MSG_SIZE);
        data[..len].copy_from_slice(&payload[..len]);
        Message { sender, data, len, tick }
    }

    pub fn payload(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// A channel — ring buffer of messages
pub struct Channel {
    /// Channel ID
    pub id: u64,
    /// Ring buffer
    buffer: Vec<Option<Message>>,
    /// Write position
    head: usize,
    /// Read position
    tail: usize,
    /// Number of messages in buffer
    count: usize,
}

impl Channel {
    pub fn new(id: u64) -> Self {
        let mut buffer = Vec::with_capacity(CHANNEL_CAPACITY);
        for _ in 0..CHANNEL_CAPACITY {
            buffer.push(None);
        }
        Channel { id, buffer, head: 0, tail: 0, count: 0 }
    }

    /// Send a message into the channel
    pub fn send(&mut self, msg: Message) -> Result<(), &'static str> {
        if self.count >= CHANNEL_CAPACITY {
            return Err("channel full");
        }
        self.buffer[self.head] = Some(msg);
        self.head = (self.head + 1) % CHANNEL_CAPACITY;
        self.count += 1;
        Ok(())
    }

    /// Receive a message from the channel
    pub fn recv(&mut self) -> Option<Message> {
        if self.count == 0 {
            return None;
        }
        let msg = self.buffer[self.tail].take();
        self.tail = (self.tail + 1) % CHANNEL_CAPACITY;
        self.count -= 1;
        msg
    }

    /// Check if channel has messages
    pub fn has_messages(&self) -> bool {
        self.count > 0
    }

    /// Get current message count
    pub fn message_count(&self) -> usize {
        self.count
    }
}

/// Channel registry — manages all channels
pub struct ChannelRegistry {
    channels: Vec<Channel>,
    next_id: u64,
}

impl ChannelRegistry {
    pub const fn new() -> Self {
        Self { channels: Vec::new(), next_id: 0 }
    }

    /// Create a new channel, returns channel ID
    pub fn create(&mut self) -> Result<u64, &'static str> {
        if self.channels.len() >= MAX_CHANNELS {
            return Err("max channels reached");
        }
        let id = self.next_id;
        self.next_id += 1;
        self.channels.push(Channel::new(id));
        Ok(id)
    }

    /// Get a channel by ID
    pub fn get(&self, id: u64) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// Get a mutable channel by ID
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }
}

/// Global channel registry
pub static CHANNELS: Mutex<ChannelRegistry> = Mutex::new(ChannelRegistry::new());
