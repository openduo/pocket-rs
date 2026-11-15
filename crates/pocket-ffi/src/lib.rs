// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
//! Stable C ABI. Handles are validated IDs; no Rust object layout crosses the boundary.
use pocket_core::{
    Error,
    api::{Api, RequestCommand, pair_request},
    audio::{PacketStream, frame_packets},
    pairing::{PairedDevice, PairingLink},
    session::{Command, Session, SessionConfig},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, OnceLock},
};

const MAX_JSON_INPUT: usize = 24 * 1024 * 1024;
#[derive(Default)]
struct Registry {
    next: u64,
    sessions: HashMap<u64, Session>,
}
static SESSIONS: OnceLock<Mutex<Registry>> = OnceLock::new();

#[repr(C)]
pub struct PocketBuffer {
    pub data: *mut u8,
    pub len: usize,
}
#[repr(C)]
pub struct PocketResult {
    pub status: i32,
    pub handle: u64,
    pub payload: PocketBuffer,
}
impl PocketResult {
    fn json(status: i32, handle: u64, value: Value) -> Self {
        let bytes = serde_json::to_vec(&value)
            .unwrap_or_else(|_| br#"{"code":"internal_error"}"#.to_vec())
            .into_boxed_slice();
        let len = bytes.len();
        Self {
            status,
            handle,
            payload: PocketBuffer {
                data: Box::into_raw(bytes).cast::<u8>(),
                len,
            },
        }
    }
    fn error(error: Error) -> Self {
        Self::json(1, 0, json!({"error":error}))
    }
}
fn guard(f: impl FnOnce() -> pocket_core::Result<PocketResult>) -> PocketResult {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => PocketResult::error(error),
        Err(_) => PocketResult::error(Error::new(
            "internal_error",
            "Rust panic contained at ABI boundary",
        )),
    }
}
unsafe fn input<'a>(data: *const u8, len: usize) -> pocket_core::Result<&'a [u8]> {
    if len > MAX_JSON_INPUT || (len > 0 && data.is_null()) {
        return Err(Error::invalid("Null input or input limit exceeded"));
    }
    if len == 0 {
        return Ok(&[]);
    }
    // SAFETY: the ABI caller guarantees readable memory for len bytes during this call.
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}
fn with_session<T>(
    handle: u64,
    f: impl FnOnce(&mut Session) -> pocket_core::Result<T>,
) -> pocket_core::Result<T> {
    let mut registry = SESSIONS
        .get_or_init(|| Mutex::new(Registry::default()))
        .lock()
        .map_err(|_| Error::new("internal_error", "Session lock poisoned"))?;
    let session = registry
        .sessions
        .get_mut(&handle)
        .ok_or_else(|| Error::new("invalid_handle", "Unknown or closed session"))?;
    f(session)
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ProtocolCall {
    ParsePairing {
        text: String,
    },
    PairRequest {
        code: String,
    },
    DecodePair {
        body: Vec<u8>,
    },
    FrameVoice {
        packets: Vec<Vec<u8>>,
    },
    ParseVoice {
        body: Vec<u8>,
    },
    Request {
        config: SessionConfig,
        request: RequestCommand,
    },
}

#[unsafe(no_mangle)]
pub extern "C" fn pocket_abi_version() -> u32 {
    1
}

/// # Safety
/// `data` must be readable for `len` bytes until return; null is allowed only for zero length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_protocol_call(data: *const u8, len: usize) -> PocketResult {
    guard(|| {
        let call: ProtocolCall = serde_json::from_slice(unsafe { input(data, len)? })?;
        let value = match call {
            ProtocolCall::ParsePairing { text } => {
                serde_json::to_value(PairingLink::parse(&text)?)?
            }
            ProtocolCall::PairRequest { code } => serde_json::to_value(pair_request(&code)?)?,
            ProtocolCall::DecodePair { body } => {
                serde_json::to_value(PairedDevice::decode(&body)?)?
            }
            ProtocolCall::FrameVoice { packets } => json!({"body":frame_packets(&packets)?}),
            ProtocolCall::ParseVoice { body } => {
                let mut parser = PacketStream::default();
                let packets = parser.push(&body)?;
                parser.finish()?;
                json!({"packets":packets})
            }
            ProtocolCall::Request { config, request } => serde_json::to_value(
                request.build(&Api::new(config.endpoint, &config.room, &config.token)?)?,
            )?,
        };
        Ok(PocketResult::json(0, 0, value))
    })
}

