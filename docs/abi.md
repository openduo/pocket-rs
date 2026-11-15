# C ABI v1 and host effects

The ABI is defined by `include/pocket_core.h`; no Rust layout, allocator, executor or asynchronous
callback crosses it. `pocket_abi_version()` returns 1. Rust crate versions remain separately semver'd.

Every result owns one length-delimited UTF-8 JSON buffer. Copy/parse it, then pass the unchanged
result to `pocket_result_free` exactly once. Results are not NUL-terminated. Inputs are borrowed
only until return. A nonzero `status` contains `error.code` and diagnostic `error.detail`; applications
localize codes. Protocol calls may return a token or pairing code intentionally: treat those
buffers as credentials, do not log them. Debug formatting of Rust credentials/requests is redacted.

Input pointers must refer to readable memory for their supplied length; the library rejects null
nonempty buffers and oversized requests. Invalid/destroyed integer handles return `invalid_handle`.
Session operations are synchronized and return owned effects; invoke platform work after return.
The host must not free buffers twice or pass fabricated allocation pointers to the free function.
Panics are caught at result-producing boundaries. This is a C ownership contract, not a memory
sandbox for callers that violate pointer validity.

## Configuration and control

`pocket_session_create` accepts:

```json
{"endpoint":{"host":"example.test","port":443,"tls":true},"room":"prepared-room","token":"device-token","scope":"room","surface":{"display":"later","spoken":true},"cursor":null,"media":false}
```

The returned handle is private to this process. `pocket_session_dispatch` takes a `type`-tagged
command and returns `effects` plus a credential-free `snapshot`. The Rust `Command`/`Effect` enums
are the schema of record; unknown command tags and invalid field types fail. Unknown additive
server events remain available to the host without crashing the protocol owner.

| Command | Important fields |
| --- | --- |
| `connect`, `disconnect` | Explicit connection demand; disconnect returns final cleanup effects |
| `live_text`, `live_binary`, `socket_closed` | Current connection `generation`; frame/bytes or close code |
| `set_ambient`, `capture_packet`, `takeover` | Enabled flag, one Opus packet and actual AEC capability |
| `press`, `hush` | Stable input source and active flag; shared by touch, keys and remotes |
| `decoded`, `playback_scheduled`, `played` | Audio declaration generation; decoded samples, actual completion ms |
| `tick` | Monotonic elapsed `now_ms`, actual `hardware_pending` |
| `send_text`, `send_voice`, `restore` | Stable id, immutable body/metadata, monotonic time |
| `persisted`, `http_result`, `retry`, `discard` | Delivery operation generation, body/status and monotonic time |
| `watch_sessions`, `unwatch_sessions`, `notify_session` | Watch target; notify requires notify scope and advertised support |

`pocket_session_binary` borrows raw packet bytes to avoid JSON encoding the input audio path.
Output decode effects currently include owned byte arrays; applications should process each batch
promptly. A future ABI can add bulk binary output without changing ABI v1's functions.

## Effect execution rules

- **Open live:** HTTP upgrade over the endpoint configured for this session, with its headers;
  no redirects to another origin. Every completion includes the connection generation.
- **Close live:** attempt `final_frames` in order on that old connection before closing it. Bound
  the attempt by host policy; a dead network cannot guarantee final unmute delivery. Do not use it
  for new callbacks or close a replacement connection.
- **Persist:** atomically store the versioned `DurableMessage` under its room/id, flush as required
  by the platform, then acknowledge with `persisted`. No HTTP effect is issued beforehand.
  Stored records exclude bearer credentials. Preserve ids/bodies until authoritative settlement;
  server idempotency retention still defines how long a manual retry is safe.
- **HTTP/schedule/cancel:** one send/retry slot, bounded pending admission. Cancel the matching
  transport request and timer. A timer's retry time must be refreshed to actual monotonic elapsed
  time; the 25-second budget includes storage, queueing, network and backoff.
- **Capture:** acquire/release the platform microphone lease. Supply the first encoded packet
  before hello; peer devices must not upload until the room grants the seat. Report truthful AEC.
- **Decode/playback:** reset the decoder per declaration, preserve already queued earlier PCM,
  reject stale completion generations. Register a block only when scheduled; report `played` only
  after actual hardware completion (or `skipped:true` for deliberately muted audio). Network
  synthesis completion is not hardware drain.
- **Events:** commit rows/cursors and message settlement atomically in the application's store,
  then publish immutable presentation state. The SDK's cursor is in-memory; durable cursor/row
  transaction policy belongs to the host during this migration.

Close a session explicitly and execute returned effects before releasing host resources. The library
does not own sockets/audio handles and cannot close those OS resources on the application's behalf.
No command is produced merely because an application lost foreground visibility. Android/iOS
adapters supply permissions/background policy while preserving already-started playback.

## Stateless protocol entry point

`pocket_protocol_call` accepts `op` values `parse_pairing`, `pair_request`, `decode_pair`,
`frame_voice`, `parse_voice` and `request`. The last takes a session config and `RequestCommand`.
It constructs authenticated, origin-relative data requests without executing them. Pairing and
health requests omit authorization. Input JSON is bounded at 24 MiB, while individual live Opus
packets are at most 1,275 bytes (1,276 with media-v1 kind byte).
