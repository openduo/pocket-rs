// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use pocket_core::{
    api::Surface,
    audio::{Ambient, PacketStream, frame_packets},
    pairing::PairingLink,
    retry::RedialBackoff,
};
use serde_json::{Value, json};

fn vectors() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/ios-vectors.json")).unwrap()
}

#[test]
fn original_swift_pairing_vectors() {
    for case in vectors()["pairing"].as_array().unwrap() {
        let actual = PairingLink::parse(case["input"].as_str().unwrap())
            .ok()
            .map(|p| serde_json::to_value(p).unwrap())
            .unwrap_or(Value::Null);
        assert_eq!(actual, case["expected"], "{}", case["input"]);
    }
}
#[test]
fn original_swift_voice_and_surface_vectors() {
    let data = vectors();
    for case in data["voice"].as_array().unwrap() {
        let packets: Vec<Vec<u8>> = serde_json::from_value(case["packets"].clone()).unwrap();
        let body = frame_packets(&packets).unwrap();
        assert_eq!(serde_json::to_value(&body).unwrap(), case["body"]);
        let mut parser = PacketStream::default();
        assert_eq!(parser.push(&body).unwrap(), packets);
        parser.finish().unwrap();
        assert_eq!(json!(packets.len() as u64 * 20), case["duration_ms"]);
    }
    for case in data["surfaces"].as_array().unwrap() {
        let surface: Surface = serde_json::from_value(case["input"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(surface.headers()).unwrap(),
            case["headers"]
        );
        assert_eq!(surface.frame(), case["frame"]);
    }
    let mut backoff = RedialBackoff::default();
    for expected in data["redial_ms"].as_array().unwrap() {
        assert_eq!(json!(backoff.failed()), *expected);
    }
}
#[test]
fn original_swift_ambient_scenarios() {
    for scenario in vectors()["ambient"].as_array().unwrap() {
        let mut ambient = Ambient::new("fixture-room");
        let mut presses = 0u64;
        for (step, result) in scenario["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(scenario["results"].as_array().unwrap())
        {
            let generation = step["generation"].as_u64().unwrap_or_default();
            let ms = step["ms"].as_u64().unwrap_or_default();
            let actions = match step["op"].as_str().unwrap() {
                "open" => {
                    ambient.socket_opened();
                    vec![]
                }
                "close" => ambient.socket_closed(),
                "packet" => ambient.packet_encoded(step["aec"].as_bool().unwrap_or(false)),
                "frame" => ambient.frame(&step["frame"], 0).unwrap(),
                "takeover" => ambient.takeover(),
                "press_begin" => {
                    presses += 1;
                    ambient.press(&format!("source-{presses}"), true)
                }
                "press_end" => {
                    let id = format!("source-{presses}");
                    presses = presses.saturating_sub(1);
                    ambient.press(&id, false)
                }
                "read_begin" => ambient.read_began(),
                "read_end" => {
                    ambient.read_ended();
                    vec![]
                }
                "hush" => ambient.hush(),
                "binary" => ambient
                    .binary(&serde_json::from_value::<Vec<u8>>(step["bytes"].clone()).unwrap())
                    .unwrap(),
                "scheduled" => {
                    ambient.scheduled(generation);
                    vec![]
                }
                "played" => ambient.played(generation, ms, false),
                "skipped" => ambient.played(generation, ms, true),
                _ => panic!("Unknown vector step"),
            };
            assert_eq!(
                serde_json::to_value(actions).unwrap(),
                result["actions"],
                "{}: {}",
                scenario["name"],
                step
            );
            let state = json!({"conn":ambient.conn, "role":ambient.role, "hello_sent":ambient.hello_sent, "sends_audio":ambient.sends_audio(), "generation":ambient.generation, "current":ambient.current, "reading":ambient.reading, "pending_blocks":ambient.pending_blocks});
            assert_eq!(state, result["state"], "{}: {}", scenario["name"], step);
        }
    }
}
