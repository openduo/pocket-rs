// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const SAMPLE_RATE: u64 = 16_000;
pub const FRAME_MS: u64 = 20;
pub const MAX_OPUS_PACKET: usize = 1275;
pub const MAX_VOICE_BYTES: usize = 16 * 1024 * 1024;
pub const VOICE_CONTENT_TYPE: &str = "application/vnd.ambient.opus-packets";
pub const RECEIVE_IDLE_MS: u64 = 30_000;

pub fn frame_packets(packets: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for packet in packets {
        validate_packet(packet)?;
        if out.len() + packet.len() + 2 > MAX_VOICE_BYTES {
            return Err(Error::new("limit_exceeded", "Voice body too large"));
        }
        out.extend_from_slice(&(packet.len() as u16).to_le_bytes());
        out.extend_from_slice(packet);
    }
    Ok(out)
}
pub fn validate_packet(packet: &[u8]) -> Result<()> {
    if packet.is_empty() || packet.len() > MAX_OPUS_PACKET {
        return Err(Error::protocol("Opus packet length must be 1..1275"));
    }
    Ok(())
}
pub fn validate_voice(body: &[u8]) -> Result<usize> {
    if body.is_empty() || body.len() > MAX_VOICE_BYTES {
        return Err(Error::invalid("Voice body is empty or exceeds limit"));
    }
    let mut parser = PacketStream::default();
    let count = parser.push(body)?.len();
    parser.finish()?;
    Ok(count)
}

