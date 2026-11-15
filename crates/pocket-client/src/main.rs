// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use clap::{Parser, Subcommand};
use pocket_client::transport::{Network, Socket};
use pocket_core::{
    Error, Result,
    api::{Api, Display, Surface},
    audio::{AudioAction, PacketStream, frame_packets},
    pairing::{PairedDevice, PairingLink},
    session::{Command, Effect, Session, SessionConfig},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    name = "pocket",
    version,
    about = "DuoDuo Pocket Rust protocol reference client"
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}
#[derive(Subcommand)]
enum CliCommand {
    /// Native loopback acceptance; no external room, microphone or speaker is accessed.
    SelfTest,
    /// Redeem a new pairing link from a private file and write a private configuration.
    Pair {
        #[arg(long)]
        link_file: PathBuf,
        #[arg(long)]
        config: PathBuf,
    },
    State {
        #[arg(long)]
        config: PathBuf,
    },
    History {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value_t = 0)]
        since: u64,
    },
    Text {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        text_file: PathBuf,
    },
    Voice {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        packets: PathBuf,
    },
    /// Explicitly retry the same durable body and id; no automatic startup sends occur.
    Retry {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        id: String,
    },
    /// Save length-prefixed Opus speech to a new file; this does not play audio.
    Speech {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        text_file: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Observe live events; optional pre-encoded input/output demonstrates the audio wire path.
    Live {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=600))]
        seconds: u64,
        #[arg(long)]
        ambient: bool,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}
