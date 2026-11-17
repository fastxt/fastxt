//
//  FastxtTests.swift
//  FastxtTests
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import XCTest
@testable import Fastxt

final class FastxtTests: XCTestCase {

    // MARK: Ideas and prompts

    func testTheKindIsReadFromTheTags() {
        XCTAssertEqual(NoteKind.of(tags: "kind:prompt,auth"), .prompt)
        XCTAssertEqual(NoteKind.of(tags: "auth, Kind:Idea"), .idea)
        XCTAssertNil(NoteKind.of(tags: "auth,kind:other"))
        XCTAssertNil(NoteKind.of(tags: ""))
    }

    func testSettingAKindReplacesTheOldOne() {
        XCTAssertEqual(NoteKind.set(.prompt, in: "kind:idea,auth"), "kind:prompt,auth")
        XCTAssertEqual(NoteKind.set(.idea, in: ""), "kind:idea")
        XCTAssertEqual(NoteKind.set(nil, in: "auth，kind:prompt, login   flow"), "auth,login flow")
    }

    func testTheKindIsNotAmongTheTagsShown() {
        XCTAssertEqual(NoteKind.visibleTags("kind:prompt, auth ,  login   flow"), ["auth", "login flow"])
        XCTAssertEqual(NoteKind.visibleTags(""), [])
    }

    func testEachFilterAsksForItsTag() {
        XCTAssertEqual(NoteFilter.prompts.tag, "kind:prompt")
        XCTAssertEqual(NoteFilter.ideas.tag, "kind:idea")
        XCTAssertNil(NoteFilter.all.tag)
        XCTAssertNil(NoteFilter.sent.tag)
    }

    // MARK: Time stamps

    func testStampsAreReadAsUTC() {
        var components = DateComponents()
        (components.year, components.month, components.day) = (2026, 11, 16)
        (components.hour, components.minute, components.second) = (15, 3, 34)
        components.timeZone = TimeZone(identifier: "UTC")
        let expected = Calendar(identifier: .gregorian).date(from: components)
        XCTAssertEqual(Stamp.date("2026-11-16 15:03:34"), expected)
        XCTAssertNil(Stamp.date("yesterday"))
        XCTAssertEqual(Stamp.local("yesterday"), "yesterday", "an unreadable stamp is shown as it is")
        XCTAssertNotEqual(Stamp.local("2026-11-16 15:03:34"), "2026-11-16 15:03:34")
    }

    // MARK: Pairing codes

    func testAPairingCodeHasFiveParts() {
        let fingerprint = String(repeating: "a", count: 32)
        let code = "FASTXT1:192.168.1.5:3456:\(fingerprint):ABCDEFGHIJ"
        XCTAssertTrue(PairedComputer.isPairingCode(code))
        XCTAssertTrue(PairedComputer.isPairingCode("  \(code)\n"))
        XCTAssertFalse(PairedComputer.isPairingCode("FASTXT1:192.168.1.5:3456:abc:ABCDEFGHIJ"))
        XCTAssertFalse(PairedComputer.isPairingCode("FASTXT1:192.168.1.5:99999:\(fingerprint):ABCDEFGHIJ"))
        XCTAssertFalse(PairedComputer.isPairingCode("FASTXT1::3456:\(fingerprint):ABCDEFGHIJ"))
        XCTAssertFalse(PairedComputer.isPairingCode("192.168.1.5:3456"))
        XCTAssertEqual(PairedComputer.address(code), "192.168.1.5:3456")
    }

    // MARK: Replies

    func testASessionListIsRead() throws {
        let reply = """
        {"sessions": [{"host": "mac", "handle": "term_1", "agent": "claude", "name": "fix login",
          "cwd": "/Users/me/app", "branch": "main", "state": "asking", "last_prompt": null,
          "updated_at": 1}]}
        """
        let sessions = try AgentLink.decode(reply, field: "sessions", as: [AgentSession].self).get()
        XCTAssertEqual(sessions.count, 1)
        XCTAssertEqual(sessions[0].id, "term_1")
        XCTAssertEqual(sessions[0].folderName, "app")
        XCTAssertTrue(sessions[0].isAsking)
    }

