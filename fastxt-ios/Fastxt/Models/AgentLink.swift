/*
 Fastxt
 Copyright (C) 2026  Yi Wang

 This program is free software: you can redistribute it and/or modify
 it under the terms of the GNU Affero General Public License as published by
 the Free Software Foundation, either version 3 of the License, or
 (at your option) any later version.

 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU Affero General Public License for more details.

 You should have received a copy of the GNU Affero General Public License
 along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import Foundation
import Security

/// A coding-agent session open on the paired computer, as its
/// `fastxt serve --agents` lists it.
struct AgentSession: Codable, Identifiable, Hashable {
    var host: String
    var handle: String
    /// `claude`, `codex`, `kimi`, …
    var agent: String
    var name: String
    /// The project folder it works in, on that computer.
    var cwd: String
    var branch: String?
    /// `ready`, `working`, `asking`, or empty when the computer doesn't say.
    var state: String
    var last_prompt: String?
    var updated_at: Int64?

    var id: String { handle }

    /// Nothing is typed into a session that is asking its user a question.
    var isAsking: Bool { state == "asking" }

    /// The project folder's name, to group sessions by.
    var folderName: String {
        let name = (cwd as NSString).lastPathComponent
        return name.isEmpty ? cwd : name
    }
}

/// A record that a note was typed into a session: where and when.
struct SentRecord: Codable, Identifiable, Hashable {
    var uuid4: String
    var note_uuid: String
    /// UTC, `yyyy-MM-dd HH:mm:ss`.
    var sent_at: String
    var host: String
    var agent: String
    var session: String
    var cwd: String
    var started: Bool

    var id: String { uuid4 }
}

/// What a sync moved.
struct SyncReport: Codable {
    var notes_pulled: Int
    var notes_pushed: Int
    var sent_pulled: Int?
    var sent_pushed: Int?
}

/// Why the computer typed nothing, or couldn't be reached.
struct LinkError: Error, Equatable {
    var message: String
    /// `unavailable`, `gone`, `asking`, `not-an-agent`, `invalid` or
    /// `failed` when the computer answered; nil when it couldn't be reached
    /// or the pairing code was refused.
    var reason: String?

    /// What to do about it, for people.
    var advice: String {
        switch reason {
        case "unavailable":
            return "On your computer, run `fastxt serve --agents` and pair with its new code."
        case "gone":
            return "That session closed. Refresh the list."
        case "asking":
            return "That session is asking you something. Answer it on the computer, then send again."
        case "not-an-agent":
            return "The agent had just quit, so the text went to its terminal instead. Look at that terminal now."
        case nil:
            return "Is `fastxt serve --agents` still running on your computer, on this network? Its code lasts until it stops; if it restarted, pair again."
        default:
            return ""
        }
    }
}

/// The commands that reach the paired computer and the records of what was
/// sent. Each runs synchronously: call them off the main thread.
enum AgentLink {
    /// Run one JSON command and decode its `field`.
    static func call<T: Decodable>(_ command: [String: Any], field: String, as type: T.Type) -> Result<T, LinkError> {
        guard let data = try? JSONSerialization.data(withJSONObject: command),
              let input = String(data: data, encoding: .utf8) else {
            return .failure(LinkError(message: "The command could not be written.", reason: nil))
        }
        return decode(AppState.ft.run(json_input: input), field: field, as: type)
    }

    /// Read a reply: `{field: …}`, or `{"error": …, "reason": …}`.
    static func decode<T: Decodable>(_ reply: String, field: String, as type: T.Type) -> Result<T, LinkError> {
        guard let data = reply.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return .failure(LinkError(message: "The reply could not be read.", reason: nil))
        }
        if let error = object["error"] as? String {
            return .failure(LinkError(message: error, reason: object["reason"] as? String))
        }
        guard let value = object[field],
              let valueData = try? JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed]),
              let decoded = try? JSONDecoder().decode(type, from: valueData) else {
            return .failure(LinkError(message: "The reply had no \(field).", reason: nil))
        }
        return .success(decoded)
    }

    /// The sessions open on the paired computer, the one that changed last
    /// first.
    static func sessions(code: String) -> Result<[AgentSession], LinkError> {
        call(["action": "agent-sessions", "code": code], field: "sessions", as: [AgentSession].self)
    }

    /// Type a note into one of them, as its user would; the record of it is
    /// kept on both sides.
    static func send(code: String, rowid: Int64, handle: String) -> Result<SentRecord, LinkError> {
        call(["action": "send-to-session", "code": code, "rowid": rowid, "handle": handle],
             field: "sent", as: SentRecord.self)
    }

    /// What was sent, newest first: one note's records, or every note's.
    static func sent(noteUUID: String? = nil, limit: Int = 200) -> [SentRecord] {
        var command: [String: Any] = ["action": "sent", "limit": limit]
        if let noteUUID = noteUUID {
            command["uuid4"] = noteUUID
        }
        return (try? call(command, field: "sent", as: [SentRecord].self).get()) ?? []
    }

    /// Sync the notes, and the records of what was sent, with the computer.
    static func sync(code: String) -> Result<SyncReport, LinkError> {
        call(["action": "sync", "code": code], field: "report", as: SyncReport.self)
    }
}

/// The computer this phone sends to: its pairing code, kept in the Keychain,
/// since its token lets this phone sync with that computer and type into its
/// sessions while its `fastxt serve --agents` runs.
enum PairedComputer {
    private static let service = "app.fastxt.ios.pairing"
    private static let account = "computer"

    private static var query: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }

    static func load() -> String? {
        var item = query
        item[kSecReturnData as String] = true
        item[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(item as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    static func save(_ code: String) -> Bool {
        forget()
        var item = query
        item[kSecValueData as String] = Data(code.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func forget() {
        SecItemDelete(query as CFDictionary)
    }

    /// Whether text looks like a pairing code: FASTXT1, host, port,
    /// certificate fingerprint and token.
    static func isPairingCode(_ text: String) -> Bool {
        let parts = text.trimmingCharacters(in: .whitespacesAndNewlines)
            .split(separator: ":", omittingEmptySubsequences: false)
        return parts.count == 5 && parts[0] == "FASTXT1" && !parts[1].isEmpty
            && UInt16(parts[2]) != nil && parts[3].count == 32 && !parts[4].isEmpty
    }

    /// Where a code points, for people: host and port.
    static func address(_ code: String) -> String {
        let parts = code.split(separator: ":", omittingEmptySubsequences: false)
        return parts.count >= 3 ? "\(parts[1]):\(parts[2])" : ""
    }
}
