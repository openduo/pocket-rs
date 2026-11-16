// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use crate::{
    CLIENT_API_MAJOR, Error, Result,
    api::{Api, Attachment, Request, Surface, query, validate_message_id},
    audio::{Ambient, AudioAction, validate_packet, validate_voice},
    live::{LiveEvent, RowCursor},
    pairing::{DeviceScope, Endpoint, unauthorized},
    retry::{RedialBackoff, VoiceRetry},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MAX_PENDING: usize = 64;
const MAX_FRAME_JSON: usize = 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub endpoint: Endpoint,
    pub room: String,
    pub token: String,
    #[serde(default = "room_scope")]
    pub scope: DeviceScope,
    #[serde(default)]
    pub surface: Surface,
    #[serde(default)]
    pub cursor: Option<u64>,
    #[serde(default)]
    pub media: bool,
}
fn room_scope() -> DeviceScope {
    DeviceScope::Room
}

/// The host executes effects, then returns their completions with the supplied generation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Connect,
    Reconnect {
        generation: u64,
    },
    Disconnect,
    LiveText {
        generation: u64,
        frame: Value,
        #[serde(default)]
        now_ms: u64,
    },
    LiveBinary {
        generation: u64,
        bytes: Vec<u8>,
    },
    SocketClosed {
        generation: u64,
        #[serde(default)]
        code: Option<u16>,
    },
    SetAmbient {
        enabled: bool,
    },
    CapturePacket {
        packet: Vec<u8>,
        #[serde(default)]
        aec: bool,
    },
    Press {
        source: String,
        active: bool,
    },
    Takeover,
    Hush,
    ReadBegan,
    ReadEnded,
    PlaybackScheduled {
        generation: u64,
    },
    Decoded {
        generation: u64,
        samples: u64,
        now_ms: u64,
    },
    Played {
        generation: u64,
        ms: u64,
        #[serde(default)]
        skipped: bool,
    },
    Tick {
        now_ms: u64,
        #[serde(default)]
        hardware_pending: bool,
    },
    SetSurface {
        surface: Surface,
    },
    SendText {
        id: String,
        text: String,
        #[serde(default)]
        attachments: Vec<Attachment>,
        #[serde(default)]
        place: Option<String>,
        now_ms: u64,
    },
    SendVoice {
        id: String,
        body: Vec<u8>,
        #[serde(default = "phone_source")]
        source: String,
        #[serde(default)]
        place: Option<String>,
        now_ms: u64,
    },
    Restore {
        message: DurableMessage,
        now_ms: u64,
    },
    Persisted {
        id: String,
        generation: u64,
        ok: bool,
        now_ms: u64,
    },
    HttpResult {
        id: String,
        generation: u64,
        status: u16,
        #[serde(default)]
        body: Vec<u8>,
        now_ms: u64,
    },
    Retry {
        id: String,
        generation: u64,
        now_ms: u64,
    },
    Discard {
        id: String,
        #[serde(default)]
        now_ms: u64,
    },
    WatchSessions {
        #[serde(default)]
        key: Option<String>,
    },
    UnwatchSessions,
    NotifySession {
        id: String,
        key: String,
        text: String,
    },
}
fn phone_source() -> String {
    "phone".into()
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    OpenLive {
        generation: u64,
        request: Request,
    },
    CloseLive {
        generation: u64,
        final_frames: Vec<Value>,
    },
    SendFrame {
        generation: u64,
        frame: Value,
    },
    SendBinary {
        generation: u64,
        bytes: Vec<u8>,
    },
    Capture {
        enabled: bool,
    },
    Audio {
        action: AudioAction,
    },
    Persist {
        generation: u64,
        message: DurableMessage,
    },
    Http {
        generation: u64,
        id: String,
        request: Request,
        timeout_ms: u64,
    },
    CancelHttp {
        generation: u64,
        id: String,
    },
    Schedule {
        delay_ms: u64,
        command: Command,
    },
    Event {
        event: Value,
    },
}

/// Stable, credential-free admission record. Store atomically before acknowledging `persisted`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DurableMessage {
    pub version: u32,
    pub room: String,
    pub id: String,
    pub voice: bool,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Prepared,
    Ready,
    Sending,
    Retrying,
    Accepted,
    Cleaned,
    Logged,
    Failed,
    Discarded,
}
impl DeliveryState {
    fn terminal(self) -> bool {
        matches!(
            self,
            Self::Cleaned | Self::Logged | Self::Failed | Self::Discarded
        )
    }
    fn busy(self) -> bool {
        matches!(self, Self::Sending | Self::Retrying)
    }
}
struct Delivery {
    generation: u64,
    state: DeliveryState,
    record: DurableMessage,
    request: Request,
    retry: VoiceRetry,
}

