// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use pocket_core::{
    api::{Api, RequestCommand, Surface, validate_message_id},
    audio::frame_packets,
    pairing::{DeviceScope, Endpoint},
    session::{Command, DurableMessage, Effect, Session, SessionConfig},
};
use std::collections::BTreeMap;

const ID: &str = "aabbccdd-1234-4567-89ab-0123456789ef";

fn api() -> Api {
    Api::new(
        Endpoint::new("example.test", 443, true).unwrap(),
        "prepared",
        "prepared-token",
    )
    .unwrap()
}
fn new_session() -> Session {
    Session::new(SessionConfig {
        endpoint: Endpoint::new("example.test", 443, true).unwrap(),
        room: "prepared".into(),
        token: "prepared-token".into(),
        scope: DeviceScope::Room,
        surface: Surface::default(),
        cursor: None,
        media: false,
    })
    .unwrap()
}

#[test]
fn malformed_ids_fail_all_message_admission_paths_without_pending_work() {
    let body = frame_packets(&[vec![0xb8, 0xff, 0xfe]]).unwrap();
    for id in [
        "",
        "e1-0123456789abcdef",
        "stable-message-id",
        "aabbccdd1234456789ab0123456789ef",
        "aabbccdd_1234-4567-89ab-0123456789ef",
        "gabbccdd-1234-4567-89ab-0123456789ef",
        "aabbccdd-1234-4567-89ab-0123456789e",
        "aabbccdd-1234-4567-89ab-0123456789eff",
        "aabbccdd-1234-4567-89ab-0123456789ef\r\n",
        "ａａｂｂｃｃｄｄ-1234-4567-89ab-0123456789ef",
    ] {
        let api = api();
        for result in [
            api.text(id, "prepared", &[], Surface::default(), None),
            api.voice(id, &body, "phone", Surface::default(), None, None),
            api.voice_head(id, &body, "phone", Surface::default(), None, None),
            RequestCommand::Text {
                id: id.into(),
                text: "prepared".into(),
                attachments: vec![],
                surface: Surface::default(),
                place: None,
            }
            .build(&api),
        ] {
            assert_eq!(
                result.unwrap_err().code,
                "invalid_idempotency_key",
                "{id:?}"
            );
        }
        let mut session = new_session();
        for command in [
            Command::SendText {
                id: id.into(),
                text: "prepared".into(),
                attachments: vec![],
                place: None,
                now_ms: 0,
            },
            Command::SendVoice {
                id: id.into(),
                body: body.clone(),
                source: "phone".into(),
                place: None,
                now_ms: 0,
            },
            Command::Restore {
                message: DurableMessage {
                    version: 1,
                    room: "prepared".into(),
                    id: id.into(),
                    voice: true,
                    headers: BTreeMap::from([("Idempotency-Key".into(), id.into())]),
                    body: body.clone(),
                },
                now_ms: 0,
            },
        ] {
            assert_eq!(
                session
                    .dispatch(command)
                    .expect_err("Invalid ID must not emit effects")
                    .code,
                "invalid_idempotency_key",
                "{id:?}"
            );
            assert!(session.snapshot().pending.is_empty());
        }
    }
}

#[test]
fn valid_uuid_case_and_server_compatible_versions_preserve_identity() {
    for id in [
        ID.to_string(),
        ID.to_ascii_uppercase(),
        "00000000-0000-0000-0000-000000000000".into(),
    ] {
        validate_message_id(&id).unwrap();
        let request = api()
            .text(&id, "prepared", &[], Surface::default(), None)
            .unwrap();
        assert_eq!(request.headers["Idempotency-Key"], id.to_ascii_lowercase());
        let mut session = new_session();
        let effects = session
            .dispatch(Command::SendText {
                id,
                text: "prepared".into(),
                attachments: vec![],
                place: None,
                now_ms: 0,
            })
            .unwrap();
        let Effect::Persist {
            generation,
            message,
        } = &effects[0]
        else {
            panic!("Expected admission record");
        };
        assert_eq!(message.id, request.headers["Idempotency-Key"]);
        let mut restored = new_session();
        let result = restored
            .dispatch(Command::Restore {
                message: message.clone(),
                now_ms: 0,
            })
            .unwrap();
        assert!(
            matches!(&result[0], Effect::Persist { message: restored, .. } if restored.id == message.id && restored.body == message.body)
        );
        let effects = session
            .dispatch(Command::Persisted {
                id: message.id.clone(),
                generation: *generation,
                ok: true,
                now_ms: 0,
            })
            .unwrap();
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Http { request: sent, .. } if sent == &request)
        ));
    }
}
