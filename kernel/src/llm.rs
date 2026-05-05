//! LLM Bridge — Serial-based AI query interface
//!
//! Protocol:
//!   Kernel → Host: \x02LLM:query text\x03
//!   Host → Kernel: \x02RSP:response text\x03
//!
//! The host runs tools/llm_bridge.py which monitors serial,
//! forwards to Gemini/OpenAI, and returns the response.

use alloc::string::String;
use crate::serial::{SERIAL};
use crate::serial_println;

/// STX (Start of Text) and ETX (End of Text) framing bytes
const STX: u8 = 0x02;
const ETX: u8 = 0x03;

/// Maximum response size (bytes)
const MAX_RESPONSE: usize = 1024;

/// Timeout: max iterations to wait for response
const TIMEOUT_ITERS: u64 = 5_000_000;

/// Send a query to the LLM bridge and wait for response
pub fn query(prompt: &str) -> Result<String, &'static str> {
    // Send framed query: STX + "LLM:" + prompt + ETX
    {
        let mut serial = SERIAL.lock();
        serial.write_byte(STX);
        for b in b"LLM:" { serial.write_byte(*b); }
        for b in prompt.as_bytes() { serial.write_byte(*b); }
        serial.write_byte(ETX);
    }

    // Wait for response: STX + "RSP:" + response + ETX
    let mut buf = [0u8; MAX_RESPONSE];
    let mut pos = 0usize;
    let mut state = WaitState::WaitSTX;
    let mut iters = 0u64;

    loop {
        iters += 1;
        if iters > TIMEOUT_ITERS {
            return Err("LLM timeout — no bridge running?");
        }

        let byte = {
            let mut serial = SERIAL.lock();
            serial.read_byte()
        };

        if let Some(b) = byte {
            match state {
                WaitState::WaitSTX => {
                    if b == STX { state = WaitState::WaitR; }
                }
                WaitState::WaitR => {
                    if b == b'R' { state = WaitState::WaitS; }
                    else { state = WaitState::WaitSTX; }
                }
                WaitState::WaitS => {
                    if b == b'S' { state = WaitState::WaitP; }
                    else { state = WaitState::WaitSTX; }
                }
                WaitState::WaitP => {
                    if b == b'P' { state = WaitState::WaitColon; }
                    else { state = WaitState::WaitSTX; }
                }
                WaitState::WaitColon => {
                    if b == b':' { state = WaitState::ReadBody; }
                    else { state = WaitState::WaitSTX; }
                }
                WaitState::ReadBody => {
                    if b == ETX {
                        // Done — convert to string
                        let text = core::str::from_utf8(&buf[..pos])
                            .unwrap_or("[invalid utf8]");
                        return Ok(String::from(text));
                    }
                    if pos < MAX_RESPONSE {
                        buf[pos] = b;
                        pos += 1;
                    }
                }
            }
        } else {
            // No data — spin briefly
            for _ in 0..100 { core::hint::spin_loop(); }
        }
    }
}

#[derive(Clone, Copy)]
enum WaitState {
    WaitSTX,
    WaitR,
    WaitS,
    WaitP,
    WaitColon,
    ReadBody,
}
