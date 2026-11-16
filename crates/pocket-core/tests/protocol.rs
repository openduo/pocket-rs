// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use pocket_core::{
    api::{self, Api, Attachment, RoomState, Surface},
    audio::{Ambient, AudioAction, PacketStream, frame_packets},
    live::RowCursor,
    pairing::{DeviceScope, Endpoint, PairingLink, unauthorized},
    retry::VoiceRetry,
    session::{Command, Effect, Session, SessionConfig},
};
use serde_json::json;

#[test]
fn media_v1_frames_only_the_uplink() {
    let mut ambient = Ambient::new("prepared");
    ambient.socket_opened();
    ambient.packet_encoded(false);
    ambient
        .frame(
            &json!({"type":"meta", "conn":"prepared", "media":1, "role":"master"}),
            0,
        )
        .unwrap();
    ambient
        .frame(&json!({"type":"speech", "speech_id":"prepared"}), 1)
        .unwrap();
    for packet in [&[0xb8, 0xff, 0xfe][..], &[1, 2, 3][..], &[2][..]] {
        assert_eq!(
            ambient.uplink(packet).unwrap(),
            Some([&[1], packet].concat())
        );
        assert_eq!(ambient.downlink(packet).unwrap(), packet);
        assert_eq!(
            ambient.binary(packet).unwrap(),
            vec![AudioAction::Decode {
                generation: ambient.generation,
                packet: packet.to_vec()
            }]
        );
    }
    assert!(ambient.downlink(&[]).is_err());
    assert!(ambient.downlink(&[0; 1276]).is_err());
    assert_eq!(ambient.downlink(&[0; 1275]).unwrap().len(), 1275);
}

fn config() -> SessionConfig {
    SessionConfig {
        endpoint: Endpoint::new("example.test", 443, true).unwrap(),
        room: "test&房间".into(),
        token: "prepared-token".into(),
        scope: DeviceScope::Room,
        surface: Surface::default(),
        cursor: None,
        media: false,
    }
}
fn session() -> Session {
    Session::new(config()).unwrap()
}

#[test]
fn incompatible_audio_retires_transport_and_capture() {
    let mut s = session();
    let generation = opened(&mut s);
    s.dispatch(Command::SetAmbient { enabled: true }).unwrap();
    let effects = s
        .dispatch(Command::LiveText {
            generation,
            frame: json!({"type":"audio_params", "rate":48000}),
            now_ms: 0,
        })
        .unwrap();
    assert!(!s.snapshot().online && !s.snapshot().ambient_wanted);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CloseLive { .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Capture { enabled: false }))
    );
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Event { event } if event["code"] == "unsupported_audio_rate")
    ));
}
fn opened(s: &mut Session) -> u64 {
    match s.dispatch(Command::Connect).unwrap().last().unwrap() {
        Effect::OpenLive { generation, .. } => *generation,
        _ => panic!(),
    }
}
fn prepare(s: &mut Session, id: &str) -> u64 {
    let effects = s
        .dispatch(Command::SendText {
            id: id.into(),
            text: "prepared text".into(),
            attachments: vec![],
            place: None,
            now_ms: 0,
        })
        .unwrap();
    match effects[0] {
        Effect::Persist { generation, .. } => generation,
        _ => panic!(),
    }
}