fn file_error(_: std::io::Error) -> Error {
    Error::new("storage_error", "File operation failed")
}
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    File::open(path)
        .map_err(file_error)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(file_error)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::new("limit_exceeded", "Input file exceeds limit"));
    }
    Ok(bytes)
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(file_error)?;
    let temporary = parent.join(format!(".pocket-{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(file_error)?;
    let result = (|| {
        file.write_all(bytes).map_err(file_error)?;
        file.sync_all().map_err(file_error)?;
        fs::rename(&temporary, path).map_err(file_error)?;
        File::open(parent)
            .map_err(file_error)?
            .sync_all()
            .map_err(file_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn load(path: &Path) -> Result<SessionConfig> {
    Ok(serde_json::from_slice(&read_bounded(path)?)?)
}
fn print(value: Value) {
    println!("{value}");
}
fn checked(response: pocket_client::transport::Response, status: u16) -> Result<Vec<u8>> {
    if response.status != status {
        return Err(Error::new(
            "http_error",
            format!("HTTP {}", response.status),
        ));
    }
    Ok(response.body)
}
fn deliver(config_path: &Path, command: Command) -> Result<()> {
    let config = load(config_path)?;
    let endpoint = config.endpoint.clone();
    let mut session = Session::new(config)?;
    let network = Network::new()?;
    let start = Instant::now();
    let mut queue: VecDeque<_> = session.dispatch(command)?.into();
    while let Some(effect) = queue.pop_front() {
        let now = || start.elapsed().as_millis() as u64;
        match effect {
            Effect::Persist {
                generation,
                message,
            } => {
                let directory = config_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("outbox");
                let stored = directory.join(format!("{}.json", message.id));
                private_write(&stored, &serde_json::to_vec(&message)?)?;
                queue.extend(session.dispatch(Command::Persisted {
                    id: message.id,
                    generation,
                    ok: true,
                    now_ms: now(),
                })?);
            }
            Effect::Http {
                generation,
                id,
                request,
                timeout_ms,
            } => {
                let (status, body) = match network.request(&endpoint, &request, timeout_ms) {
                    Ok(r) => (r.status, r.body),
                    Err(_) => (0, vec![]),
                };
                queue.extend(session.dispatch(Command::HttpResult {
                    id,
                    generation,
                    status,
                    body,
                    now_ms: now(),
                })?);
            }
            Effect::Schedule {
                delay_ms,
                mut command,
            } => {
                std::thread::sleep(Duration::from_millis(delay_ms));
                if let Command::Retry { now_ms, .. } = &mut command {
                    *now_ms = now();
                }
                queue.extend(session.dispatch(command)?);
            }
            Effect::Event { event } => {
                print(event.clone());
                if event["state"] == "failed" || event["type"] == "unauthorized" {
                    return Err(Error::new(
                        "send_failed",
                        "Message remains in the private outbox",
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
fn live(
    config_path: &Path,
    seconds: u64,
    ambient: bool,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<()> {
    if ambient && input.is_none() {
        return Err(Error::invalid(
            "Reference Ambient mode requires --input with pre-encoded Opus packets",
        ));
    }
    let mut config = load(config_path)?;
    config.surface.spoken = output.is_some();
    let endpoint = config.endpoint.clone();
    let mut session = Session::new(config)?;
    if ambient {
        session.dispatch(Command::SetAmbient { enabled: true })?;
    }
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
        .ok_or_else(|| Error::protocol("Missing live request"))?;
    let mut socket = Socket::connect(&endpoint, &request)?;
    socket.poll_timeout(20)?;
    let mut output = if let Some(path) = output {
        Some(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(file_error)?,
        )
    } else {
        None
    };
    let packets = if let Some(path) = input {
        let bytes = read_bounded(&path)?;
        let mut parser = PacketStream::default();
        let packets = parser.push(&bytes)?;
        parser.finish()?;
        packets
    } else {
        vec![]
    };
    let started = Instant::now();
    let mut sent = 0usize;
    while started.elapsed().as_secs() < seconds {
        let now_ms = started.elapsed().as_millis() as u64;
        if session.snapshot().online && ambient && !session.snapshot().ambient_wanted {
            session.dispatch(Command::SetAmbient { enabled: true })?;
        }
        if ambient
            && session.snapshot().online
            && sent < packets.len()
            && now_ms >= sent as u64 * 20
        {
            for effect in session.dispatch(Command::CapturePacket {
                packet: packets[sent].clone(),
                aec: false,
            })? {
                apply(&mut socket, effect, &mut output)?;
            }
            sent += 1;
        }
        let command = match socket.read() {
            Ok(tungstenite::Message::Text(text)) => Some(Command::LiveText {
                generation,
                frame: serde_json::from_str(&text)?,
                now_ms,
            }),
            Ok(tungstenite::Message::Binary(bytes)) => Some(Command::LiveBinary {
                generation,
                bytes: bytes.to_vec(),
            }),
            Ok(tungstenite::Message::Close(_)) => break,
            Err(e) if e.code == "would_block" => None,
            Err(e) => return Err(e),
            _ => None,
        };
        if let Some(command) = command {
            for effect in session.dispatch(command)? {
                apply(&mut socket, effect, &mut output)?;
            }
        }
        session.dispatch(Command::Tick {
            now_ms,
            hardware_pending: false,
        })?;
    }
    for effect in session.dispatch(Command::Disconnect)? {
        apply(&mut socket, effect, &mut output)?;
    }
    Ok(())
}
fn apply(socket: &mut Socket, effect: Effect, output: &mut Option<File>) -> Result<()> {
    match effect {
        Effect::SendFrame { frame, .. } => socket.text(&frame)?,
        Effect::SendBinary { bytes, .. } => socket.binary(&bytes)?,
        Effect::Event { event } => print(event),
        Effect::Audio {
            action: AudioAction::Decode { packet, .. },
        } => {
            if let Some(output) = output {
                output
                    .write_all(&frame_packets(&[packet])?)
                    .map_err(file_error)?;
            }
        }
        Effect::CloseLive { final_frames, .. } => {
            for frame in final_frames {
                socket.text(&frame)?;
            }
            socket.close()?;
        }
        _ => {}
    }
    Ok(())
}
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        CliCommand::SelfTest => print(pocket_client::self_test::run()?),
        CliCommand::Pair { link_file, config } => {
            let text = String::from_utf8(read_bounded(&link_file)?)
                .map_err(|_| Error::invalid("Pairing file must be UTF-8"))?;
            let link = PairingLink::parse(&text)?;
            let network = Network::new()?;
            let result = network.request(
                &link.endpoint,
                &pocket_core::api::pair_request(&link.code)?,
                30_000,
            )?;
            let device = PairedDevice::decode(&checked(result, 200)?)?;
            let saved = SessionConfig {
                endpoint: link.endpoint,
                room: device.room,
                token: device.token,
                scope: device.scope,
                surface: Surface {
                    display: Display::Later,
                    spoken: false,
                },
                cursor: None,
                media: false,
            };
            private_write(&config, &serde_json::to_vec_pretty(&saved)?)?;
            print(json!({"status":"paired", "scope":device.scope}));
        }
        CliCommand::State { config } => {
            let c = load(&config)?;
            let api = Api::new(c.endpoint.clone(), &c.room, &c.token)?;
            let body = checked(
                Network::new()?.request(&c.endpoint, &api.state()?, 20_000)?,
                200,
            )?;
            print(serde_json::from_slice(&body)?);
        }
        CliCommand::History { config, since } => {
            let c = load(&config)?;
            let api = Api::new(c.endpoint.clone(), &c.room, &c.token)?;
            print(serde_json::from_slice(&checked(
                Network::new()?.request(&c.endpoint, &api.rows(since)?, 20_000)?,
                200,
            )?)?);
        }
        CliCommand::Text {
            config,
            id,
            text_file,
        } => {
            let text = String::from_utf8(read_bounded(&text_file)?)
                .map_err(|_| Error::invalid("Text file must be UTF-8"))?;
            deliver(
                &config,
                Command::SendText {
                    id,
                    text,
                    attachments: vec![],
                    place: None,
                    now_ms: 0,
                },
            )?;
        }
        CliCommand::Voice {
            config,
            id,
            packets,
        } => deliver(
            &config,
            Command::SendVoice {
                id,
                body: read_bounded(&packets)?,
                source: "phone".into(),
                place: None,
                now_ms: 0,
            },
        )?,
        CliCommand::Retry { config, id } => {
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            {
                return Err(Error::invalid("Invalid message identity"));
            }
            let path = config
                .parent()
                .unwrap_or(Path::new("."))
                .join("outbox")
                .join(format!("{id}.json"));
            deliver(
                &config,
                Command::Restore {
                    message: serde_json::from_slice(&read_bounded(&path)?)?,
                    now_ms: 0,
                },
            )?;
        }
        CliCommand::Speech {
            config,
            text_file,
            output,
        } => {
            let c = load(&config)?;
            let api = Api::new(c.endpoint.clone(), &c.room, &c.token)?;
            let text = String::from_utf8(read_bounded(&text_file)?)
                .map_err(|_| Error::invalid("Text file must be UTF-8"))?;
            let body = checked(
                Network::new()?.request(&c.endpoint, &api.speech(&text)?, 60_000)?,
                200,
            )?;
            let mut parser = PacketStream::default();
            parser.push(&body)?;
            parser.finish()?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)
                .map_err(file_error)?;
            file.write_all(&body).map_err(file_error)?;
            print(json!({"status":"saved", "format":"length_prefixed_opus", "bytes":body.len()}));
        }
        CliCommand::Live {
            config,
            seconds,
            ambient,
            input,
            output,
        } => live(&config, seconds, ambient, input, output)?,
    }
    Ok(())
}
fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("{}", json!({"error":error}));
        std::process::exit(1);
    }
}