/// # Safety
/// `data` must be readable for `len` bytes until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_session_create(data: *const u8, len: usize) -> PocketResult {
    guard(|| {
        let config: SessionConfig = serde_json::from_slice(unsafe { input(data, len)? })?;
        let session = Session::new(config)?;
        let mut registry = SESSIONS
            .get_or_init(|| Mutex::new(Registry::default()))
            .lock()
            .map_err(|_| Error::new("internal_error", "Session lock poisoned"))?;
        if registry.sessions.len() >= 256 {
            return Err(Error::new("limit_exceeded", "Too many open sessions"));
        }
        registry.next = registry
            .next
            .checked_add(1)
            .ok_or_else(|| Error::new("limit_exceeded", "Handle space exhausted"))?;
        let handle = registry.next;
        registry.sessions.insert(handle, session);
        Ok(PocketResult::json(
            0,
            handle,
            json!({"abi":1, "version":pocket_core::VERSION}),
        ))
    })
}

/// # Safety
/// `data` must be readable for `len` bytes until return. Calls on a handle are serialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_session_dispatch(
    handle: u64,
    data: *const u8,
    len: usize,
) -> PocketResult {
    guard(|| {
        let command: Command = serde_json::from_slice(unsafe { input(data, len)? })?;
        let value = with_session(handle, |session| {
            let effects = session.dispatch(command)?;
            Ok(json!({"effects":effects, "snapshot":session.snapshot()}))
        })?;
        Ok(PocketResult::json(0, handle, value))
    })
}

/// Binary packets avoid JSON/base64 encoding on the per-frame input path.
/// # Safety
/// `data` must be readable for `len` bytes until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_session_binary(
    handle: u64,
    generation: u64,
    data: *const u8,
    len: usize,
) -> PocketResult {
    guard(|| {
        if len > 1276 {
            return Err(Error::invalid("Live binary frame too large"));
        }
        let bytes = unsafe { input(data, len)? }.to_vec();
        let value = with_session(handle, |session| {
            let effects = session.dispatch(Command::LiveBinary { generation, bytes })?;
            Ok(json!({"effects":effects, "snapshot":session.snapshot()}))
        })?;
        Ok(PocketResult::json(0, handle, value))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn pocket_session_close(handle: u64) -> PocketResult {
    guard(|| {
        let mut registry = SESSIONS
            .get_or_init(|| Mutex::new(Registry::default()))
            .lock()
            .map_err(|_| Error::new("internal_error", "Session lock poisoned"))?;
        let mut session = registry
            .sessions
            .remove(&handle)
            .ok_or_else(|| Error::new("invalid_handle", "Unknown or closed session"))?;
        let effects = session.dispatch(Command::Disconnect)?;
        Ok(PocketResult::json(0, 0, json!({"effects":effects})))
    })
}

/// # Safety
/// Pass each result returned by this library exactly once, without changing its pointer/length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_result_free(result: PocketResult) {
    if !result.payload.data.is_null() {
        // SAFETY: callers return the same boxed-slice pointer and length allocated in json().
        drop(unsafe {
            Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                result.payload.data,
                result.payload.len,
            ))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn take(result: PocketResult) -> (i32, u64, Value) {
        let value = serde_json::from_slice(unsafe {
            std::slice::from_raw_parts(result.payload.data, result.payload.len)
        })
        .unwrap();
        let status = result.status;
        let handle = result.handle;
        unsafe {
            pocket_result_free(result);
        }
        (status, handle, value)
    }
    #[test]
    fn null_malformed_and_retired_handles_are_errors() {
        unsafe {
            assert_eq!(take(pocket_protocol_call(std::ptr::null(), 1)).0, 1);
            assert_eq!(take(pocket_protocol_call(b"{".as_ptr(), 1)).0, 1);
            assert_eq!(
                take(pocket_session_binary(u64::MAX, 0, b"a".as_ptr(), 1)).2["error"]["code"],
                "invalid_handle"
            );
            assert_eq!(take(pocket_session_close(u64::MAX)).0, 1);
        }
    }
    #[test]
    fn concurrent_consumers_have_independent_owned_sessions() {
        let consumers: Vec<_> = (0..4).map(|_| std::thread::spawn(|| unsafe {
            let config = br#"{"endpoint":{"host":"example.test","port":443,"tls":true},"room":"prepared","token":"fixture-token"}"#;
            let (status, handle, _) = take(pocket_session_create(config.as_ptr(), config.len())); assert_eq!(status, 0);
            let command = br#"{"type":"connect"}"#;
            let result = take(pocket_session_dispatch(handle, command.as_ptr(), command.len()));
            assert_eq!(result.2["snapshot"]["generation"], 1);
            assert_eq!(take(pocket_session_close(handle)).0, 0);
            assert_eq!(take(pocket_session_close(handle)).0, 1);
        })).collect();
        for consumer in consumers {
            consumer.join().unwrap();
        }
    }
}
