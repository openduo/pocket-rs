// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
//! Granular owners for incremental platform adoption; never run beside a Session for the same work.
use pocket_core::{
    Error, Result,
    audio::{Ambient, AudioAction, PacketStream},
};
use serde_json::{Value, json};

pub enum Component {
    Input(Ambient),
    Output(Ambient),
    Packets(PacketStream),
}
fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str> {
    v[name]
        .as_str()
        .ok_or_else(|| Error::invalid("Missing string command field"))
}
fn number(v: &Value, name: &str) -> Result<u64> {
    v[name]
        .as_u64()
        .ok_or_else(|| Error::invalid("Missing nonnegative command field"))
}
fn flag(v: &Value, name: &str) -> Result<bool> {
    v[name]
        .as_bool()
        .ok_or_else(|| Error::invalid("Missing boolean command field"))
}
fn frames(actions: Vec<AudioAction>) -> Value {
    json!(
        actions
            .into_iter()
            .filter_map(|a| match a {
                AudioAction::Send { frame } => Some(frame),
                _ => None,
            })
            .collect::<Vec<_>>()
    )
}
impl Component {
    pub fn new(v: &Value) -> Result<Self> {
        match text(v, "type")? {
            "ambient_input" => Ok(Self::Input(Ambient::new(""))),
            "ambient_output" => Ok(Self::Output(Ambient::new(""))),
            "packet_stream" => Ok(Self::Packets(PacketStream::default())),
            _ => Err(Error::invalid("Unknown component type")),
        }
    }
    pub fn dispatch(&mut self, v: &Value) -> Result<Value> {
        let op = text(v, "type")?;
        let mut result = Value::Null;
        match self {
            Self::Input(a) => match op {
                "state" => {}
                "opened" => a.socket_opened(),
                "stopped" => {
                    a.capturing = false;
                    a.hello_sent = false;
                }
                "packet" => {
                    a.room = text(v, "room")?.into();
                    result = frames(a.packet_encoded(flag(v, "aec")?));
                }
                "meta" | "frame" => {
                    a.room = text(v, "room")?.into();
                    result = frames(a.frame(&v["frame"], 0)?);
                }
                "takeover" => {
                    a.room = text(v, "room")?.into();
                    result = frames(a.takeover());
                }
                "press" => result = frames(a.press(text(v, "source")?, flag(v, "active")?)),
                "release" => result = json!(a.microphone_restore()),
                "mic_frame" => result = json!({"type":"mute", "on":!a.muted()}),
                _ => return Err(Error::invalid("Unknown input command")),
            },
            Self::Output(a) => match op {
                "state" => {}
                "speech" => {
                    a.frame(
                        &json!({"type":"speech", "speech_id":text(v, "id")?}),
                        number(v, "now_ms")?,
                    )?;
                }
                "read" => a.output_read(text(v, "id")?, number(v, "now_ms")?)?,
                "stop" => a.stop_output(),
                "received" => a.decoded(a.generation, number(v, "samples")?, number(v, "now_ms")?),
                "completed" => {
                    let mut frame = v["frame"].clone();
                    frame["type"] = json!("playback");
                    let f = &frame;
                    result = json!(
                        !a.reading
                            && a.current.is_some()
                            && f["state"] == "done"
                            && f["speech_id"].as_str() == a.current.as_deref()
                            && f["played_ms"].as_u64().is_some()
                    );
                    if result == true {
                        a.frame(f, number(v, "now_ms")?)?;
                    }
                }
                "expire" => {
                    let before = a.receiving;
                    a.tick(number(v, "now_ms")?, flag(v, "hardware_pending")?);
                    result = json!(before && !a.receiving);
                }
                "accepts_stop" => result = json!(a.accepts_stop(&v["frame"])),
                "interrupted" => {
                    if flag(v, "reports")? && !a.reading && a.current.is_some() {
                        result = json!({"type":"interrupted", "speech_id":a.current});
                    }
                }
                "played" => {
                    if text(v, "key")? == format!("output:{}", a.generation) {
                        result = json!(a.output_position(
                            a.generation,
                            number(v, "ms")?,
                            0,
                            flag(v, "reports")?
                        ));
                    }
                }
                "skipped" => {
                    result = json!(a.output_position(
                        a.generation,
                        0,
                        number(v, "samples")?,
                        flag(v, "reports")?
                    ))
                }
                _ => return Err(Error::invalid("Unknown output command")),
            },
            Self::Packets(p) => match op {
                "state" => {}
                "finish" => p.finish()?,
                _ => return Err(Error::invalid("Unknown packet command")),
            },
        }
        Ok(json!({"result":result, "state":self.snapshot()}))
    }
    pub fn binary(&mut self, kind: u64, bytes: &[u8]) -> Result<Value> {
        let result = match self {
            Self::Packets(p) if kind == 0 => json!(p.push(bytes)?),
            Self::Packets(_) if kind == 3 => json!(pocket_core::audio::validate_voice(bytes)?),
            Self::Input(a) if kind == 1 => json!(a.uplink(bytes)?),
            Self::Input(a) if kind == 2 => json!(a.downlink(bytes)?),
            _ => return Err(Error::invalid("Invalid binary component operation")),
        };
        Ok(json!({"result":result, "state":self.snapshot()}))
    }
    fn snapshot(&self) -> Value {
        match self {
            Self::Input(a) => {
                json!({"role":a.role, "room_state":a.room_state, "hello":a.hello_sent, "media":a.framed, "meta_seen":a.meta_seen, "muted":a.muted(), "sends_audio":a.sends_audio()})
            }
            Self::Output(a) => {
                json!({"id":a.current, "reading":a.reading, "receiving":a.receiving, "played_ms":a.played_ms, "key":a.current.as_ref().map(|_| format!("output:{}", a.generation))})
            }
            Self::Packets(p) => json!({"partial":p.buffered_bytes()}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiated_input_keeps_raw_downlink_bytes() {
        let mut input = Component::new(&json!({"type":"ambient_input"})).unwrap();
        input
            .dispatch(&json!({"type":"meta", "room":"prepared", "frame":{
                "type":"meta", "conn":"prepared", "media":1
            }}))
            .unwrap();
        for bytes in [&[0xb8, 0xff, 0xfe][..], &[1, 2, 3][..]] {
            assert_eq!(input.binary(2, bytes).unwrap()["result"], json!(bytes));
        }
        assert!(input.binary(2, &[]).is_err());
        assert!(input.binary(2, &[0; 1276]).is_err());
    }
}
