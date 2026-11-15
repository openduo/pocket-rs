// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use crate::{
    Error, Result,
    audio::validate_voice,
    pairing::{Endpoint, needs_token},
};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const QUERY: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');
pub fn query(text: &str) -> String {
    utf8_percent_encode(text, QUERY).to_string()
}
pub fn validate_header(value: &str) -> Result<()> {
    if value.chars().any(|c| c.is_control()) {
        return Err(Error::invalid("Control character in header"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    PhoneAwake,
    Phone,
    Passport,
    Later,
}
impl Display {
    pub fn wire(self) -> &'static str {
        match self {
            Self::PhoneAwake => "phone_awake",
            Self::Phone => "phone",
            Self::Passport => "passport",
            Self::Later => "later",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    pub display: Display,
    pub spoken: bool,
}
impl Surface {
    pub fn frame(self) -> Value {
        json!({"type":"surface", "display":self.display, "spoken":self.spoken})
    }
    pub fn headers(self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("X-Ambient-Display".into(), self.display.wire().into()),
            (
                "X-Ambient-Spoken".into(),
                if self.spoken { "1" } else { "0" }.into(),
            ),
        ])
    }
}
impl Default for Surface {
    fn default() -> Self {
        Self {
            display: Display::Later,
            spoken: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct Api {
    pub endpoint: Endpoint,
    room: String,
    token: String,
}
impl Api {
    pub fn new(endpoint: Endpoint, room: &str, token: &str) -> Result<Self> {
        validate_header(token)?;
        if room.is_empty() || token.is_empty() {
            return Err(Error::invalid("Room and token are required"));
        }
        Ok(Self {
            endpoint,
            room: room.into(),
            token: token.into(),
        })
    }
    pub fn room(&self) -> &str {
        &self.room
    }
    pub fn request(
        &self,
        method: &str,
        path: String,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Result<Request> {
        self.endpoint.url(&path)?;
        let mut headers = headers;
        if headers.keys().any(|k| {
            k.eq_ignore_ascii_case("authorization")
                || k.eq_ignore_ascii_case("cookie")
                || k.eq_ignore_ascii_case("host")
        }) {
            return Err(Error::invalid(
                "Authentication and origin headers are library-owned",
            ));
        }
        for (key, value) in &headers {
            validate_header(key)?;
            validate_header(value)?;
        }
        if needs_token(&path) {
            headers.insert("Authorization".into(), format!("Bearer {}", self.token));
        }
        if !matches!(method, "GET" | "POST" | "PUT" | "DELETE") {
            return Err(Error::invalid("Unsupported method"));
        }
        Ok(Request {
            method: method.into(),
            path,
            headers,
            body,
        })
    }
    fn path(&self, route: &str) -> String {
        format!("/api/{route}?room={}", query(&self.room))
    }
    pub fn health(&self) -> Result<Request> {
        self.request("GET", "/healthz".into(), BTreeMap::new(), vec![])
    }
    pub fn state(&self) -> Result<Request> {
        self.request("GET", self.path("state"), BTreeMap::new(), vec![])
    }
    pub fn rows(&self, cursor: u64) -> Result<Request> {
        self.request(
            "GET",
            format!("{}&since={cursor}", self.path("imlog")),
            BTreeMap::new(),
            vec![],
        )
    }
    pub fn history(&self, day: &str) -> Result<Request> {
        self.request(
            "GET",
            format!("{}&date={}", self.path("imlog"), query(day)),
            BTreeMap::new(),
            vec![],
        )
    }
    fn message_headers(
        &self,
        id: &str,
        surface: Surface,
        place: Option<&str>,
    ) -> Result<BTreeMap<String, String>> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        {
            return Err(Error::invalid("Invalid idempotency key"));
        }
        let mut headers = surface.headers();
        headers.insert("Idempotency-Key".into(), id.to_ascii_lowercase());
        if let Some(place) = place.filter(|s| !s.is_empty()) {
            validate_header(place)?;
            headers.insert("X-Ambient-Place".into(), place.into());
        }
        Ok(headers)
    }
    pub fn text(
        &self,
        id: &str,
        text: &str,
        attachments: &[Attachment],
        surface: Surface,
        place: Option<&str>,
    ) -> Result<Request> {
        if text.trim().is_empty() && attachments.is_empty() {
            return Err(Error::invalid("Empty message"));
        }
        let mut body = json!({"text":text});
        if !attachments.is_empty() {
            body["attachments"] = serde_json::to_value(attachments)?;
        }
        let mut headers = self.message_headers(id, surface, place)?;
        headers.insert("Content-Type".into(), "application/json".into());
        self.request(
            "POST",
            self.path("messages"),
            headers,
            serde_json::to_vec(&body)?,
        )
    }
    pub fn voice(
        &self,
        id: &str,
        packets: &[u8],
        source: &str,
        surface: Surface,
        place: Option<&str>,
        state: Option<&RoomState>,
    ) -> Result<Request> {
        let mut request = self.voice_head(id, packets, source, surface, place, state)?;
        request.body = packets.into();
        Ok(request)
    }
    /// Validate borrowed voice bytes and construct headers without a second owned body copy.
    pub fn voice_head(
        &self,
        id: &str,
        packets: &[u8],
        source: &str,
        surface: Surface,
        place: Option<&str>,
        state: Option<&RoomState>,
    ) -> Result<Request> {
        validate_voice(packets)?;
        if !matches!(source, "phone" | "passport") {
            return Err(Error::invalid("Unknown voice source"));
        }
        if let Some(state) = state {
            state.admit_upload(packets.len())?;
        }
        let mut headers = self.message_headers(id, surface, place)?;
        headers.insert(
            "Content-Type".into(),
            crate::audio::VOICE_CONTENT_TYPE.into(),
        );
        headers.insert("X-Voice-Source".into(), source.into());
        self.request("POST", self.path("messages"), headers, vec![])
    }
    pub fn speech(&self, text: &str) -> Result<Request> {
        self.request(
            "POST",
            self.path("speech"),
            BTreeMap::from([("Content-Type".into(), "application/json".into())]),
            serde_json::to_vec(&json!({"text":text}))?,
        )
    }
    pub fn upload(
        &self,
        name: &str,
        mime: &str,
        bytes: &[u8],
        state: Option<&RoomState>,
    ) -> Result<Request> {
        let mut request = self.upload_head(name, mime, bytes.len(), state)?;
        request.body = bytes.into();
        Ok(request)
    }
    pub fn upload_head(
        &self,
        name: &str,
        mime: &str,
        len: usize,
        state: Option<&RoomState>,
    ) -> Result<Request> {
        if name.is_empty() || mime.is_empty() {
            return Err(Error::invalid("Missing attachment metadata"));
        }
        if let Some(state) = state {
            state.admit_upload(len)?;
        }
        self.request(
            "POST",
            format!("{}&name={}", self.path("upload"), query(name)),
            BTreeMap::from([("Content-Type".into(), mime.into())]),
            vec![],
        )
    }
    pub fn attachment(&self, a: &Attachment) -> Result<Request> {
        let digest = a
            .sha256
            .as_deref()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| Error::invalid("Missing attachment digest"))?;
        self.request(
            "GET",
            format!(
                "{}&sha256={}&mime={}&name={}",
                self.path("attachment"),
                query(digest),
                query(&a.mime),
                query(&a.name)
            ),
            BTreeMap::new(),
            vec![],
        )
    }
    pub fn live(&self, cursor: Option<u64>, audio: bool, media: bool) -> Result<Request> {
        let mut path = format!("/live?room={}", query(&self.room));
        if let Some(cursor) = cursor {
            path.push_str(&format!("&since={cursor}"));
        }
        path.push_str(if audio { "&audio=1" } else { "&audio=0" });
        if media {
            path.push_str("&media=1");
        }
        self.request("GET", path, BTreeMap::new(), vec![])
    }
}

pub fn pair_request(code: &str) -> Result<Request> {
    if code.is_empty() {
        return Err(Error::invalid("Missing pairing code"));
    }
    Ok(Request {
        method: "POST".into(),
        path: "/api/pair".into(),
        headers: BTreeMap::from([("Content-Type".into(), "application/json".into())]),
        body: serde_json::to_vec(&json!({"code":code}))?,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomState {
    pub room: String,
    pub room_name: String,
    pub date: String,
    pub upload_max_bytes: Option<u64>,
    pub uploads_disabled: bool,
    pub daemon_ok: Option<bool>,
    pub cerebellum_ok: Option<bool>,
    pub protocol_mismatch: bool,
    pub config_issues: Vec<String>,
}
impl RoomState {
    pub fn decode(body: &[u8]) -> Result<Self> {
        let v: Value = serde_json::from_slice(body)?;
        let room = v["room"]
            .as_str()
            .ok_or_else(|| Error::protocol("Missing room"))?
            .to_owned();
        Ok(Self {
            room_name: v["room_name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(&room)
                .into(),
            room,
            date: v["date"].as_str().unwrap_or_default().into(),
            upload_max_bytes: v["limits"]["upload_max_bytes"].as_u64().filter(|n| *n > 0),
            uploads_disabled: v
                .get("limits")
                .and_then(|l| l.get("upload_max_bytes"))
                .is_some_and(Value::is_null),
            daemon_ok: v["daemon_ok"].as_bool(),
            cerebellum_ok: v["cerebellum_ok"].as_bool(),
            protocol_mismatch: v["cerebellum_halt"]["reason"] == "protocol",
            config_issues: v["config_issues"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
    pub fn admit_upload(&self, bytes: usize) -> Result<()> {
        if self.uploads_disabled {
            return Err(Error::new("uploads_disabled", "Room disabled uploads"));
        }
        if self
            .upload_max_bytes
            .is_some_and(|limit| bytes as u64 > limit)
        {
            return Err(Error::new(
                "upload_too_large",
                "Body exceeds advertised limit",
            ));
        }
        Ok(())
    }
}

pub fn uploaded(attachment: Attachment, bytes: &[u8], response: &[u8]) -> Result<Attachment> {
    let v: Value = serde_json::from_slice(response)?;
    let digest = format!("{:x}", Sha256::digest(bytes));
    uploaded_digest(attachment, &digest, &v)
}

/// The platform can hash a large file without marshaling it through JSON a second time.
pub fn uploaded_digest(mut attachment: Attachment, digest: &str, v: &Value) -> Result<Attachment> {
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::invalid("Invalid body digest"));
    }
    let path = v["path"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::protocol("Upload acknowledgement lacks path"))?;
    if let Some(supplied) = v["sha256"].as_str().filter(|s| !s.is_empty())
        && !supplied.eq_ignore_ascii_case(digest)
    {
        return Err(Error::new(
            "digest_mismatch",
            "Upload acknowledgement differs from body",
        ));
    }
    attachment.path = Some(path.into());
    attachment.sha256 = Some(digest.to_ascii_lowercase());
    Ok(attachment)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestCommand {
    Health,
    State,
    Rows {
        cursor: u64,
    },
    History {
        date: String,
    },
    Text {
        id: String,
        text: String,
        #[serde(default)]
        attachments: Vec<Attachment>,
        #[serde(default)]
        surface: Surface,
        #[serde(default)]
        place: Option<String>,
    },
    Voice {
        id: String,
        #[serde(default)]
        body: Vec<u8>,
        source: String,
        #[serde(default)]
        surface: Surface,
        #[serde(default)]
        place: Option<String>,
    },
    Speech {
        text: String,
    },
    Upload {
        name: String,
        mime: String,
        #[serde(default)]
        body: Vec<u8>,
    },
    Attachment {
        attachment: Attachment,
    },
    Live {
        #[serde(default)]
        cursor: Option<u64>,
        #[serde(default)]
        audio: bool,
        #[serde(default)]
        media: bool,
    },
}
impl RequestCommand {
    pub fn build(self, api: &Api) -> Result<Request> {
        match self {
            Self::Health => api.health(),
            Self::State => api.state(),
            Self::Rows { cursor } => api.rows(cursor),
            Self::History { date } => api.history(&date),
            Self::Text {
                id,
                text,
                attachments,
                surface,
                place,
            } => api.text(&id, &text, &attachments, surface, place.as_deref()),
            Self::Voice {
                id,
                body,
                source,
                surface,
                place,
            } => api.voice(&id, &body, &source, surface, place.as_deref(), None),
            Self::Speech { text } => api.speech(&text),
            Self::Upload { name, mime, body } => api.upload(&name, &mime, &body, None),
            Self::Attachment { attachment } => api.attachment(&attachment),
            Self::Live {
                cursor,
                audio,
                media,
            } => api.live(cursor, audio, media),
        }
    }
}
