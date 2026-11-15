// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
//! Stable C ABI. Handles are validated IDs; no Rust object layout crosses the boundary.
mod component;
use pocket_core::{
    Error,
    api::{Api, RequestCommand, pair_request},
    audio::{PacketStream, frame_packets},
    pairing::{PairedDevice, PairingLink},
    retry::{RedialBackoff, VoiceRetry},
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
    components: HashMap<u64, component::Component>,
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
unsafe fn binary_input<'a>(data: *const u8, len: usize) -> pocket_core::Result<&'a [u8]> {
    if len > 64 * 1024 * 1024 || (len > 0 && data.is_null()) {
        return Err(Error::invalid("Null input or binary body limit exceeded"));
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
    Endpoint {
        host: String,
        port: u16,
        tls: bool,
    },
    Query {
        text: String,
    },
    VoiceRetry {
        state: Option<VoiceRetry>,
        now_ms: u64,
        budget_ms: u64,
        next: bool,
    },
    Redial {
        state: RedialBackoff,
        failed: bool,
        reset: bool,
    },
    ParsePairing {
        text: String,
    },
    PairRequest {
        code: String,
    },
    DecodePair {
        body: Vec<u8>,
    },
    DecodeState {
        frame: Value,
    },
    DecodeLive {
        frame: Value,
    },
    Uploaded {
        attachment: pocket_core::api::Attachment,
        digest: String,
        frame: Value,
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
            ProtocolCall::Endpoint { host, port, tls } => {
                serde_json::to_value(pocket_core::pairing::Endpoint::new(&host, port, tls)?)?
            }
            ProtocolCall::Query { text } => json!({"query":pocket_core::api::query(&text)}),
            ProtocolCall::VoiceRetry {
                state,
                now_ms,
                budget_ms,
                next,
            } => {
                let mut retry = state.unwrap_or_else(|| VoiceRetry::new(now_ms, budget_ms));
                let delay = if next { retry.failed(now_ms) } else { None };
                json!({"remaining_ms":retry.remaining(now_ms), "delay_ms":delay, "state":retry})
            }
            ProtocolCall::Redial {
                mut state,
                failed,
                reset,
            } => {
                if reset {
                    state.reset();
                }
                let delay = if failed { state.failed() } else { 0 };
                json!({"delay_ms":delay, "sustained":state.sustained(), "state":state})
            }
            ProtocolCall::ParsePairing { text } => {
                serde_json::to_value(PairingLink::parse(&text)?)?
            }
            ProtocolCall::PairRequest { code } => serde_json::to_value(pair_request(&code)?)?,
            ProtocolCall::DecodePair { body } => {
                serde_json::to_value(PairedDevice::decode(&body)?)?
            }
            ProtocolCall::DecodeState { frame } => serde_json::to_value(
                pocket_core::api::RoomState::decode(&serde_json::to_vec(&frame)?)?,
            )?,
            ProtocolCall::DecodeLive { frame } => {
                serde_json::to_value(pocket_core::live::LiveEvent::decode(&frame)?)?
            }
            ProtocolCall::Uploaded {
                attachment,
                digest,
                frame,
            } => serde_json::to_value(pocket_core::api::uploaded_digest(
                attachment, &digest, &frame,
            )?)?,
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
        if registry.sessions.len() + registry.components.len() >= 256 {
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

/// Build a request without JSON-encoding a potentially large voice/upload body.
/// # Safety
/// Both inputs must be readable for their lengths until return. Null is valid for zero length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_request_build(
    data: *const u8,
    len: usize,
    body: *const u8,
    body_len: usize,
) -> PocketResult {
    guard(|| {
        #[derive(Deserialize)]
        struct Build {
            config: SessionConfig,
            request: RequestCommand,
        }
        let build: Build = serde_json::from_slice(unsafe { input(data, len)? })?;
        let bytes = unsafe { binary_input(body, body_len)? };
        let api = Api::new(
            build.config.endpoint,
            &build.config.room,
            &build.config.token,
        )?;
        let (request, input_body) = match build.request {
            RequestCommand::Voice {
                id,
                source,
                surface,
                place,
                body,
            } if body.is_empty() => (
                api.voice_head(&id, bytes, &source, surface, place.as_deref(), None)?,
                true,
            ),
            RequestCommand::Upload { name, mime, body } if body.is_empty() => {
                (api.upload_head(&name, &mime, bytes.len(), None)?, true)
            }
            request if bytes.is_empty() => (request.build(&api)?, false),
            _ => return Err(Error::invalid("Conflicting binary request body")),
        };
        let mut value = serde_json::to_value(request)?;
        value["input_body"] = json!(input_body);
        Ok(PocketResult::json(0, 0, value))
    })
}

/// # Safety
/// Input JSON is borrowed until return. Close the returned component handle exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_component_create(data: *const u8, len: usize) -> PocketResult {
    guard(|| {
        let value: Value = serde_json::from_slice(unsafe { input(data, len)? })?;
        let mut component = component::Component::new(&value)?;
        let state = component.dispatch(&json!({"type":"state"}))?;
        let mut registry = SESSIONS
            .get_or_init(|| Mutex::new(Registry::default()))
            .lock()
            .map_err(|_| Error::new("internal_error", "Owner lock poisoned"))?;
        if registry.sessions.len() + registry.components.len() >= 256 {
            return Err(Error::new("limit_exceeded", "Too many owners"));
        }
        registry.next = registry
            .next
            .checked_add(1)
            .ok_or_else(|| Error::new("limit_exceeded", "Handle space exhausted"))?;
        let handle = registry.next;
        registry.components.insert(handle, component);
        Ok(PocketResult::json(0, handle, state))
    })
}

fn with_component(
    handle: u64,
    f: impl FnOnce(&mut component::Component) -> pocket_core::Result<Value>,
) -> PocketResult {
    guard(|| {
        let mut registry = SESSIONS
            .get_or_init(|| Mutex::new(Registry::default()))
            .lock()
            .map_err(|_| Error::new("internal_error", "Owner lock poisoned"))?;
        let owner = registry
            .components
            .get_mut(&handle)
            .ok_or_else(|| Error::new("invalid_handle", "Unknown or closed component"))?;
        Ok(PocketResult::json(0, handle, f(owner)?))
    })
}
/// # Safety
/// Input JSON is borrowed until return; calls on the handle are serialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_component_dispatch(
    handle: u64,
    data: *const u8,
    len: usize,
) -> PocketResult {
    with_component(handle, |owner| {
        owner.dispatch(&serde_json::from_slice(unsafe { input(data, len)? })?)
    })
}
/// # Safety
/// Binary input is borrowed until return. kind: 0 packet stream, 1 uplink, 2 downlink.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pocket_component_binary(
    handle: u64,
    kind: u64,
    data: *const u8,
    len: usize,
) -> PocketResult {
    with_component(handle, |owner| {
        owner.binary(kind, unsafe { input(data, len)? })
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn pocket_component_close(handle: u64) -> PocketResult {
    guard(|| {
        let mut registry = SESSIONS
            .get_or_init(|| Mutex::new(Registry::default()))
            .lock()
            .map_err(|_| Error::new("internal_error", "Owner lock poisoned"))?;
        registry
            .components
            .remove(&handle)
            .ok_or_else(|| Error::new("invalid_handle", "Unknown or closed component"))?;
        Ok(PocketResult::json(0, 0, json!({})))
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
    fn component_playback_completion_and_native_body_bridge() {
        unsafe {
            let input = br#"{"type":"ambient_output"}"#;
            let (_, handle, _) = take(pocket_component_create(input.as_ptr(), input.len()));
            let dispatch = |command: Value| {
                let bytes = serde_json::to_vec(&command).unwrap();
                let result = take(pocket_component_dispatch(
                    handle,
                    bytes.as_ptr(),
                    bytes.len(),
                ));
                assert_eq!(result.0, 0);
                result.2
            };
            dispatch(json!({"type":"speech", "id":"c-reply", "now_ms":0}));
            assert_eq!(
                dispatch(json!({"type":"received", "samples":16000, "now_ms":1}))["state"]["receiving"],
                true
            );
            assert_eq!(
                dispatch(
                    json!({"type":"completed", "frame":{"state":"done","speech_id":"c-reply","played_ms":2000}, "now_ms":2})
                )["state"]["receiving"],
                true
            );
            assert_eq!(
                dispatch(json!({"type":"received", "samples":16000, "now_ms":3}))["state"]["receiving"],
                false
            );
            assert_eq!(take(pocket_component_close(handle)).0, 0);
            let request = br#"{"config":{"endpoint":{"host":"example.test","port":443,"tls":true},"room":"fixture","token":"prepared"},"request":{"type":"voice","id":"same-key","source":"phone","place":"fixture-place"}}"#;
            let body = [3, 0, 0xb8, 0xff, 0xfe];
            let (status, _, result) = take(pocket_request_build(
                request.as_ptr(),
                request.len(),
                body.as_ptr(),
                body.len(),
            ));
            assert_eq!(status, 0);
            assert_eq!(result["input_body"], true);
            assert_eq!(result["body"], json!([]));
            assert_eq!(result["headers"]["X-Ambient-Place"], "fixture-place");
        }
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