#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub generation: u64,
    pub online: bool,
    pub authorized: bool,
    pub cursor: Option<u64>,
    pub ambient_wanted: bool,
    pub ambient: &'a Ambient,
    pub pending: BTreeMap<&'a str, DeliveryState>,
}

pub struct Session {
    api: Api,
    scope: DeviceScope,
    surface: Surface,
    media: bool,
    generation: u64,
    next_operation: u64,
    wanted: bool,
    online: bool,
    authorized: bool,
    live_audio: bool,
    ambient_wanted: bool,
    ambient: Ambient,
    cursor: RowCursor,
    redial: RedialBackoff,
    deliveries: BTreeMap<String, Delivery>,
    watch: Option<Option<String>>,
    notifiable: BTreeMap<String, bool>,
}
impl Session {
    pub fn new(config: SessionConfig) -> Result<Self> {
        let api = Api::new(config.endpoint, &config.room, &config.token)?;
        Ok(Self {
            api,
            scope: config.scope,
            surface: config.surface,
            media: config.media,
            generation: 0,
            next_operation: 0,
            wanted: false,
            online: false,
            authorized: true,
            live_audio: false,
            ambient_wanted: false,
            ambient: Ambient::new(&config.room),
            cursor: RowCursor { seq: config.cursor },
            redial: RedialBackoff::default(),
            deliveries: BTreeMap::new(),
            watch: None,
            notifiable: BTreeMap::new(),
        })
    }
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            generation: self.generation,
            online: self.online,
            authorized: self.authorized,
            cursor: self.cursor.seq,
            ambient_wanted: self.ambient_wanted,
            ambient: &self.ambient,
            pending: self
                .deliveries
                .iter()
                .filter(|(_, d)| !d.state.terminal())
                .map(|(k, d)| (k.as_str(), d.state))
                .collect(),
        }
    }
    pub fn api(&self) -> &Api {
        &self.api
    }
    fn emit(event: Value) -> Effect {
        Effect::Event { event }
    }
    fn audio(&self, actions: Vec<AudioAction>) -> Vec<Effect> {
        actions
            .into_iter()
            .map(|action| match action {
                AudioAction::Send { frame } => Effect::SendFrame {
                    generation: self.generation,
                    frame,
                },
                action => Effect::Audio { action },
            })
            .collect()
    }
    fn open(&mut self) -> Result<Vec<Effect>> {
        if !self.authorized {
            return Err(Error::new("unauthorized", "Pair again before reconnecting"));
        }
        let old = self.generation;
        self.generation = self.generation.wrapping_add(1);
        self.online = false;
        let final_frames = self
            .ambient
            .release_capture()
            .into_iter()
            .filter_map(|a| {
                if let AudioAction::Send { frame } = a {
                    Some(frame)
                } else {
                    None
                }
            })
            .collect();
        let retired_audio = if old > 0 {
            self.ambient.socket_closed()
        } else {
            vec![]
        };
        self.ambient.socket_opened();
        self.live_audio = self.surface.spoken || self.ambient_wanted;
        let request = self
            .api
            .live(self.cursor.seq, self.live_audio, self.media)?;
        let mut out = vec![];
        if old > 0 {
            out.push(Effect::CloseLive {
                generation: old,
                final_frames,
            });
        }
        out.extend(self.audio(retired_audio));
        out.push(Effect::OpenLive {
            generation: self.generation,
            request,
        });
        Ok(out)
    }
    fn retire(&mut self) -> Vec<Effect> {
        let old = self.generation;
        let final_frames = self
            .ambient
            .release_capture()
            .into_iter()
            .filter_map(|a| {
                if let AudioAction::Send { frame } = a {
                    Some(frame)
                } else {
                    None
                }
            })
            .collect();
        let audio = self.ambient.socket_closed();
        self.generation = self.generation.wrapping_add(1);
        self.online = false;
        let mut out = vec![Effect::CloseLive {
            generation: old,
            final_frames,
        }];
        out.extend(self.audio(audio));
        out
    }
    fn revoke(&mut self) -> Vec<Effect> {
        self.authorized = false;
        self.wanted = false;
        let mut out = self.retire();
        out.push(Effect::Capture { enabled: false });
        for (id, d) in &mut self.deliveries {
            if !d.state.terminal() {
                out.push(Effect::CancelHttp {
                    generation: d.generation,
                    id: id.clone(),
                });
                d.state = DeliveryState::Failed;
                out.push(Self::emit(
                    json!({"type":"delivery", "id":id, "state":"failed", "reason":"unauthorized"}),
                ));
            }
        }
        out.push(Self::emit(json!({"type":"unauthorized"})));
        out
    }
    fn prepare(&mut self, request: Request, voice: bool, now_ms: u64) -> Result<Vec<Effect>> {
        if !self.authorized {
            return Err(Error::new("unauthorized", "Pair again before sending"));
        }
        let id = request
            .headers
            .get("Idempotency-Key")
            .ok_or_else(|| Error::invalid("Missing message identity"))?
            .clone();
        if self
            .deliveries
            .get(&id)
            .is_some_and(|d| !d.state.terminal())
        {
            return Err(Error::new("duplicate_message", "Message already pending"));
        }
        self.deliveries.retain(|_, d| !d.state.terminal());
        let bytes: usize = self.deliveries.values().map(|d| d.record.body.len()).sum();
        if self.deliveries.len() >= MAX_PENDING
            || bytes.saturating_add(request.body.len()) > crate::audio::MAX_VOICE_BYTES
        {
            return Err(Error::new("queue_full", "Pending message limit reached"));
        }
        self.next_operation = self.next_operation.wrapping_add(1);
        let mut headers = request.headers.clone();
        headers.remove("Authorization");
        let record = DurableMessage {
            version: 1,
            room: self.api.room().into(),
            id: id.clone(),
            voice,
            headers,
            body: request.body.clone(),
        };
        self.deliveries.insert(
            id,
            Delivery {
                generation: self.next_operation,
                state: DeliveryState::Prepared,
                record: record.clone(),
                request,
                retry: VoiceRetry::new(now_ms, 25_000),
            },
        );
        Ok(vec![Effect::Persist {
            generation: self.next_operation,
            message: record,
        }])
    }
    fn pump(&mut self, now_ms: u64) -> Vec<Effect> {
        if self.deliveries.values().any(|d| d.state.busy()) {
            return vec![];
        }
        let id = self
            .deliveries
            .iter()
            .filter(|(_, d)| d.state == DeliveryState::Ready)
            .min_by_key(|(_, d)| d.generation)
            .map(|(id, _)| id.clone());
        let Some(id) = id else {
            return vec![];
        };
        let d = self
            .deliveries
            .get_mut(&id)
            .expect("selected delivery exists");
        let remaining = d.retry.remaining(now_ms);
        if remaining == 0 {
            d.state = DeliveryState::Failed;
            let mut out = vec![Self::emit(
                json!({"type":"delivery", "id":id, "state":"failed", "reason":"deadline_exceeded"}),
            )];
            out.extend(self.pump(now_ms));
            return out;
        }
        d.state = DeliveryState::Sending;
        vec![Effect::Http {
            generation: d.generation,
            id,
            request: d.request.clone(),
            timeout_ms: remaining,
        }]
    }
    fn settle(&mut self, id: &str, state: DeliveryState) -> Vec<Effect> {
        let Some(d) = self.deliveries.get_mut(id).filter(|d| !d.state.terminal()) else {
            return vec![];
        };
        let mut out = vec![];
        if d.state.busy() {
            out.push(Effect::CancelHttp {
                generation: d.generation,
                id: id.into(),
            });
        }
        d.state = state;
        out.push(Self::emit(
            json!({"type":"delivery", "id":id, "state":state}),
        ));
        out
    }
    pub fn dispatch(&mut self, command: Command) -> Result<Vec<Effect>> {
        match command {
            Command::Connect => {
                self.wanted = true;
                self.redial.reset();
                self.open()
            }
            Command::Reconnect { generation } => {
                if generation != self.generation || !self.wanted {
                    return Ok(vec![]);
                }
                self.open()
            }
            Command::Disconnect => {
                self.wanted = false;
                self.ambient_wanted = false;
                let mut out = self.retire();
                out.push(Effect::Capture { enabled: false });
                Ok(out)
            }
            Command::SocketClosed { generation, code } => {
                if generation != self.generation {
                    return Ok(vec![]);
                }
                if code == Some(4401) {
                    return Ok(self.revoke());
                }
                let mut out = self.retire();
                if self.wanted {
                    let delay_ms = self.redial.failed();
                    if self.redial.sustained() {
                        out.push(Effect::Capture { enabled: false });
                    }
                    out.push(Effect::Schedule {
                        delay_ms,
                        command: Command::Reconnect {
                            generation: self.generation,
                        },
                    });
                }
                Ok(out)
            }
            Command::LiveText {
                generation,
                frame,
                now_ms,
            } => {
                if generation != self.generation {
                    return Ok(vec![]);
                }
                if serde_json::to_vec(&frame)?.len() > MAX_FRAME_JSON {
                    return Err(Error::new("limit_exceeded", "Live text frame too large"));
                }
                let mut out = vec![];
                match LiveEvent::decode(&frame)? {
                    LiveEvent::Ready {
                        seq, api, messages, ..
                    } => {
                        if api != Some(CLIENT_API_MAJOR) {
                            self.wanted = false;
                            out.extend(self.retire());
                            out.push(Self::emit(json!({"type":"upgrade_required", "api":api})));
                            return Ok(out);
                        }
                        self.online = true;
                        self.redial.reset();
                        self.cursor.reached(seq);
                        out.push(Effect::SendFrame {
                            generation,
                            frame: self.surface.frame(),
                        });
                        if self.ambient_wanted {
                            out.push(Effect::Capture { enabled: true });
                        }
                        if let Some(key) = &self.watch {
                            out.push(Effect::SendFrame {
                                generation,
                                frame: json!({"type":"sessions_watch", "key":key}),
                            });
                        }
                        let missing: Vec<_> = self
                            .deliveries
                            .iter()
                            .filter(|(_, d)| {
                                matches!(d.state, DeliveryState::Accepted | DeliveryState::Cleaned)
                                    && !messages.contains_key(&d.record.id)
                            })
                            .map(|(id, _)| id.clone())
                            .collect();
                        for id in missing {
                            out.extend(self.settle(&id, DeliveryState::Failed));
                        }
                        out.push(Self::emit(frame));
                        out.extend(self.pump(now_ms));
                    }
                    LiveEvent::Row { seq, row } => {
                        if self.cursor.admit(seq) {
                            if let Some(id) = &row.utt_id {
                                out.extend(self.settle(id, DeliveryState::Logged));
                            }
                            out.push(Self::emit(json!({"type":"row", "seq":seq, "row":row})));
                            out.extend(self.pump(now_ms));
                        }
                    }
                    LiveEvent::Cleaned { id, text } => {
                        if self.deliveries.get(&id).is_some_and(|d| {
                            matches!(
                                d.state,
                                DeliveryState::Sending
                                    | DeliveryState::Retrying
                                    | DeliveryState::Accepted
                            )
                        }) {
                            out.extend(self.settle(&id, DeliveryState::Cleaned));
                            out.push(Self::emit(json!({"type":"cleaned", "id":id, "text":text})));
                            out.extend(self.pump(now_ms));
                        }
                    }
                    LiveEvent::Failed { id, reason } => {
                        out.extend(self.settle(&id, DeliveryState::Failed));
                        out.push(Self::emit(
                            json!({"type":"message_failed", "id":id, "reason":reason}),
                        ));
                        out.extend(self.pump(now_ms));
                    }
                    LiveEvent::Other { frame } => {
                        if frame["type"] == "sessions" {
                            self.notifiable.clear();
                            if let Some(list) = frame["sessions"].as_array() {
                                for item in list {
                                    if let Some(key) = item["key"].as_str() {
                                        self.notifiable.insert(
                                            key.into(),
                                            item["notify"].as_bool().unwrap_or(false),
                                        );
                                    }
                                }
                            }
                        }
                        let audio = match self.ambient.frame(&frame, now_ms) {
                            Ok(audio) => audio,
                            Err(error)
                                if matches!(
                                    error.code.as_str(),
                                    "unsupported_audio_rate" | "unsupported_media"
                                ) =>
                            {
                                self.wanted = false;
                                self.ambient_wanted = false;
                                out.extend(self.retire());
                                out.push(Effect::Capture { enabled: false });
                                out.push(Self::emit(
                                    json!({"type":"protocol_error", "code":error.code}),
                                ));
                                return Ok(out);
                            }
                            Err(error) => return Err(error),
                        };
                        out.extend(self.audio(audio));
                        out.push(Self::emit(frame));
                    }
                }
                Ok(out)
            }
            Command::LiveBinary { generation, bytes } => {
                if generation != self.generation {
                    return Ok(vec![]);
                }
                Ok(self.audio(self.ambient.binary(&bytes)?))
            }
            Command::SetAmbient { enabled } => {
                if enabled == self.ambient_wanted {
                    return Ok(vec![]);
                }
                self.ambient_wanted = enabled;
                if enabled && self.online && !self.live_audio {
                    return self.open();
                }
                let mut out = if self.online || !enabled {
                    vec![Effect::Capture { enabled }]
                } else {
                    vec![]
                };
                if !enabled {
                    let audio = self.ambient.release_capture();
                    out.extend(self.audio(audio));
                }
                if self.online {
                    out.push(Effect::SendFrame {
                        generation: self.generation,
                        frame: self.surface.frame(),
                    });
                }
                Ok(out)
            }
            Command::CapturePacket { packet, aec } => {
                validate_packet(&packet)?;
                if !self.ambient_wanted || !self.online {
                    return Ok(vec![]);
                }
                let actions = self.ambient.packet_encoded(aec);
                let mut out = self.audio(actions);
                if let Some(bytes) = self.ambient.uplink(&packet)? {
                    out.push(Effect::SendBinary {
                        generation: self.generation,
                        bytes,
                    });
                }
                Ok(out)
            }
            Command::Press { source, active } => {
                if source.is_empty() || source.len() > 128 {
                    return Err(Error::invalid("Invalid input source"));
                }
                let audio = self.ambient.press(&source, active);
                Ok(self.audio(audio))
            }
            Command::Takeover => {
                let audio = self.ambient.takeover();
                Ok(self.audio(audio))
            }
            Command::Hush => {
                let audio = self.ambient.hush();
                Ok(self.audio(audio))
            }
            Command::ReadBegan => {
                let audio = self.ambient.read_began();
                Ok(self.audio(audio))
            }
            Command::ReadEnded => {
                self.ambient.read_ended();
                Ok(vec![])
            }
            Command::PlaybackScheduled { generation } => {
                self.ambient.scheduled(generation);
                Ok(vec![])
            }
            Command::Decoded {
                generation,
                samples,
                now_ms,
            } => {
                self.ambient.decoded(generation, samples, now_ms);
                Ok(vec![])
            }
            Command::Played {
                generation,
                ms,
                skipped,
            } => {
                let audio = self.ambient.played(generation, ms, skipped);
                Ok(self.audio(audio))
            }
            Command::Tick {
                now_ms,
                hardware_pending,
            } => {
                self.ambient.tick(now_ms, hardware_pending);
                Ok(vec![])
            }
            Command::SetSurface { surface } => {
                self.surface = surface;
                Ok(if self.online {
                    vec![Effect::SendFrame {
                        generation: self.generation,
                        frame: surface.frame(),
                    }]
                } else {
                    vec![]
                })
            }
            Command::SendText {
                id,
                text,
                attachments,
                place,
                now_ms,
            } => {
                let request =
                    self.api
                        .text(&id, &text, &attachments, self.surface, place.as_deref())?;
                self.prepare(request, false, now_ms)
            }
            Command::SendVoice {
                id,
                body,
                source,
                place,
                now_ms,
            } => {
                let request =
                    self.api
                        .voice(&id, &body, &source, self.surface, place.as_deref(), None)?;
                self.prepare(request, true, now_ms)
            }
            Command::Restore {
                mut message,
                now_ms,
            } => {
                if message.version != 1 || message.room != self.api.room() {
                    return Err(Error::invalid("Stored message version or room mismatch"));
                }
                validate_message_id(&message.id)?;
                if message.headers.get("Idempotency-Key") != Some(&message.id)
                    || message.headers.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "Idempotency-Key"
                                | "Content-Type"
                                | "X-Voice-Source"
                                | "X-Ambient-Display"
                                | "X-Ambient-Spoken"
                                | "X-Ambient-Place"
                        )
                    })
                {
                    return Err(Error::invalid("Invalid stored message headers"));
                }
                // The server canonicalizes UUIDs, so restore the same identity
                // used by newly constructed text and voice requests.
                message
                    .headers
                    .insert("Idempotency-Key".into(), message.id.to_ascii_lowercase());
                if message.voice {
                    validate_voice(&message.body)?;
                } else {
                    let _: Value = serde_json::from_slice(&message.body)?;
                }
                let request = self.api.request(
                    "POST",
                    format!("/api/messages?room={}", query(self.api.room())),
                    message.headers,
                    message.body,
                )?;
                self.prepare(request, message.voice, now_ms)
            }
            Command::Persisted {
                id,
                generation,
                ok,
                now_ms,
            } => {
                let Some(d) = self
                    .deliveries
                    .get_mut(&id)
                    .filter(|d| d.generation == generation && d.state == DeliveryState::Prepared)
                else {
                    return Ok(vec![]);
                };
                if !ok {
                    d.state = DeliveryState::Failed;
                    return Ok(vec![Self::emit(
                        json!({"type":"delivery", "id":id, "state":"failed", "reason":"storage_failed"}),
                    )]);
                }
                d.state = DeliveryState::Ready;
                Ok(self.pump(now_ms))
            }
            Command::HttpResult {
                id,
                generation,
                status,
                body,
                now_ms,
            } => {
                if !self.deliveries.get(&id).is_some_and(|d| {
                    d.generation == generation && d.state == DeliveryState::Sending
                }) {
                    return Ok(vec![]);
                }
                if unauthorized(status, &body) {
                    return Ok(self.revoke());
                }
                let mut out = vec![];
                let d = self
                    .deliveries
                    .get_mut(&id)
                    .expect("validated delivery exists");
                if status == 202 {
                    d.state = DeliveryState::Accepted;
                    out.push(Self::emit(
                        json!({"type":"delivery", "id":id, "state":"accepted"}),
                    ));
                } else if status == 0 || status >= 500 {
                    if let Some(delay_ms) = d.retry.failed(now_ms) {
                        d.state = DeliveryState::Retrying;
                        out.push(Effect::Schedule {
                            delay_ms,
                            command: Command::Retry {
                                id,
                                generation,
                                now_ms: now_ms.saturating_add(delay_ms),
                            },
                        });
                    } else {
                        d.state = DeliveryState::Failed;
                        out.push(Self::emit(json!({"type":"delivery", "id":id, "state":"failed", "reason":"deadline_exceeded"})));
                    }
                } else {
                    d.state = DeliveryState::Failed;
                    out.push(Self::emit(
                        json!({"type":"delivery", "id":id, "state":"failed", "status":status}),
                    ));
                }
                out.extend(self.pump(now_ms));
                Ok(out)
            }
            Command::Retry {
                id,
                generation,
                now_ms,
            } => {
                let Some(d) = self
                    .deliveries
                    .get_mut(&id)
                    .filter(|d| d.generation == generation && d.state == DeliveryState::Retrying)
                else {
                    return Ok(vec![]);
                };
                d.state = DeliveryState::Ready;
                Ok(self.pump(now_ms))
            }
            Command::Discard { id, now_ms } => {
                let mut out = self.settle(&id, DeliveryState::Discarded);
                out.extend(self.pump(now_ms));
                Ok(out)
            }
            Command::WatchSessions { key } => {
                if !self.scope.can_view() {
                    return Err(Error::new(
                        "forbidden",
                        "Device scope does not allow sessions",
                    ));
                }
                self.watch = Some(key.clone());
                Ok(if self.online {
                    vec![Effect::SendFrame {
                        generation: self.generation,
                        frame: json!({"type":"sessions_watch", "key":key}),
                    }]
                } else {
                    vec![]
                })
            }
            Command::UnwatchSessions => {
                let was = self.watch.take().is_some();
                Ok(if was && self.online {
                    vec![Effect::SendFrame {
                        generation: self.generation,
                        frame: json!({"type":"sessions_unwatch"}),
                    }]
                } else {
                    vec![]
                })
            }
            Command::NotifySession { id, key, text } => {
                if !self.scope.can_notify() {
                    return Err(Error::new(
                        "forbidden",
                        "Device scope does not allow notifications",
                    ));
                }
                if !self.online {
                    return Err(Error::new("not_connected", "Session viewer is offline"));
                }
                if self.notifiable.get(&key) != Some(&true) {
                    return Err(Error::new(
                        "not_notifiable",
                        "Target has not advertised notification support",
                    ));
                }
                if id.is_empty() || text.trim().is_empty() {
                    return Err(Error::invalid("Missing notification identity or text"));
                }
                Ok(vec![Effect::SendFrame {
                    generation: self.generation,
                    frame: json!({"type":"session_notify", "id":id, "key":key, "text":text}),
                }])
            }
        }
    }
}