    func testARefusalKeepsItsReason() {
        let reply = #"{"error": "that session is asking you something", "reason": "asking"}"#
        guard case .failure(let error) = AgentLink.decode(reply, field: "sent", as: SentRecord.self) else {
            return XCTFail("a refusal was read as a success")
        }
        XCTAssertEqual(error.message, "that session is asking you something")
        XCTAssertEqual(error.reason, "asking")
        XCTAssertFalse(error.advice.isEmpty)
    }

    func testAnErrorWithoutAReasonIsAComputerNotReached() {
        guard case .failure(let error) = AgentLink.decode(#"{"error": "timed out"}"#, field: "sessions",
                                                          as: [AgentSession].self) else {
            return XCTFail("an error was read as a success")
        }
        XCTAssertNil(error.reason)
        XCTAssertTrue(error.advice.contains("fastxt serve --agents"))
    }

    func testARecordSaysWhetherTheAgentTookItOrWasSentIt() throws {
        let reply = """
        {"sent": [{"uuid4": "s2", "note_uuid": "n1", "sent_at": "2026-11-16 23:11:41", "host": "mcp",
          "agent": "claude-code", "session": "", "cwd": "/Users/me/app", "started": true},
         {"uuid4": "s1", "note_uuid": "n1", "sent_at": "2026-11-16 23:10:00", "host": "orca",
          "agent": "claude", "session": "Fix login", "cwd": "/Users/me/app", "started": true}]}
        """
        let records = try AgentLink.decode(reply, field: "sent", as: [SentRecord].self).get()
        XCTAssertTrue(records[0].taken)
        XCTAssertEqual(records[0].title, "Taken by claude-code")
        XCTAssertFalse(records[1].taken)
        XCTAssertEqual(records[1].title, "claude · Fix login")
    }

    func testAReplyWithoutTheFieldIsAnError() {
        XCTAssertThrowsError(try AgentLink.decode(#"{"count": 0}"#, field: "sessions", as: [AgentSession].self).get())
        XCTAssertThrowsError(try AgentLink.decode("not json", field: "sessions", as: [AgentSession].self).get())
    }

    // MARK: Through the Rust core

    func testAMalformedCodeIsRefusedBeforeAnyNetwork() {
        guard case .failure(let error) = AgentLink.sessions(code: "FASTXT1:nowhere") else {
            return XCTFail("a malformed code was accepted")
        }
        XCTAssertNil(error.reason)
        XCTAssertFalse(error.message.isEmpty)
    }

    func testPromptsAreFoundByTheirKind() throws {
        let marker = "zq\(Int.random(in: 100_000...999_999))"
        let idea = try insert("\(marker) an idea", tags: NoteKind.idea.tag)
        let prompt = try insert("\(marker) a prompt", tags: NoteKind.set(.prompt, in: "tests"))
        defer {
            AppState.delete(rowid: idea.id)
            AppState.delete(rowid: prompt.id)
        }

        XCTAssertEqual(try search(marker, kind: .prompt).map(\.uuid4), [prompt.uuid4])
        XCTAssertEqual(try search(marker, kind: .idea).map(\.uuid4), [idea.uuid4])
        XCTAssertEqual(try search(marker, kind: nil).count, 2)

        // Making the prompt an idea keeps its other tags and its place.
        let changed = try XCTUnwrap(AppState.setKind(.idea, of: prompt))
        XCTAssertEqual(changed.id, prompt.id)
        XCTAssertEqual(changed.tags, "kind:idea,tests")
        XCTAssertEqual(try search(marker, kind: .prompt).count, 0)
        XCTAssertEqual(AppState.getNote(uuid4: prompt.uuid4)?.tags, "kind:idea,tests")

        // Nothing was sent anywhere.
        XCTAssertTrue(AgentLink.sent(noteUUID: prompt.uuid4).isEmpty)
    }

    private func insert(_ txt: String, tags: String) throws -> Note {
        try AgentLink.call(["action": "insert", "txt": txt, "tags": tags], field: "note", as: Note.self).get()
    }

    private func search(_ query: String, kind: NoteKind?) throws -> [Note] {
        var command: [String: Any] = ["action": "search", "query": query, "limit": 10, "offset": 0]
        if let kind = kind {
            command["tags"] = [kind.tag]
        }
        return try AgentLink.call(command, field: "notes", as: [Note].self).get()
    }
}
