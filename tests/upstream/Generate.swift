// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
import Foundation

func actions(_ values: [AmbientEdge.Action]) -> [[String: Any]] {
    values.map { action in
        switch action {
        case .send(let f): return ["type": "send", "frame": f.mapValues { $0.base }]
        case .play(let bytes, let gen): return ["type": "decode", "generation": gen, "packet": Array(bytes)]
        case .resetDecoder: return ["type": "reset_decoder"]
        case .clearPlayback: return ["type": "clear_playback"]
        case .stopRead: return ["type": "stop_read"]
        case .fatal: return ["type": "fatal"]
        case .warn: return ["type": "notice", "code": "audio_without_speech"]
        case .camera: return ["type": "camera"]
        case .keyFrame: return ["type": "key_frame"]
        }
    }
}

func runScenario(_ name: String, _ steps: [[String: Any]]) -> [String: Any] {
    var edge = AmbientEdge(room: "fixture-room")
    var results: [[String: Any]] = []
    for step in steps {
        var output: [AmbientEdge.Action] = []
        switch step["op"] as! String {
        case "open": edge.socketOpened()
        case "close": output = edge.socketClosed()
        case "packet": edge.aec = step["aec"] as? Bool ?? false; output = edge.packetEncoded()
        case "frame": output = edge.frame(step["frame"] as! [String: Any])
        case "takeover": output = edge.takeover()
        case "press_begin": output = edge.pressBegan()
        case "press_end": output = edge.pressEnded()
        case "read_begin": output = edge.readBegan()
        case "read_end": edge.readEnded()
        case "hush": output = edge.hush()
        case "binary": output = edge.binary(Data((step["bytes"] as! [Int]).map(UInt8.init)))
        case "scheduled": edge.scheduled(gen: step["generation"] as! Int)
        case "played": output = edge.played(gen: step["generation"] as! Int, ms: Double(step["ms"] as! Int))
        case "skipped": output = edge.skipped(gen: step["generation"] as! Int, ms: Double(step["ms"] as! Int))
        default: fatalError("Unknown vector operation")
        }
        results.append(["actions": actions(output), "state": [
            "conn": edge.conn as Any? ?? NSNull(), "role": edge.role?.rawValue as Any? ?? NSNull(),
            "hello_sent": edge.helloSent, "sends_audio": edge.sendsAudio,
            "generation": edge.gen, "current": edge.current as Any? ?? NSNull(),
            "reading": edge.reading, "pending_blocks": edge.pending,
        ]])
    }
    return ["name": name, "steps": steps, "results": results]
}