#[test]
fn every_packet_split_and_truncation_is_bounded() {
    let packets = vec![vec![1, 2, 3], vec![0x55; 1275], vec![255]];
    let body = frame_packets(&packets).unwrap();
    for split in 0..=body.len() {
        let mut parser = PacketStream::default();
        let mut actual = parser.push(&body[..split]).unwrap();
        assert!(parser.buffered_bytes() <= 1275);
        actual.extend(parser.push(&body[split..]).unwrap());
        parser.finish().unwrap();
        assert_eq!(actual, packets);
    }
    for body in [vec![1], vec![2, 0, 1], vec![0, 0], vec![0xff, 0xff]] {
        let mut parser = PacketStream::default();
        let result = parser.push(&body);
        assert!(result.is_err() || parser.finish().is_err());
    }
    assert!(frame_packets(&[vec![0; 1276]]).is_err());
}
#[test]
fn credentials_are_bound_to_origin_and_anonymous_routes() {
    let c = config();
    let api = Api::new(c.endpoint.clone(), &c.room, &c.token).unwrap();
    assert!(!api.health().unwrap().headers.contains_key("Authorization"));
    assert!(
        !api::pair_request("prepared")
            .unwrap()
            .headers
            .contains_key("Authorization")
    );
    assert_eq!(
        api.state().unwrap().headers["Authorization"],
        "Bearer prepared-token"
    );
    assert!(
        api.state()
            .unwrap()
            .path
            .contains("test%26%E6%88%BF%E9%97%B4")
    );
    for host in [
        "x/y",
        "user@host",
        "host?x",
        "host\\path",
        "host\n",
        "host:443",
    ] {
        assert!(Endpoint::new(host, 443, true).is_err(), "{host}");
    }
    for path in [
        "https://evil.test/",
        "//evil.test/",
        "/\\evil.test/",
        "/api/state#fragment",
    ] {
        assert!(c.endpoint.url(path).is_err());
    }
    assert_eq!(
        Endpoint::new("::1", 8443, true).unwrap().origin(),
        "https://[::1]:8443"
    );
    assert!(Api::new(c.endpoint, &c.room, "token\r\nInjected: true").is_err());
    assert!(unauthorized(401, br#"{"error":"unauthorized"}"#));
    assert!(!unauthorized(401, br#"{"error":"invalid_code"}"#));
    assert!(!format!("{:?}", api.state().unwrap()).contains("prepared-token"));
    assert!(
        !format!(
            "{:?}",
            PairingLink::parse("duoduopocket://pair?host=h&code=secret-code").unwrap()
        )
        .contains("secret-code")
    );
}
#[test]
fn upload_limits_and_digest_use_actual_bytes() {
    let disabled =
        RoomState::decode(br#"{"room":"r","limits":{"upload_max_bytes":null}}"#).unwrap();
    assert!(disabled.admit_upload(1).is_err());
    let bounded = RoomState::decode(br#"{"room":"r","limits":{"upload_max_bytes":3}}"#).unwrap();
    assert!(bounded.admit_upload(3).is_ok());
    assert!(bounded.admit_upload(4).is_err());
    let local = Attachment {
        name: "x.txt".into(),
        mime: "text/plain".into(),
        sha256: None,
        path: None,
    };
    let got = api::uploaded(local.clone(), b"abc", br#"{"path":"/uploads/x"}"#).unwrap();
    assert_eq!(
        got.sha256.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert!(api::uploaded(local.clone(), b"abc", br#"{"path":"x","sha256":"wrong"}"#).is_err());
    assert!(api::uploaded(local, b"abc", b"{}").is_err());
}
#[test]
fn locked_playback_waits_for_tail_and_hardware_then_expires_missing_receipt() {
    let mut a = Ambient::new("r");
    a.frame(&json!({"type":"speech","speech_id":"c1"}), 0)
        .unwrap();
    let g = a.generation;
    a.decoded(g, 16_000, 100);
    a.frame(
        &json!({"type":"playback","speech_id":"old","state":"done","played_ms":0}),
        200,
    )
    .unwrap();
    assert!(a.receiving);
    a.frame(
        &json!({"type":"playback","speech_id":"c1","state":"done","played_ms":2000}),
        300,
    )
    .unwrap();
    a.scheduled(g);
    a.decoded(g, 16_000, 400);
    assert!(!a.receiving && a.playback_pending());
    a.played(g, 2000, false);
    assert!(!a.playback_pending());
    a.frame(&json!({"type":"speech","speech_id":"c2"}), 1000)
        .unwrap();
    a.tick(30_999, false);
    assert!(a.receiving);
    a.tick(31_000, true);
    assert!(a.receiving);
    a.tick(31_000, false);
    assert!(!a.receiving);
}
#[test]
fn final_unmute_survives_press_end_and_retiring_connection() {
    let mut s = session();
    let g = opened(&mut s);
    s.dispatch(Command::LiveText {
        generation: g,
        frame: json!({"type":"ready","api":2,"seq":0,"messages":[]}),
        now_ms: 0,
    })
    .unwrap();
    s.dispatch(Command::LiveText {
        generation: g,
        frame: json!({"type":"meta","conn":"prepared"}),
        now_ms: 0,
    })
    .unwrap();
    s.dispatch(Command::SetAmbient { enabled: true }).unwrap();
    s.dispatch(Command::CapturePacket {
        packet: vec![1],
        aec: false,
    })
    .unwrap();
    s.dispatch(Command::Press {
        source: "key".into(),
        active: true,
    })
    .unwrap();
    s.dispatch(Command::Press {
        source: "key".into(),
        active: false,
    })
    .unwrap();
    let effects = s.dispatch(Command::Disconnect).unwrap();
    assert!(effects.iter().any(|e| matches!(e, Effect::CloseLive { generation, final_frames } if *generation == g && final_frames == &vec![json!({"type":"mute","on":true})])));
    assert!(
        s.dispatch(Command::LiveText {
            generation: g,
            frame: json!({"type":"speech","speech_id":"old"}),
            now_ms: 0
        })
        .unwrap()
        .is_empty()
    );
}
#[test]
fn persistence_is_required_and_stale_or_failed_admission_cannot_send() {
    let mut s = session();
    let g = prepare(&mut s, "00000000-0000-4000-8000-000000000001");
    assert!(
        s.dispatch(Command::Persisted {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation: g + 1,
            ok: true,
            now_ms: 0
        })
        .unwrap()
        .is_empty()
    );
    let effects = s
        .dispatch(Command::Persisted {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation: g,
            ok: false,
            now_ms: 0,
        })
        .unwrap();
    assert!(!effects.iter().any(|e| matches!(e, Effect::Http { .. })));
    let g = prepare(&mut s, "00000000-0000-4000-8000-000000000002");
    s.dispatch(Command::Discard {
        id: "00000000-0000-4000-8000-000000000002".into(),
        now_ms: 0,
    })
    .unwrap();
    assert!(
        s.dispatch(Command::Persisted {
            id: "00000000-0000-4000-8000-000000000002".into(),
            generation: g,
            ok: true,
            now_ms: 0
        })
        .unwrap()
        .is_empty()
    );
}
#[test]
fn live_settlement_outranks_late_http_and_cancellation_retires_retry() {
    let mut s = session();
    let connection = opened(&mut s);
    let operation = prepare(&mut s, "00000000-0000-4000-8000-000000000001");
    s.dispatch(Command::Persisted {
        id: "00000000-0000-4000-8000-000000000001".into(),
        generation: operation,
        ok: true,
        now_ms: 0,
    })
    .unwrap();
    s.dispatch(Command::LiveText {
        generation: connection,
        frame: json!({"type":"message","id":"00000000-0000-4000-8000-000000000001","state":"cleaned","text":"recognized"}),
        now_ms: 1,
    })
    .unwrap();
    assert!(
        s.dispatch(Command::HttpResult {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation: operation,
            status: 401,
            body: br#"{"error":"unauthorized"}"#.to_vec(),
            now_ms: 2
        })
        .unwrap()
        .is_empty()
    );
    assert!(s.snapshot().authorized);
    assert!(
        s.dispatch(Command::Retry {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation: operation,
            now_ms: 500
        })
        .unwrap()
        .is_empty()
    );
}
#[test]
fn retry_bytes_and_budget_are_frozen() {
    let mut s = session();
    let generation = prepare(&mut s, "00000000-0000-4000-8000-000000000001");
    let effects = s
        .dispatch(Command::Persisted {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation,
            ok: true,
            now_ms: 0,
        })
        .unwrap();
    let first = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::Http { request, .. } = e {
                Some(request)
            } else {
                None
            }
        })
        .unwrap();
    let effects = s
        .dispatch(Command::HttpResult {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation,
            status: 503,
            body: vec![],
            now_ms: 100,
        })
        .unwrap();
    let command = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::Schedule { command, .. } = e {
                Some(command)
            } else {
                None
            }
        })
        .unwrap();
    let effects = s.dispatch(command).unwrap();
    let second = effects
        .into_iter()
        .find_map(|e| {
            if let Effect::Http { request, .. } = e {
                Some(request)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(first, second);
    s.dispatch(Command::HttpResult {
        id: "00000000-0000-4000-8000-000000000001".into(),
        generation,
        status: 503,
        body: vec![],
        now_ms: 24_999,
    })
    .unwrap();
    assert!(
        !s.dispatch(Command::Retry {
            id: "00000000-0000-4000-8000-000000000001".into(),
            generation,
            now_ms: 25_000
        })
        .unwrap()
        .iter()
        .any(|e| matches!(e, Effect::Http { .. }))
    );
    let mut retry = VoiceRetry::new(100, 25_000);
    assert_eq!(retry.failed(100), Some(500));
    assert_eq!(retry.failed(25_099), Some(1));
    assert_eq!(retry.failed(25_100), None);
}
#[test]
fn unauthorized_and_old_api_stop_reconnect_and_preserve_cursor_monotonicity() {
    let mut s = session();
    let generation = opened(&mut s);
    let effects = s
        .dispatch(Command::SocketClosed {
            generation,
            code: Some(4401),
        })
        .unwrap();
    assert!(!effects.iter().any(|e| matches!(e, Effect::Schedule { .. })));
    assert!(!s.snapshot().authorized);
    assert!(s.dispatch(Command::Connect).is_err());
    let mut s = session();
    let generation = opened(&mut s);
    s.dispatch(Command::LiveText {
        generation,
        frame: json!({"type":"ready","api":1,"seq":5,"messages":[]}),
        now_ms: 0,
    })
    .unwrap();
    assert!(!s.snapshot().online);
    let mut cursor = RowCursor::default();
    assert!(cursor.admit(10));
    assert!(!cursor.admit(9));
    cursor.reached(3);
    assert_eq!(cursor.seq, Some(10));
}
#[test]
fn session_scope_and_unknown_additive_frames() {
    let mut s = session();
    assert!(s.dispatch(Command::WatchSessions { key: None }).is_err());
    assert!(
        s.dispatch(Command::NotifySession {
            id: "n".into(),
            key: "ambient:prepared".into(),
            text: "prepared".into()
        })
        .is_err()
    );
    let generation = opened(&mut s);
    assert!(
        s.dispatch(Command::LiveText {
            generation,
            frame: json!({"type":"future_additive_event","field":1}),
            now_ms: 0
        })
        .unwrap()
        .iter()
        .any(|e| matches!(e, Effect::Event { .. }))
    );
}

#[test]
fn discarding_current_send_admits_the_next_persisted_message() {
    let mut s = session();
    let first = prepare(&mut s, "00000000-0000-4000-8000-000000000003");
    let second = prepare(&mut s, "00000000-0000-4000-8000-000000000004");
    s.dispatch(Command::Persisted {
        id: "00000000-0000-4000-8000-000000000003".into(),
        generation: first,
        ok: true,
        now_ms: 0,
    })
    .unwrap();
    assert!(
        s.dispatch(Command::Persisted {
            id: "00000000-0000-4000-8000-000000000004".into(),
            generation: second,
            ok: true,
            now_ms: 1
        })
        .unwrap()
        .is_empty()
    );
    let effects = s
        .dispatch(Command::Discard {
            id: "00000000-0000-4000-8000-000000000003".into(),
            now_ms: 2,
        })
        .unwrap();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CancelHttp { id, .. } if id == "00000000-0000-4000-8000-000000000003"))
    );
    assert!(effects.iter().any(
        |e| matches!(e, Effect::Http { id, .. } if id == "00000000-0000-4000-8000-000000000004")
    ));
}
