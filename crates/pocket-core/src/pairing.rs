// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use crate::{Error, Result};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EndpointInput")]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

#[derive(Deserialize)]
struct EndpointInput {
    host: String,
    port: u16,
    tls: bool,
}

impl TryFrom<EndpointInput> for Endpoint {
    type Error = Error;
    fn try_from(v: EndpointInput) -> Result<Self> {
        Self::new(&v.host, v.port, v.tls)
    }
}

impl Endpoint {
    pub fn new(host: &str, port: u16, tls: bool) -> Result<Self> {
        if host.chars().any(char::is_control) {
            return Err(Error::invalid("Control character in host"));
        }
        let host = host.trim();
        if host.is_empty()
            || port == 0
            || host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || "/@?#\\%".contains(c))
        {
            return Err(Error::invalid(
                "Expected a hostname or IP and a nonzero port",
            ));
        }
        let authority = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.into()
        };
        let parsed = url::Url::parse(&format!("https://{authority}:{port}/"))
            .map_err(|_| Error::invalid("Invalid host"))?;
        if parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(Error::invalid("Invalid host"));
        }
        let canonical = parsed
            .host_str()
            .unwrap_or_default()
            .trim_matches(['[', ']'])
            .to_owned();
        Ok(Self {
            host: canonical,
            port,
            tls,
        })
    }

    pub fn origin(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        let authority = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let port = if self.port == if self.tls { 443 } else { 80 } {
            String::new()
        } else {
            format!(":{}", self.port)
        };
        format!("{scheme}://{authority}{port}")
    }

    /// Only origin-relative paths may carry this endpoint's device credential.
    pub fn url(&self, path: &str) -> Result<String> {
        if !path.starts_with('/')
            || path.starts_with("//")
            || path.contains(['\r', '\n', '#', '\\'])
        {
            return Err(Error::invalid("Expected an origin-relative request path"));
        }
        let raw = format!("{}{path}", self.origin());
        let url = url::Url::parse(&raw).map_err(|_| Error::invalid("Invalid request path"))?;
        if url.origin().ascii_serialization() != self.origin() {
            return Err(Error::invalid("Request changed origin"));
        }
        Ok(raw)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingLink {
    pub endpoint: Endpoint,
    pub code: String,
}

impl std::fmt::Debug for PairingLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingLink")
            .field("endpoint", &self.endpoint)
            .field("code", &"[REDACTED]")
            .finish()
    }
}

impl PairingLink {
    pub fn parse(text: &str) -> Result<Self> {
        let start = text
            .to_ascii_lowercase()
            .find("duoduopocket://")
            .ok_or_else(|| Error::invalid("Pairing link not found"))?;
        let raw: String = text[start..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !"\"'<>“”「」".contains(*c))
            .collect();
        let url = url::Url::parse(&raw).map_err(|_| Error::invalid("Malformed pairing link"))?;
        if !url.scheme().eq_ignore_ascii_case("duoduopocket")
            || !url
                .host_str()
                .is_some_and(|h| h.eq_ignore_ascii_case("pair"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || !matches!(url.path(), "" | "/")
            || url.fragment().is_some()
        {
            return Err(Error::invalid("Invalid pairing action"));
        }
        let decode = |v: &str| -> Result<String> {
            let bytes = v.as_bytes();
            for (i, b) in bytes.iter().enumerate() {
                if *b == b'%'
                    && (i + 2 >= bytes.len()
                        || !bytes[i + 1].is_ascii_hexdigit()
                        || !bytes[i + 2].is_ascii_hexdigit())
                {
                    return Err(Error::invalid("Invalid query escape"));
                }
            }
            percent_decode_str(v)
                .decode_utf8()
                .map(|v| v.into_owned())
                .map_err(|_| Error::invalid("Invalid query UTF-8"))
        };
        let mut query = BTreeMap::new();
        for item in url.query().unwrap_or_default().split('&') {
            let (key, value) = item.split_once('=').unwrap_or((item, ""));
            query.entry(decode(key)?).or_insert(decode(value)?);
        }
        let host = query.get("host").map(String::as_str).unwrap_or_default();
        let code = query.get("code").map(|v| v.trim()).unwrap_or_default();
        if code.is_empty() || code.chars().any(char::is_control) {
            return Err(Error::invalid("Missing or invalid pairing code"));
        }
        let port = match query.get("port").map(String::as_str) {
            None | Some("") => 443,
            Some(p) => p
                .parse::<u16>()
                .map_err(|_| Error::invalid("Port must be 1..65535"))?,
        };
        let tls = match query.get("tls").map(|v| v.to_ascii_lowercase()).as_deref() {
            None | Some("" | "1" | "true") => true,
            Some("0" | "false") => false,
            _ => return Err(Error::invalid("Invalid TLS setting")),
        };
        Ok(Self {
            endpoint: Endpoint::new(host, port, tls)?,
            code: code.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceScope {
    Room,
    Sessions,
    Notify,
}
impl DeviceScope {
    pub fn can_view(self) -> bool {
        self != Self::Room
    }
    pub fn can_notify(self) -> bool {
        self == Self::Notify
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    #[serde(default)]
    pub name: String,
    pub room: String,
    #[serde(default)]
    pub room_name: String,
    pub scope: DeviceScope,
    pub token: String,
}
impl std::fmt::Debug for PairedDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedDevice")
            .field("device_id", &self.device_id)
            .field("scope", &self.scope)
            .field("token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}
impl PairedDevice {
    pub fn decode(body: &[u8]) -> Result<Self> {
        let mut device: Self = serde_json::from_slice(body)?;
        if device.device_id.is_empty() || device.room.is_empty() || device.token.is_empty() {
            return Err(Error::protocol("Incomplete pairing result"));
        }
        crate::api::validate_header(&device.token)?;
        if device.room_name.is_empty() {
            device.room_name.clone_from(&device.room);
        }
        Ok(device)
    }
}

pub fn needs_token(path: &str) -> bool {
    !matches!(path.split('?').next(), Some("/healthz" | "/api/pair"))
}
pub fn unauthorized(status: u16, body: &[u8]) -> bool {
    status == 401
        && serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
            .as_deref()
            == Some("unauthorized")
}
