// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
import Foundation
import PocketCore

enum PocketFailure: Error { case abi, result(String), unexpected }

func take(_ result: PocketResult) throws -> (UInt64, [String: Any]) {
    defer { pocket_result_free(result) }
    let data = Data(bytes: result.payload.data!, count: result.payload.len)
    guard result.status == 0 else { throw PocketFailure.result(String(decoding: data, as: UTF8.self)) }
    guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw PocketFailure.unexpected }
    return (result.handle, object)
}

func bytes<T>(_ object: [String: Any], _ call: (UnsafePointer<UInt8>?, Int) throws -> T) throws -> T {
    let data = try JSONSerialization.data(withJSONObject: object)
    return try data.withUnsafeBytes { raw in try call(raw.baseAddress?.assumingMemoryBound(to: UInt8.self), raw.count) }
}

@main struct Consumer {
    static func main() throws {
        guard pocket_abi_version() == POCKET_ABI_VERSION else { throw PocketFailure.abi }
        let (_, pairing) = try bytes(["op": "parse_pairing", "text": "duoduopocket://pair?host=example.test&code=prepared"]) { try take(pocket_protocol_call($0, $1)) }
        guard (pairing["endpoint"] as? [String: Any])?["host"] as? String == "example.test" else { throw PocketFailure.unexpected }
        let config: [String: Any] = ["endpoint": ["host": "example.test", "port": 443, "tls": true], "room": "prepared", "token": "fixture-token"]
        let (handle, _) = try bytes(config) { try take(pocket_session_create($0, $1)) }
        let (_, connected) = try bytes(["type": "connect"]) { try take(pocket_session_dispatch(handle, $0, $1)) }
        guard (connected["effects"] as? [[String: Any]])?.contains(where: { $0["type"] as? String == "open_live" }) == true else { throw PocketFailure.unexpected }
        let (_, ready) = try bytes(["type": "live_text", "generation": 1, "frame": ["type": "ready", "api": 2, "seq": 0, "messages": []]]) { try take(pocket_session_dispatch(handle, $0, $1)) }
        guard (ready["snapshot"] as? [String: Any])?["online"] as? Bool == true else { throw PocketFailure.unexpected }
        let (_, closed) = try take(pocket_session_close(handle))
        guard (closed["effects"] as? [[String: Any]])?.contains(where: { $0["type"] as? String == "close_live" }) == true else { throw PocketFailure.unexpected }
        print("PASS Swift consumer: module import, pairing, create/connect/ready/close and Rust buffer ownership")
    }
}
