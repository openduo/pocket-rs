// Copyright 2026 openduo and OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use crate::{Error, Result, api::Attachment};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    #[serde(default)]
    pub seq: Option<u64>,
    pub at: String,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub text: String,
    #[serde(default)]
    pub utt_id: Option<String>,
    #[serde(default)]
    pub attachments: Option<Vec<Attachment>>,
    #[serde(default)]
    pub message_ids: Option<Vec<String>>,
    #[serde(default)]
    pub truncated: Option<bool>,
    #[serde(default)]
    pub unspoken: Option<bool>,
    #[serde(default)]
    pub voice_source: Option<String>,
    #[serde(default)]
    pub full_text: Option<String>,
}
impl Row {
    pub fn shown_text(&self) -> &str {
        self.full_text.as_deref().unwrap_or(&self.text)
    }
    pub fn key(&self) -> String {
        if let Some(seq) = self.seq {
            return format!("seq:{seq}");
        }
        if self.kind.as_deref() == Some("typed")
            && let Some(id) = self.utt_id.as_deref().filter(|s| !s.is_empty())
        {
            return format!("typed:{id}");
        }
        format!(
            "{}|{}|{}",
            self.at,
            self.kind.as_deref().unwrap_or_default(),
            self.text
        )
    }
    pub fn is_answer(&self) -> bool {
        self.kind.as_deref() != Some("reaction")
            && (self.kind.as_deref() == Some("answer") || self.speaker.as_deref() == Some("多多"))
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowCursor {
    pub seq: Option<u64>,
}
impl RowCursor {
    pub fn admit(&mut self, seq: u64) -> bool {
        if self.seq.is_some_and(|old| seq <= old) {
            return false;
        }
        self.seq = Some(seq);
        true
    }
    pub fn reached(&mut self, seq: u64) {
        self.seq = Some(self.seq.unwrap_or(seq).max(seq));
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveEvent {
    Ready {
        seq: u64,
        api: Option<u64>,
        messages: BTreeMap<String, String>,
        turn: Option<Value>,
    },
    Row {
        seq: u64,
        row: Row,
    },
    Cleaned {
        id: String,
        text: String,
    },
    Failed {
        id: String,
        reason: String,
    },
    Other {
        frame: Value,
    },
}
impl LiveEvent {
    pub fn decode(frame: &Value) -> Result<Self> {
        Ok(
            match frame["type"]
                .as_str()
                .ok_or_else(|| Error::protocol("Missing live frame type"))?
            {
                "ready" => {
                    let seq = frame["seq"]
                        .as_u64()
                        .ok_or_else(|| Error::protocol("Missing ready cursor"))?;
                    let mut messages = BTreeMap::new();
                    if let Some(list) = frame["messages"].as_array() {
                        for item in list {
                            let id = item["id"]
                                .as_str()
                                .ok_or_else(|| Error::protocol("Malformed ready message"))?;
                            messages.insert(
                                id.into(),
                                item["state"].as_str().unwrap_or("accepted").into(),
                            );
                        }
                    }
                    Self::Ready {
                        seq,
                        api: frame["api"].as_u64(),
                        messages,
                        turn: frame.get("turn").filter(|v| !v.is_null()).cloned(),
                    }
                }
                "row" => {
                    let seq = frame["seq"]
                        .as_u64()
                        .ok_or_else(|| Error::protocol("Missing row cursor"))?;
                    let mut row: Row = serde_json::from_value(frame["row"].clone())?;
                    if row.seq.is_none() {
                        row.seq = Some(seq);
                    }
                    Self::Row { seq, row }
                }
                "message" if frame["state"] == "cleaned" => Self::Cleaned {
                    id: required(frame, "id")?,
                    text: frame["text"].as_str().unwrap_or_default().into(),
                },
                "message" if frame["state"] == "failed" => Self::Failed {
                    id: required(frame, "id")?,
                    reason: frame["reason"].as_str().unwrap_or_default().into(),
                },
                _ => Self::Other {
                    frame: frame.clone(),
                },
            },
        )
    }
}
fn required(v: &Value, field: &str) -> Result<String> {
    v[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::protocol(format!("Missing {field}")))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Settlement {
    Open(String),
    Logged,
    Failed,
}
pub fn settlement(id: &str, messages: &BTreeMap<String, String>, logged: bool) -> Settlement {
    if let Some(state) = messages.get(id) {
        Settlement::Open(state.clone())
    } else if logged {
        Settlement::Logged
    } else {
        Settlement::Failed
    }
}
