// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
//! Native loopback acceptance. No microphone, speaker, external host or real room is used.
use crate::transport::{Network, Socket};
use pocket_core::{
    Error, Result,
    api::{self, Api, Attachment, Surface},
    audio::{AudioAction, PacketStream, frame_packets},
    pairing::{Endpoint, PairedDevice},
    session::{Command, Effect, Session, SessionConfig},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OPUS: &[u8] = &[0xb8, 0xff, 0xfe];
#[derive(Clone, Debug)]
struct Observed {
    path: String,
    headers: String,
    body: Vec<u8>,
}
struct Fixture {
    endpoint: Endpoint,
    stop: Arc<AtomicBool>,
    observations: Arc<Mutex<Vec<Observed>>>,
    errors: Arc<Mutex<Vec<String>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn start() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(fixture_error)?;
        let endpoint = Endpoint::new(
            "127.0.0.1",
            listener.local_addr().map_err(fixture_error)?.port(),
            false,
        )?;
        listener.set_nonblocking(true).map_err(fixture_error)?;
        let stop = Arc::new(AtomicBool::new(false));
        let observations = Arc::new(Mutex::new(Vec::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));
        let s = stop.clone();
        let log = observations.clone();
        let err = errors.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut children = Vec::new();
            while !s.load(Ordering::Acquire) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let log = log.clone();
                        let err = err.clone();
                        children.push(thread::spawn(move || {
                            if let Err(e) = serve(stream, &log) {
                                eprintln!("Local fixture: {e}");
                                if let Ok(mut errors) = err.lock() {
                                    errors.push(e.to_string());
                                }
                            }
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
            for child in children {
                if child.join().is_err() {
                    err.lock().unwrap().push("Fixture worker panicked".into());
                }
            }
        });
        Ok(Self {
            endpoint,
            stop,
            observations,
            errors,
            worker: Some(worker),
        })
    }
    fn finish(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::protocol("Fixture accept worker panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn fixture_error(_: std::io::Error) -> Error {
    Error::new("fixture_error", "Local fixture I/O failed")
}
fn check(value: bool, detail: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::new("acceptance_failed", detail))
    }
}

fn serve(mut stream: TcpStream, log: &Arc<Mutex<Vec<Observed>>>) -> Result<()> {
    stream.set_nonblocking(false).map_err(fixture_error)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(fixture_error)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(fixture_error)?;
    let mut peek = [0; 8192];
    let n = loop {
        let n = stream.peek(&mut peek).map_err(fixture_error)?;
        if n == 0 {
            return Ok(());
        }
        if peek[..n].windows(4).any(|w| w == b"\r\n\r\n") {
            break n;
        }
        if n == peek.len() {
            return Err(Error::protocol("Fixture headers too large"));
        }
        thread::sleep(Duration::from_millis(1));
    };
    let head = String::from_utf8_lossy(&peek[..n]);
    if head.starts_with("GET /live?") {
        check(
            head.to_ascii_lowercase()
                .contains("authorization: bearer fixture-token"),
            "WebSocket sends device authentication",
        )?;
        let mut socket = tungstenite::accept(stream)
            .map_err(|_| Error::protocol("Fixture WebSocket handshake failed"))?;
        socket
            .send(tungstenite::Message::Text(
                json!({"type":"meta", "conn":"fixture-conn", "role":"master", "media":1})
                    .to_string()
                    .into(),
            ))
            .map_err(|_| Error::protocol("Fixture meta write failed"))?;
        socket
            .send(tungstenite::Message::Text(
                json!({"type":"ready", "api":2, "seq":3, "messages":[]})
                    .to_string()
                    .into(),
            ))
            .map_err(|_| Error::protocol("Fixture ready write failed"))?;
        let mut hello = false;
        let mut microphone = false;
        loop {
            match socket
                .read()
                .map_err(|_| Error::protocol("Fixture waits for capture"))?
            {
                tungstenite::Message::Text(text) => {
                    let v: Value = serde_json::from_str(&text)?;
                    if v["type"] == "hello" {
                        hello = true;
                    }
                    if v["type"] == "mute" {
                        microphone = v["on"] == true;
                    }
                }
                tungstenite::Message::Binary(bytes) => {
                    check(
                        hello && microphone && bytes.as_ref() == [vec![1], OPUS.to_vec()].concat(),
                        "Ambient hello/mute precede framed audio",
                    )?;
                    break;
                }
                _ => {}
            }
        }
        for frame in [
            json!({"type":"speech", "speech_id":"c-fixture"}),
            json!({"type":"playback", "speech_id":"c-fixture", "state":"done", "played_ms":20}),
        ] {
            socket
                .send(tungstenite::Message::Text(frame.to_string().into()))
                .map_err(|_| Error::protocol("Fixture speech write failed"))?;
        }
        socket
            .send(tungstenite::Message::Binary(OPUS.to_vec().into()))
            .map_err(|_| Error::protocol("Fixture audio write failed"))?;
        loop {
            if let tungstenite::Message::Text(text) = socket
                .read()
                .map_err(|_| Error::protocol("Fixture waits for playback"))?
            {
                let v: Value = serde_json::from_str(&text)?;
                if v["type"] == "played" {
                    check(v["ms"] == 20, "Actual playback callback controls watermark")?;
                    break;
                }
            }
        }
        socket
            .send(tungstenite::Message::Text(
                json!({"type":"speech", "speech_id":"c-interrupt"})
                    .to_string()
                    .into(),
            ))
            .map_err(|_| Error::protocol("Fixture interrupt declaration failed"))?;
        let mut muted = false;
        let mut interrupted = false;
        loop {
            if let tungstenite::Message::Text(text) = socket
                .read()
                .map_err(|_| Error::protocol("Fixture waits for interrupt"))?
            {
                let v: Value = serde_json::from_str(&text)?;
                if v["type"] == "mute" && v["on"] == false {
                    muted = true;
                }
                if v["type"] == "interrupted" {
                    interrupted = v["speech_id"] == "c-interrupt";
                }
                if muted && interrupted && v["type"] == "mute" && v["on"] == true {
                    break;
                }
            }
        }
        socket
            .send(tungstenite::Message::Text(
                json!({"type":"fixture_done"}).to_string().into(),
            ))
            .map_err(|_| Error::protocol("Fixture completion write failed"))?;
        let mut final_unmute = false;
        loop {
            match socket.read() {
                Ok(tungstenite::Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => {
                    break;
                }
                Ok(tungstenite::Message::Text(text)) => {
                    let frame: Value = serde_json::from_str(&text)?;
                    if frame["type"] == "mute" && frame["on"] == true {
                        final_unmute = true;
                    }
                }
                Ok(_) => {}
                Err(_) => return Err(Error::protocol("Fixture close handshake failed")),
            }
        }
        check(
            final_unmute,
            "Final microphone restore is written before socket close",
        )?;
        return Ok(());
    }
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(fixture_error)?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
        if bytes.len() > 8192 {
            return Err(Error::protocol("Fixture headers too large"));
        }
    }
    let headers =
        String::from_utf8(bytes).map_err(|_| Error::protocol("Fixture headers not UTF-8"))?;
    let path = headers
        .lines()
        .next()
        .and_then(|s| s.split_whitespace().nth(1))
        .ok_or_else(|| Error::protocol("Missing fixture path"))?
        .to_owned();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|n| n.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    check(length <= 1024 * 1024, "Fixture input body is bounded")?;
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(fixture_error)?;
    let anonymous = path == "/healthz" || path == "/api/pair";
    check(
        headers
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-token")
            != anonymous,
        "Anonymous routes omit token; data routes authenticate",
    )?;
    let mut observations = log
        .lock()
        .map_err(|_| Error::protocol("Fixture log poisoned"))?;
    observations.push(Observed {
        path: path.clone(),
        headers: headers.clone(),
        body: body.clone(),
    });
    let (status, mime, output) = if path == "/healthz" {
        (200, "application/json", br#"{"ok":true}"#.to_vec())
    } else if path == "/api/pair" {
        check(
            serde_json::from_slice::<Value>(&body)? == json!({"code":"fixture-code"}),
            "Pairing request contains only code",
        )?;
        (200, "application/json", br#"{"device_id":"fixture-device","room":"fixture-room","room_name":"Prepared room","scope":"notify","token":"fixture-token"}"#.to_vec())
    } else if path.starts_with("/api/state?") {
        (
            200,
            "application/json",
            br#"{"room":"fixture-room","date":"2026-01-01","limits":{"upload_max_bytes":1048576}}"#
                .to_vec(),
        )
    } else if path.starts_with("/api/messages?") {
        if headers
            .to_ascii_lowercase()
            .contains("application/vnd.ambient.opus-packets")
        {
            check(
                pocket_core::audio::validate_voice(&body)? == 2,
                "Voice body uses original packet framing",
            )?;
        }
        let attempts = observations
            .iter()
            .filter(|o| o.path.starts_with("/api/messages?") && o.headers.contains("prepared-text"))
            .count();
        if headers.contains("prepared-text") && attempts == 1 {
            (
                503,
                "application/json",
                br#"{"error":"perception_unavailable"}"#.to_vec(),
            )
        } else {
            (
                202,
                "application/json",
                br#"{"message_id":"prepared"}"#.to_vec(),
            )
        }
    } else if path.starts_with("/api/speech?") {
        (
            200,
            pocket_core::audio::VOICE_CONTENT_TYPE,
            frame_packets(&[OPUS.to_vec(), OPUS.to_vec()])?,
        )
    } else if path.starts_with("/api/upload?") {
        (
            200,
            "application/json",
            br#"{"path":"/uploads/prepared.txt"}"#.to_vec(),
        )
    } else if path.starts_with("/api/attachment?") {
        (200, "text/plain", b"abc".to_vec())
    } else if path == "/redirect" {
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/unreachable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").map_err(fixture_error)?;
        return Ok(());
    } else {
        return Err(Error::protocol("Unexpected fixture request"));
    };
    drop(observations);
    write!(stream, "HTTP/1.1 {status} Prepared\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", output.len()).map_err(fixture_error)?;
    stream.write_all(&output).map_err(fixture_error)?;
    Ok(())
}

fn send_socket(socket: &mut Socket, effects: Vec<Effect>) -> Result<()> {
    for effect in effects {
        match effect {
            Effect::SendFrame { frame, .. } => socket.text(&frame)?,
            Effect::SendBinary { bytes, .. } => socket.binary(&bytes)?,
            Effect::CloseLive { final_frames, .. } => {
                for frame in final_frames {
                    socket.text(&frame)?;
                }
                socket.close()?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn run() -> Result<Value> {
    let mut fixture = Fixture::start()?;
    let network = Network::new()?;
    let paired = network.request(&fixture.endpoint, &api::pair_request("fixture-code")?, 5000)?;
    let device = PairedDevice::decode(&paired.body)?;
    let api = Api::new(fixture.endpoint.clone(), &device.room, &device.token)?;
    check(
        network
            .request(&fixture.endpoint, &api.health()?, 5000)?
            .status
            == 200,
        "Anonymous health succeeds",
    )?;
    let state = api::RoomState::decode(
        &network
            .request(&fixture.endpoint, &api.state()?, 5000)?
            .body,
    )?;
    check(
        state.upload_max_bytes == Some(1048576),
        "Advertised upload bounds decode",
    )?;
    let config = SessionConfig {
        endpoint: fixture.endpoint.clone(),
        room: device.room,
        token: device.token,
        scope: device.scope,
        surface: Surface::default(),
        cursor: None,
        media: true,
    };
    let mut session = Session::new(config)?;
    let effects = session.dispatch(Command::SendText {
        id: "prepared-text".into(),
        text: "Prepared 文本".into(),
        attachments: vec![],
        place: None,
        now_ms: 0,
    })?;
    let (generation, record) = match effects.first() {
        Some(Effect::Persist {
            generation,
            message,
        }) => (*generation, message),
        _ => return Err(Error::protocol("Expected durable admission")),
    };
    check(
        !record.headers.contains_key("Authorization"),
        "Admission record omits credential",
    )?;
    let effects = session.dispatch(Command::Persisted {
        id: "prepared-text".into(),
        generation,
        ok: true,
        now_ms: 0,
    })?;
    let mut first = None;
    for effect in effects {
        if let Effect::Http {
            request,
            timeout_ms,
            ..
        } = effect
        {
            first = Some(network.request(&fixture.endpoint, &request, timeout_ms)?);
        }
    }
    let first = first.ok_or_else(|| Error::protocol("Missing admitted HTTP send"))?;
    check(first.status == 503, "Prepared transient failure occurs")?;
    let retry = session.dispatch(Command::HttpResult {
        id: "prepared-text".into(),
        generation,
        status: first.status,
        body: first.body,
        now_ms: 0,
    })?;
    let command = retry
        .into_iter()
        .find_map(|e| {
            if let Effect::Schedule { command, .. } = e {
                Some(command)
            } else {
                None
            }
        })
        .ok_or_else(|| Error::protocol("Missing retry effect"))?;
    for effect in session.dispatch(command)? {
        if let Effect::Http {
            request,
            timeout_ms,
            ..
        } = effect
        {
            let response = network.request(&fixture.endpoint, &request, timeout_ms)?;
            check(response.status == 202, "Retried text accepted")?;
            session.dispatch(Command::HttpResult {
                id: "prepared-text".into(),
                generation,
                status: response.status,
                body: response.body,
                now_ms: 500,
            })?;
        }
    }
    let voice = frame_packets(&[OPUS.to_vec(), OPUS.to_vec()])?;
    check(
        network
            .request(
                &fixture.endpoint,
                &api.voice(
                    "prepared-voice",
                    &voice,
                    "phone",
                    Surface::default(),
                    None,
                    Some(&state),
                )?,
                5000,
            )?
            .status
            == 202,
        "Voice upload accepted",
    )?;
    let speech = network.request(&fixture.endpoint, &api.speech("Prepared reply")?, 5000)?;
    let mut parser = PacketStream::default();
    let mut received = vec![];
    for byte in speech.body {
        received.extend(parser.push(&[byte])?);
    }
    parser.finish()?;
    check(
        received == vec![OPUS.to_vec(), OPUS.to_vec()],
        "Incremental speech reconstructs packet boundaries",
    )?;
    let local = Attachment {
        name: "prepared.txt".into(),
        mime: "text/plain".into(),
        sha256: None,
        path: None,
    };
    let upload = network.request(
        &fixture.endpoint,
        &api.upload(&local.name, &local.mime, b"abc", Some(&state))?,
        5000,
    )?;
    let uploaded = api::uploaded(local, b"abc", &upload.body)?;
    check(
        network
            .request(&fixture.endpoint, &api.attachment(&uploaded)?, 5000)?
            .body
            == b"abc",
        "Attachment upload/digest/download completes",
    )?;
    let redirect = api.request("GET", "/redirect".into(), Default::default(), vec![])?;
    check(
        network.request(&fixture.endpoint, &redirect, 5000)?.status == 302,
        "Credential-bearing redirects are not followed",
    )?;

    let open = session.dispatch(Command::Connect)?;
    let (generation, request) = open
        .into_iter()
        .find_map(|e| {
            if let Effect::OpenLive {
                generation,
                request,
            } = e
            {
                Some((generation, request))
            } else {
                None
            }
        })
        .ok_or_else(|| Error::protocol("Missing live dial"))?;
    let mut socket = Socket::connect(&fixture.endpoint, &request)?;
    for _ in 0..2 {
        let tungstenite::Message::Text(text) = socket.read()? else {
            return Err(Error::protocol("Expected meta/ready"));
        };
        let effects = session.dispatch(Command::LiveText {
            generation,
            frame: serde_json::from_str(&text)?,
            now_ms: 600,
        })?;
        send_socket(&mut socket, effects)?;
    }
    check(session.snapshot().online, "WebSocket replay reaches Online")?;
    send_socket(
        &mut socket,
        session.dispatch(Command::SetAmbient { enabled: true })?,
    )?;
    send_socket(
        &mut socket,
        session.dispatch(Command::CapturePacket {
            packet: OPUS.to_vec(),
            aec: false,
        })?,
    )?;
    let mut done = false;
    let mut decoded = false;
    for _ in 0..12 {
        let effects = match socket.read()? {
            tungstenite::Message::Text(text) => {
                let frame: Value = serde_json::from_str(&text)?;
                if frame["type"] == "fixture_done" {
                    done = true;
                    break;
                }
                let interrupted = frame["type"] == "speech" && frame["speech_id"] == "c-interrupt";
                let mut effects = session.dispatch(Command::LiveText {
                    generation,
                    frame,
                    now_ms: 700,
                })?;
                if interrupted {
                    effects.extend(session.dispatch(Command::Press {
                        source: "prepared-input".into(),
                        active: true,
                    })?);
                    effects.extend(session.dispatch(Command::Press {
                        source: "prepared-input".into(),
                        active: false,
                    })?);
                }
                effects
            }
            tungstenite::Message::Binary(bytes) => session.dispatch(Command::LiveBinary {
                generation,
                bytes: bytes.to_vec(),
            })?,
            _ => vec![],
        };
        for effect in &effects {
            if let Effect::Audio {
                action:
                    AudioAction::Decode {
                        generation: audio_generation,
                        packet,
                    },
            } = effect
            {
                check(packet == OPUS, "Framed downlink preserves Opus packet")?;
                let g = *audio_generation;
                session.dispatch(Command::PlaybackScheduled { generation: g })?;
                session.dispatch(Command::Decoded {
                    generation: g,
                    samples: 320,
                    now_ms: 700,
                })?;
                send_socket(
                    &mut socket,
                    session.dispatch(Command::Played {
                        generation: g,
                        ms: 20,
                        skipped: false,
                    })?,
                )?;
                decoded = true;
            }
        }
        send_socket(&mut socket, effects)?;
    }
    check(
        done && decoded,
        "Ambient playback and local interruption reach server confirmation",
    )?;
    send_socket(&mut socket, session.dispatch(Command::Disconnect)?)?;
    fixture.finish()?;
    let observations = fixture
        .observations
        .lock()
        .map_err(|_| Error::protocol("Fixture log poisoned"))?;
    let attempts: Vec<_> = observations
        .iter()
        .filter(|o| o.headers.contains("prepared-text"))
        .collect();
    check(
        attempts.len() == 2
            && attempts[0].body == attempts[1].body
            && attempts[0].headers == attempts[1].headers,
        "Retries preserve exact body, identity and surface",
    )?;
    check(
        fixture
            .errors
            .lock()
            .map_err(|_| Error::protocol("Fixture log poisoned"))?
            .is_empty(),
        "Local server observed no protocol violations",
    )?;
    Ok(
        json!({"status":"pass", "version":pocket_core::VERSION, "arch":std::env::consts::ARCH, "os":std::env::consts::OS, "checks":["pairing_and_auth", "room_limits", "durable_admission", "idempotent_text_retry", "voice_upload", "streaming_speech_framing", "attachment_roundtrip", "redirect_boundary", "live_replay", "ambient_hello_and_media", "playback_watermark", "local_interruption", "final_unmute"], "transport":"native_loopback", "audio":"prepared_packets_and_simulated_hardware_completion", "external_room_sends":0}),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_chat_voice_and_ambient() {
        super::run().expect("native loopback protocol acceptance");
    }
}