@main struct Generate {
    static func main() throws {
        let links = [
            "duoduopocket://pair?host=box.example.test&port=8443&tls=0&code=AB12-CD34",
            "在手机上打开：\nduoduopocket://pair?host=h.example.test&code=x%2By 十分钟内有效",
            "「duoduopocket://pair?host=h&code=c」",
            "DuoDuoPocket://PAIR?host=h&code=c",
            "duoduopocket://pair?host=h&code=x+y",
            "duoduopocket://pair?host=h&host=ignored&code=first&code=second",
            "", "https://example.test/pair?host=h&code=c", "duoduopocket://share",
            "duoduopocket://pair?host=h", "duoduopocket://pair?code=c",
            "duoduopocket://pair?host=h&code=c&port=0",
            "duoduopocket://pair?host=h&code=c&port=70000",
            "duoduopocket://pair?host=h&code=c&port=x",
            "duoduopocket://pair?host=h&code=c&tls=maybe",
        ]
        let pairing: [[String: Any]] = links.map { text in
            let result: Any
            if let p = PairingLink.parse(text) { result = ["endpoint": ["host": p.host, "port": p.port, "tls": p.tls], "code": p.code] }
            else { result = NSNull() }
            return ["input": text, "expected": result]
        }
        let packetSets: [[[UInt8]]] = [[], [[1]], [[1, 2, 3], [255, 0]], [Array(repeating: 0x55, count: 1275)]]
        let voice: [[String: Any]] = try packetSets.map { packets in
            let note = VoiceNote(id: UUID(uuidString: "00000000-0000-4000-8000-000000000001")!, source: .phone, packets: packets.map { Data($0) })
            return ["packets": packets, "body": Array(try note.body()), "duration_ms": note.durationMs]
        }
        let surfaces: [[String: Any]] = [Surface.Display.phoneAwake, .phone, .passport, .later].flatMap { display in
            [false, true].map { spoken in
                let surface = Surface(display: display, spoken: spoken)
                return ["input": ["display": display.rawValue, "spoken": spoken], "headers": surface.headers, "frame": surface.frame]
            }
        }
        let seated: [[String: Any]] = [["op": "open"], ["op": "frame", "frame": ["type": "meta", "conn": "c1"]], ["op": "packet", "aec": true]]
        var cadence = seated + [["op": "frame", "frame": ["type": "speech", "speech_id": "c-answer"]]]
        cadence += Array(repeating: ["op": "scheduled", "generation": 1], count: 20)
        cadence += Array(repeating: ["op": "played", "generation": 1, "ms": 60], count: 20)
        let scenarios = [
            runScenario("hello waits for a packet", seated),
            runScenario("packet before connection", [["op": "open"], ["op": "packet"], ["op": "frame", "frame": ["type": "meta", "conn": "c2"]]]),
            runScenario("peer and takeover", seated + [["op": "frame", "frame": ["type": "meta", "role": "peer"]], ["op": "takeover"], ["op": "frame", "frame": ["type": "meta", "role": "master"]]]),
            runScenario("overlapping presses", seated + [["op": "press_begin"], ["op": "press_begin"], ["op": "press_end"], ["op": "press_end"], ["op": "press_end"]]),
            runScenario("reconnect", seated + [["op": "close"], ["op": "open"], ["op": "frame", "frame": ["type": "meta", "conn": "c3"]]]),
            runScenario("played cadence", cadence),
            runScenario("stop filler and stale answer", seated + [["op": "frame", "frame": ["type": "speech", "speech_id": "s1"]], ["op": "frame", "frame": ["type": "stop_audio"]], ["op": "frame", "frame": ["type": "speech", "speech_id": "c1"]], ["op": "frame", "frame": ["type": "stop_audio", "speech_id": "old"]], ["op": "frame", "frame": ["type": "stop_audio", "speech_id": "c1"]]]),
            runScenario("press drops later audio", seated + [["op": "frame", "frame": ["type": "speech", "speech_id": "c1"]], ["op": "press_begin"], ["op": "frame", "frame": ["type": "speech", "speech_id": "c2"]], ["op": "binary", "bytes": [1]], ["op": "press_end"], ["op": "binary", "bytes": [1]]]),
            runScenario("read interrupted by live speech", seated + [["op": "frame", "frame": ["type": "speech", "speech_id": "c1"]], ["op": "read_begin"], ["op": "frame", "frame": ["type": "stop_audio"]], ["op": "frame", "frame": ["type": "speech", "speech_id": "c2"]]]),
            runScenario("stale playback completion", seated + [["op": "frame", "frame": ["type": "speech", "speech_id": "c1"]], ["op": "scheduled", "generation": 1], ["op": "frame", "frame": ["type": "speech", "speech_id": "c2"]], ["op": "played", "generation": 1, "ms": 100], ["op": "scheduled", "generation": 2], ["op": "played", "generation": 2, "ms": 20]]),
            runScenario("silent playback and hush", seated + [["op": "frame", "frame": ["type": "speech", "speech_id": "c1"]], ["op": "skipped", "generation": 1, "ms": 60], ["op": "hush"], ["op": "binary", "bytes": [1]]]),
        ]
        var backoff = RedialBackoff()
        let delays = (0..<10).map { _ in Int(backoff.failed() * 1000) }
        let object: [String: Any] = ["schema": 1, "pairing": pairing, "voice": voice, "surfaces": surfaces, "redial_ms": delays, "ambient": scenarios]
        let data = try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([10]))
    }
}
