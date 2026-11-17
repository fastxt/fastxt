//
//  FastxtTests.swift
//  FastxtTests
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import AVFoundation
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

/// Dictation's own rules, and SenseVoice hearing the sample recordings that
/// come with it when `script/fetch-speech-models.sh` has fetched them.
final class DictationTests: XCTestCase {

    // MARK: Joining what was heard

    func testPiecesJoinWithASpaceUnlessBothSidesAreChinese() {
        XCTAssertEqual(Dictation.join("", "你好"), "你好")
        XCTAssertEqual(Dictation.join("你好", ""), "你好")
        XCTAssertEqual(Dictation.join("先登录", "再上传"), "先登录再上传")
        XCTAssertEqual(Dictation.join("Add a login page", "to the app."), "Add a login page to the app.")
        XCTAssertEqual(Dictation.join("加一个", "login page"), "加一个 login page")
        XCTAssertEqual(Dictation.join("好的。", "Next"), "好的。Next")
        XCTAssertEqual(Dictation.join("Done", "，然后上传"), "Done，然后上传")
        XCTAssertEqual(Dictation.join("Done", ", then upload"), "Done, then upload")
        XCTAssertEqual(Dictation.join("First line\n", "second"), "First line\nsecond")
    }

    // MARK: Choosing the recognizer

    func testIOSHearsALanguageItKnowsOnTheDeviceAndSenseVoiceTheRest() {
        typealias Recognizer = Dictation.Recognizer
        XCTAssertEqual(Recognizer.choose(for: .english, iosOnDevice: true, senseVoice: true), .ios)
        XCTAssertEqual(Recognizer.choose(for: .chinese, iosOnDevice: true, senseVoice: false), .ios)
        XCTAssertEqual(Recognizer.choose(for: .chinese, iosOnDevice: false, senseVoice: true), .senseVoice)
        XCTAssertEqual(Recognizer.choose(for: .mixed, iosOnDevice: true, senseVoice: true), .senseVoice)
        // Never a recognizer that would send the audio away.
        XCTAssertEqual(Recognizer.choose(for: .english, iosOnDevice: false, senseVoice: false), .none)
    }

    func testMixedIsOfferedOnlyWhenTheAppCarriesSenseVoice() {
        XCTAssertEqual(Dictation.languages.contains(.mixed), SenseVoice.shared != nil)
        XCTAssertEqual(Dictation.Language.mixed.senseVoice, .auto)
        XCTAssertNil(Dictation.Language.mixed.locale)
    }

    // MARK: SenseVoice