/// Incremental HTTP packet framing, bounded even when supplied a large input chunk.
#[derive(Default)]
pub struct PacketStream {
    prefix: Vec<u8>,
    packet: Vec<u8>,
    expected: Option<usize>,
    failed: bool,
}
impl PacketStream {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        if self.failed {
            return Err(Error::protocol("Packet stream is retired"));
        }
        let result = self.push_inner(bytes);
        if result.is_err() {
            self.failed = true;
            self.prefix.clear();
            self.packet.clear();
            self.expected = None;
        }
        result
    }
    fn push_inner(&mut self, mut bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            if self.expected.is_none() {
                let take = (2 - self.prefix.len()).min(bytes.len());
                self.prefix.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if self.prefix.len() < 2 {
                    break;
                }
                let n = u16::from_le_bytes([self.prefix[0], self.prefix[1]]) as usize;
                self.prefix.clear();
                if n == 0 || n > MAX_OPUS_PACKET {
                    return Err(Error::protocol("Invalid packet prefix"));
                }
                self.expected = Some(n);
            }
            if let Some(n) = self.expected {
                let take = (n - self.packet.len()).min(bytes.len());
                self.packet.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if self.packet.len() == n {
                    out.push(std::mem::take(&mut self.packet));
                    self.expected = None;
                }
            }
        }
        Ok(out)
    }
    pub fn finish(&self) -> Result<()> {
        if self.failed || !self.prefix.is_empty() || self.expected.is_some() {
            return Err(Error::protocol("Truncated or failed packet stream"));
        }
        Ok(())
    }
    pub fn buffered_bytes(&self) -> usize {
        self.prefix.len() + self.packet.len()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioAction {
    Send { frame: Value },
    Decode { generation: u64, packet: Vec<u8> },
    ResetDecoder,
    ClearPlayback,
    StopRead,
    Notice { code: String },
}
fn send(frame: Value) -> AudioAction {
    AudioAction::Send { frame }
}

/// One audio protocol owner. The host supplies decoded durations and actual hardware completion.
#[derive(Clone, Debug, Serialize)]
pub struct Ambient {
    pub room: String,
    pub conn: Option<String>,
    pub role: Option<String>,
    pub room_state: Option<String>,
    pub meta_seen: bool,
    pub hello_sent: bool,
    pub capturing: bool,
    pub framed: bool,
    pub generation: u64,
    pub current: Option<String>,
    pub reading: bool,
    pub pending_blocks: u64,
    pub receiving: bool,
    pub played_ms: u64,
    #[serde(skip)]
    presses: BTreeSet<String>,
    #[serde(skip)]
    aec: bool,
    #[serde(skip)]
    dropped: bool,
    #[serde(skip)]
    mic_may_be_off: bool,
    #[serde(skip)]
    last_reported: Option<u64>,
    #[serde(skip)]
    received_samples: u64,
    #[serde(skip)]
    completed_ms: Option<u64>,
    #[serde(skip)]
    last_progress_ms: u64,
    #[serde(skip)]
    hardware_ms: u64,
    #[serde(skip)]
    skipped_samples: u64,
}
impl Ambient {
    pub fn new(room: &str) -> Self {
        Self {
            room: room.into(),
            conn: None,
            role: None,
            room_state: None,
            meta_seen: false,
            hello_sent: false,
            capturing: false,
            framed: false,
            generation: 0,
            current: None,
            reading: false,
            pending_blocks: 0,
            receiving: false,
            played_ms: 0,
            presses: BTreeSet::new(),
            aec: false,
            dropped: false,
            mic_may_be_off: false,
            last_reported: None,
            received_samples: 0,
            completed_ms: None,
            last_progress_ms: 0,
            hardware_ms: 0,
            skipped_samples: 0,
        }
    }
    pub fn muted(&self) -> bool {
        !self.presses.is_empty()
    }
    pub fn sends_audio(&self) -> bool {
        self.capturing && self.hello_sent && self.role.as_deref() != Some("peer")
    }
    pub fn playback_pending(&self) -> bool {
        self.receiving || self.pending_blocks > 0 || self.reading
    }
    pub fn socket_opened(&mut self) {
        self.conn = None;
        self.role = None;
        self.room_state = None;
        self.meta_seen = false;
        self.hello_sent = false;
        self.framed = false;
        self.mic_may_be_off = false;
    }
    pub fn socket_closed(&mut self) -> Vec<AudioAction> {
        self.socket_opened();
        if self.reading { vec![] } else { self.clear() }
    }
    fn mic_frame(&mut self) -> AudioAction {
        if self.muted() {
            self.mic_may_be_off = true;
        }
        send(json!({"type":"mute", "on":!self.muted()}))
    }
    fn hello(&mut self) -> Vec<AudioAction> {
        if !self.capturing || self.hello_sent || self.conn.is_none() {
            return vec![];
        }
        self.hello_sent = true;
        vec![
            send(
                json!({"type":"hello", "room":self.room, "conn":self.conn, "edge":"client", "aec":self.aec}),
            ),
            self.mic_frame(),
        ]
    }
    pub fn packet_encoded(&mut self, aec: bool) -> Vec<AudioAction> {
        self.capturing = true;
        self.aec = aec;
        self.hello()
    }
    pub fn takeover(&mut self) -> Vec<AudioAction> {
        self.hello_sent = false;
        self.hello()
    }
    /// Queue this before retiring the transport; the room's mic switch outlives its socket.
    pub fn release_capture(&mut self) -> Vec<AudioAction> {
        let out = self.microphone_restore().into_iter().map(send).collect();
        self.capturing = false;
        self.hello_sent = false;
        self.mic_may_be_off = false;
        out
    }
    pub fn microphone_restore(&self) -> Option<Value> {
        (self.hello_sent && self.mic_may_be_off).then(|| json!({"type":"mute", "on":true}))
    }
    pub fn press(&mut self, source: &str, active: bool) -> Vec<AudioAction> {
        let before = self.muted();
        let changed = if active {
            self.presses.insert(source.into())
        } else {
            self.presses.remove(source)
        };
        let mut out = if before != self.muted() && self.hello_sent {
            vec![self.mic_frame()]
        } else {
            vec![]
        };
        if changed && active {
            out.extend(self.stop_read());
            if let Some(id) = self.current.clone() {
                let report = self.hello_sent;
                out.extend(self.clear());
                self.dropped = true;
                if report {
                    out.push(send(json!({"type":"interrupted", "speech_id":id})));
                }
            }
        }
        out
    }
    fn clear(&mut self) -> Vec<AudioAction> {
        self.generation = self.generation.wrapping_add(1);
        self.current = None;
        self.pending_blocks = 0;
        self.receiving = false;
        self.played_ms = 0;
        self.last_reported = None;
        self.completed_ms = None;
        self.received_samples = 0;
        self.hardware_ms = 0;
        self.skipped_samples = 0;
        vec![AudioAction::ClearPlayback, AudioAction::ResetDecoder]
    }
    fn stop_read(&mut self) -> Vec<AudioAction> {
        if !self.reading {
            return vec![];
        }
        self.reading = false;
        vec![AudioAction::StopRead, AudioAction::ClearPlayback]
    }
    pub fn read_began(&mut self) -> Vec<AudioAction> {
        let mut out = self.stop_read();
        if let Some(id) = self.current.clone() {
            let report = self.hello_sent;
            out.extend(self.clear());
            self.dropped = true;
            if report {
                out.push(send(json!({"type":"interrupted", "speech_id":id})));
            }
        }
        self.reading = true;
        out
    }
    pub fn read_ended(&mut self) {
        self.reading = false;
    }
    pub fn hush(&mut self) -> Vec<AudioAction> {
        let mut out = self.stop_read();
        out.extend(self.clear());
        self.dropped = false;
        out.push(send(json!({"type":"hush"})));
        out
    }
    pub fn frame(&mut self, frame: &Value, now_ms: u64) -> Result<Vec<AudioAction>> {
        match frame["type"].as_str().unwrap_or_default() {
            "meta" => {
                self.meta_seen = true;
                if let Some(media) = frame.get("media") {
                    if media.as_u64() != Some(1) {
                        return Err(Error::new(
                            "unsupported_media",
                            "Only media framing version 1 is supported",
                        ));
                    }
                    self.framed = true;
                }
                if let Some(conn) = frame["conn"].as_str() {
                    self.conn = Some(conn.into());
                    self.hello_sent = false;
                }
                if let Some(role) = frame["role"].as_str() {
                    self.role = Some(role.into());
                }
                if let Some(state) = frame["state"].as_str() {
                    self.room_state = Some(state.into());
                }
                if frame["state"] == "unowned" {
                    self.role = None;
                }
                Ok(self.hello())
            }
            "audio_params" => {
                if frame
                    .get("rate")
                    .is_some_and(|v| v.as_u64() != Some(SAMPLE_RATE))
                {
                    return Err(Error::new(
                        "unsupported_audio_rate",
                        "Room sample rate must be 16000",
                    ));
                }
                Ok(vec![])
            }
            "speech" => {
                let Some(id) = frame["speech_id"].as_str().filter(|s| !s.is_empty()) else {
                    return Err(Error::protocol("Missing speech id"));
                };
                if self.muted() {
                    let mut out = self.clear();
                    self.dropped = true;
                    if self.hello_sent {
                        out.push(send(json!({"type":"interrupted", "speech_id":id})));
                    }
                    return Ok(out);
                }
                let mut out = self.stop_read();
                self.generation = self.generation.wrapping_add(1);
                self.current = Some(id.into());
                self.pending_blocks = 0;
                self.played_ms = 0;
                self.last_reported = None;
                self.dropped = false;
                self.receiving = true;
                self.completed_ms = None;
                self.received_samples = 0;
                self.hardware_ms = 0;
                self.skipped_samples = 0;
                self.last_progress_ms = now_ms;
                out.push(AudioAction::ResetDecoder);
                Ok(out)
            }
            "stop_audio" => {
                if !self.accepts_stop(frame) {
                    return Ok(vec![]);
                }
                Ok(self.clear())
            }
            "playback" => {
                if frame["state"] == "done"
                    && frame["speech_id"].as_str() == self.current.as_deref()
                    && !self.reading
                {
                    self.completed_ms =
                        Some(frame.get("played_ms").and_then(Value::as_u64).unwrap_or(0));
                    self.last_progress_ms = now_ms;
                    self.complete_if_received();
                }
                Ok(vec![])
            }
            "camera" => Ok(vec![send(
                json!({"type":"camera_declined", "reason":"unsupported"}),
            )]),
            _ => Ok(vec![]),
        }
    }
    pub fn binary(&self, bytes: &[u8]) -> Result<Vec<AudioAction>> {
        let packet = if self.framed {
            match bytes.first() {
                Some(1) => &bytes[1..],
                Some(_) => {
                    return Ok(vec![AudioAction::Notice {
                        code: "unsupported_media_kind".into(),
                    }]);
                }
                None => return Err(Error::protocol("Empty media frame")),
            }
        } else {
            bytes
        };
        validate_packet(packet)?;
        if self.current.is_none() || self.reading {
            return Ok(if self.dropped || self.reading {
                vec![]
            } else {
                vec![AudioAction::Notice {
                    code: "audio_without_speech".into(),
                }]
            });
        }
        Ok(vec![AudioAction::Decode {
            generation: self.generation,
            packet: packet.into(),
        }])
    }
    pub fn uplink(&self, packet: &[u8]) -> Result<Option<Vec<u8>>> {
        validate_packet(packet)?;
        if !self.sends_audio() {
            return Ok(None);
        }
        let mut out = Vec::with_capacity(packet.len() + usize::from(self.framed));
        if self.framed {
            out.push(1);
        }
        out.extend_from_slice(packet);
        Ok(Some(out))
    }
    pub fn scheduled(&mut self, generation: u64) {
        if generation == self.generation && self.current.is_some() {
            self.pending_blocks = self.pending_blocks.saturating_add(1);
        }
    }
    pub fn decoded(&mut self, generation: u64, samples: u64, now_ms: u64) {
        if generation != self.generation || self.current.is_none() {
            return;
        }
        self.received_samples = self.received_samples.saturating_add(samples);
        self.last_progress_ms = now_ms;
        self.complete_if_received();
    }
    fn complete_if_received(&mut self) {
        if self
            .completed_ms
            .is_some_and(|ms| self.received_samples.saturating_mul(1000) / SAMPLE_RATE >= ms)
        {
            self.receiving = false;
        }
    }
    pub fn played(&mut self, generation: u64, ms: u64, skipped: bool) -> Vec<AudioAction> {
        if generation != self.generation
            || self.current.is_none()
            || (!skipped && self.pending_blocks == 0)
        {
            return vec![];
        }
        if !skipped {
            self.pending_blocks -= 1;
        }
        self.played_ms = self.played_ms.saturating_add(ms);
        let report = skipped
            || self.last_reported.is_none()
            || self
                .last_reported
                .is_some_and(|p| self.played_ms.saturating_sub(p) >= 250)
            || self.pending_blocks == 0;
        if report && self.hello_sent {
            self.last_reported = Some(self.played_ms);
            vec![send(
                json!({"type":"played", "speech_id":self.current, "ms":self.played_ms}),
            )]
        } else {
            vec![]
        }
    }
    pub fn tick(&mut self, now_ms: u64, hardware_pending: bool) {
        if self.receiving
            && self.pending_blocks == 0
            && !hardware_pending
            && now_ms.saturating_sub(self.last_progress_ms) >= RECEIVE_IDLE_MS
        {
            self.receiving = false;
        }
    }

    pub fn accepts_stop(&self, frame: &Value) -> bool {
        !self.reading
            && !self.current.as_deref().is_some_and(|s| s.starts_with('s'))
            && !frame
                .get("speech_id")
                .is_some_and(|v| v.as_str() != self.current.as_deref())
    }

    /// Adapter for platforms reporting an absolute hardware position rather than per-block drains.
    pub fn output_position(
        &mut self,
        key: u64,
        ms: u64,
        skipped_samples: u64,
        reports: bool,
    ) -> Option<Value> {
        if key != self.generation || self.current.is_none() {
            return None;
        }
        self.hardware_ms = self.hardware_ms.max(ms);
        if !self.reading {
            self.skipped_samples = self.skipped_samples.saturating_add(skipped_samples);
        }
        self.played_ms = self
            .hardware_ms
            .saturating_add(self.skipped_samples.saturating_mul(1000) / SAMPLE_RATE);
        if !reports || self.reading || self.played_ms <= self.last_reported.unwrap_or(0) {
            return None;
        }
        self.last_reported = Some(self.played_ms);
        Some(json!({"type":"played", "speech_id":self.current, "ms":self.played_ms}))
    }

    pub fn stop_output(&mut self) {
        self.reading = false;
        self.clear();
    }

    pub fn output_read(&mut self, id: &str, now_ms: u64) -> Result<()> {
        self.frame(
            &json!({"type":"speech", "speech_id":format!("read:{id}")}),
            now_ms,
        )?;
        self.reading = true;
        self.receiving = false;
        Ok(())
    }
}