    func testTheMicrophonesAudioBecomes16kHzOnOneChannel() throws {
        let format = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 2))
        let converter = try XCTUnwrap(SenseVoice.Converter(from: format))
        var counts: [Int] = []
        var loudest: Float = 0
        // Three seconds of a 440 Hz tone, a tenth at a time.
        for block in 0..<30 {
            let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4_800))
            buffer.frameLength = 4_800
            let channels = try XCTUnwrap(buffer.floatChannelData)
            for i in 0..<4_800 {
                let t = Double(block * 4_800 + i) / 48_000
                channels[0][i] = Float(0.5 * sin(2 * .pi * 440 * t))
                channels[1][i] = channels[0][i]
            }
            let samples = converter.samples(buffer)
            counts.append(samples.count)
            loudest = max(loudest, samples.map(abs).max() ?? 0)
        }
        // The converter keeps a few milliseconds back and hands out its work
        // in uneven pieces, but what it keeps back doesn't grow: nothing is
        // lost between buffers. (Losing even 15 samples a buffer would leave
        // 450 missing by the end.)
        for seconds in 1...3 {
            let missing = 16_000 * seconds - counts.prefix(10 * seconds).reduce(0, +)
            XCTAssertTrue((0..<400).contains(missing), "\(missing) missing after \(seconds) s: \(counts)")
        }
        XCTAssertGreaterThan(loudest, 0.3, "the tone comes through")
    }

    func testSenseVoiceHearsEnglishAndChinese() throws {
        let senseVoice = try carried()
        let english = try recording("en")
        let chinese = try recording("zh")
        let heard = senseVoice.queue.sync {
            (senseVoice.recognize(english, language: .auto), senseVoice.recognize(chinese, language: .auto))
        }
        Self.expectTheSamples(said: [try XCTUnwrap(heard.0), try XCTUnwrap(heard.1)])

        // Told the language instead, the loaded model switches without a reload.
        let told = senseVoice.queue.sync {
            (senseVoice.recognize(english, language: .en), senseVoice.recognize(chinese, language: .zh))
        }
        Self.expectTheSamples(said: [try XCTUnwrap(told.0), try XCTUnwrap(told.1)])
    }

    func testEachPauseBecomesText() throws {
        let senseVoice = try carried()
        let pause = [Float](repeating: 0, count: SenseVoice.sampleRate)
        let recording = pause + (try recording("en")) + pause + (try recording("zh")) + pause
        var pieces: [String] = []
        let done = expectation(description: "the recording became text")
        let listener = SenseVoice.Listener(
            senseVoice, language: .auto,
            heard: { pieces.append($0) },
            failed: { XCTFail("SenseVoice's models didn't load") }
        )
        // A tenth of a second at a time, as from the microphone.
        for start in stride(from: 0, to: recording.count, by: 1_600) {
            listener.feed(Array(recording[start..<min(start + 1_600, recording.count)]))
        }
        listener.finish { done.fulfill() }
        wait(for: [done], timeout: 120)
        // One piece per stretch of speech, each in its own language.
        Self.expectTheSamples(said: pieces)
        XCTAssertEqual(pieces.reduce("", Dictation.join), pieces.joined(separator: " "))
    }

    /// What the sample recordings say, English then Chinese: their gist,
    /// punctuated and with numbers as numbers. The model may get a word
    /// wrong ("gold" can come out "code", 开放 as 开饭), so the test doesn't
    /// hold it to every word.
    private static func expectTheSamples(said pieces: [String], line: UInt = #line) {
        XCTAssertEqual(pieces.count, 2, "\(pieces)", line: line)
        guard pieces.count == 2 else { return }
        XCTAssertTrue(pieces[0].hasPrefix("The tribal chieftain called for the boy and presented him with 50 pieces of "), pieces[0], line: line)
        XCTAssertTrue(pieces[0].hasSuffix("."), pieces[0], line: line)
        XCTAssertTrue(pieces[1].hasSuffix("时间早上9点至下午5点。"), pieces[1], line: line)
    }

    /// SenseVoice, or a skip when this build doesn't carry it.
    private func carried() throws -> SenseVoice {
        guard let senseVoice = SenseVoice.shared else {
            throw XCTSkip("This build carries no speech models: run script/fetch-speech-models.sh first.")
        }
        return senseVoice
    }

    /// One of the recordings that come with SenseVoice, beside this file.
    private func recording(_ name: String) throws -> [Float] {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("SpeechSamples/\(name).wav")
        guard let data = try? Data(contentsOf: url) else {
            throw XCTSkip("No SpeechSamples/\(name).wav: run script/fetch-speech-models.sh first.")
        }
        return try samples(ofWave: data)
    }

    private struct NotA16kHzRecording: Error {}

    /// The samples of a WAV file holding 16 kHz, 16-bit audio on one channel.
    private func samples(ofWave data: Data) throws -> [Float] {
        let bytes = [UInt8](data)
        func number(_ at: Int, _ size: Int) -> Int {
            (0..<size).reduce(0) { $0 | Int(bytes[at + $1]) << (8 * $1) }
        }
        func tag(_ at: Int) -> String { String(decoding: bytes[at..<(at + 4)], as: UTF8.self) }
        guard bytes.count > 12, tag(0) == "RIFF", tag(8) == "WAVE" else { throw NotA16kHzRecording() }
        var at = 12
        var fits = false
        while at + 8 <= bytes.count {
            let size = number(at + 4, 4)
            let body = at + 8
            if tag(at) == "fmt " {
                fits = number(body + 2, 2) == 1 && number(body + 4, 4) == 16_000 && number(body + 14, 2) == 16
            }
            if tag(at) == "data" {
                guard fits else { throw NotA16kHzRecording() }
                let end = min(body + size, bytes.count)
                return stride(from: body, to: end - 1, by: 2).map {
                    Float(Int16(truncatingIfNeeded: number($0, 2))) / 32_768
                }
            }
            at = body + size + size % 2
        }
        throw NotA16kHzRecording()
    }
}
