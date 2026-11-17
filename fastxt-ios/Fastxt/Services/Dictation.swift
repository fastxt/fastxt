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

import AVFoundation
import Foundation
import Speech

/// Speech to text on this iPhone only.
///
/// Two recognizers can do it, both on the phone. The one built into iOS runs
/// with `requiresOnDeviceRecognition`, which keeps the audio from being sent
/// over the network; it hears English or Chinese when this iPhone can
/// recognize that language on the device. SenseVoice, the model the app
/// carries, hears the two mixed, and stands in for the iOS one where that
/// would need the network. When neither can, dictation doesn't start: it
/// never falls back to a server. The audio goes to the recognizer as it
/// comes and is never stored.
final class Dictation: ObservableObject {
    enum Language: String, CaseIterable, Identifiable {
        case english = "en-US"
        case chinese = "zh-CN"
        /// Chinese and English, also in one sentence: SenseVoice only.
        case mixed

        var id: String { rawValue }

        var label: String {
            switch self {
            case .english: return "English"
            case .chinese: return "中文"
            case .mixed: return "中文 + EN"
            }
        }

        /// The iOS recognizer's locale; none for mixed, which it can't hear.
        var locale: Locale? {
            self == .mixed ? nil : Locale(identifier: rawValue)
        }

        var senseVoice: SenseVoice.Language {
            switch self {
            case .english: return .en
            case .chinese: return .zh
            case .mixed: return .auto
            }
        }
    }

    /// What turns the speech into text.
    enum Recognizer: Equatable {
        /// The one built into iOS, on the device.
        case ios
        /// SenseVoice, inside the app.
        case senseVoice
        /// Neither can, on this iPhone.
        case none

        /// The iOS recognizer when it can hear `language` on the device;
        /// SenseVoice otherwise, and always for mixed.
        static func choose(for language: Language, iosOnDevice: Bool, senseVoice: Bool) -> Recognizer {
            if language != .mixed && iosOnDevice {
                return .ios
            }
            return senseVoice ? .senseVoice : .none
        }

        static func choose(for language: Language) -> Recognizer {
            let ios = language.locale.flatMap { SFSpeechRecognizer(locale: $0) }
            return choose(
                for: language,
                iosOnDevice: ios?.supportsOnDeviceRecognition ?? false,
                senseVoice: SenseVoice.shared != nil
            )
        }
    }

    /// The languages to offer: mixed only when the app carries SenseVoice.
    static var languages: [Language] {
        SenseVoice.shared == nil ? [.english, .chinese] : Language.allCases
    }

    @Published var language: Language {
        didSet {
            UserDefaults.standard.set(language.rawValue, forKey: Self.languageKey)
            recognizer = Recognizer.choose(for: language)
            prepare()
        }
    }
    /// What will turn speech into text, for the language chosen.
    @Published private(set) var recognizer: Recognizer
    /// What the current recording has recognized so far.
    @Published private(set) var heard = ""
    @Published private(set) var isRecording = false
    /// Stopped listening, and turning the last words into text.
    @Published private(set) var isFinishing = false
    /// Why dictation didn't start or stopped, for people; empty when fine.
    @Published private(set) var problem = ""

    private static let languageKey = "dictation.language"
    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var listener: SenseVoice.Listener?
    /// Which recording is current, so a late answer about an earlier one is
    /// ignored.
    private var generation = 0

    init() {
        let saved = UserDefaults.standard.string(forKey: Self.languageKey).flatMap(Language.init(rawValue:))
        let preferred: Language = Locale.preferredLanguages.first?.hasPrefix("zh") == true ? .chinese : .english
        let language = saved.flatMap { Self.languages.contains($0) ? $0 : nil } ?? preferred
        self.language = language
        recognizer = Recognizer.choose(for: language)
        prepare()
    }

    /// `after` following `before`: with a space between them, unless both
    /// sides are Chinese (or Japanese) or full-width punctuation, which take
    /// none.
    static func join(_ before: String, _ after: String) -> String {
        guard let last = before.last else { return after }
        guard let first = after.first else { return before }
        if last.isWhitespace || first.isWhitespace || first.takesNoSpaceBefore
            || last.isFullWidthPunctuation || first.isFullWidthPunctuation
            || (last.isWrittenWithoutSpaces && first.isWrittenWithoutSpaces)
        {
            return before + after
        }
        return before + " " + after
    }

    func toggle() {
        isRecording ? stop() : start()
    }

    /// Hand over what was heard and forget it.
    func takeHeard() -> String {
        let text = heard
        heard = ""
        return text
    }

    func start() {
        problem = ""
        heard = ""
        recognizer = Recognizer.choose(for: language)
        let name = language.label
        switch recognizer {
        case .ios:
            requestPermissions(speech: true) { [weak self] granted in
                guard let self = self else { return }
                if granted {
                    self.beginIOS()
                } else {
                    self.problem = "Fastxt needs the microphone and speech recognition: allow them in Settings → Fastxt."
                }
            }
        case .senseVoice:
            requestPermissions(speech: false) { [weak self] granted in
                guard let self = self else { return }
                if granted {
                    self.beginSenseVoice()
                } else {
                    self.problem = "Fastxt needs the microphone: allow it in Settings → Fastxt."
                }
            }
        case .none:
            if language.locale.flatMap({ SFSpeechRecognizer(locale: $0) }) == nil {
                problem = "This iPhone has no speech recognizer for \(name)."
            } else {
                problem = "This iPhone can't recognize \(name) without sending the audio away, so Fastxt won't. Add \(name) under Settings → General → Keyboard → Dictation Languages, then try again."
            }
        }
    }

    /// Load SenseVoice's model ahead of a recording that will need it.
    private func prepare() {
        if recognizer == .senseVoice {
            SenseVoice.shared?.prepare(language.senseVoice)
        }
    }

    private func requestPermissions(speech: Bool, _ done: @escaping (Bool) -> Void) {
        let microphone = {
            let reply: (Bool) -> Void = { granted in DispatchQueue.main.async { done(granted) } }
            if #available(iOS 17.0, *) {
                AVAudioApplication.requestRecordPermission(completionHandler: reply)
            } else {
                AVAudioSession.sharedInstance().requestRecordPermission(reply)
            }
        }
        // SenseVoice needs only the microphone, not Apple's speech recognition.
        guard speech else { return microphone() }
        SFSpeechRecognizer.requestAuthorization { status in
            guard status == .authorized else {
                DispatchQueue.main.async { done(false) }
                return
            }
            microphone()
        }
    }

    private func beginIOS() {
        guard let ios = language.locale.flatMap({ SFSpeechRecognizer(locale: $0) }),
              ios.supportsOnDeviceRecognition
        else { return }
        guard ios.isAvailable else {
            problem = "Speech recognition isn't available right now. Try again in a moment."
            return
        }

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.requiresOnDeviceRecognition = true // never over the network
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        request.taskHint = .dictation
        self.request = request

        do {
            try startAudio { buffer in request.append(buffer) }
        } catch {
            problem = "The microphone couldn't start: \(error.localizedDescription)"
            finish()
            return
        }

        generation += 1
        let current = generation
        isRecording = true
        task = ios.recognitionTask(with: request) { [weak self] result, error in
            DispatchQueue.main.async {
                guard let self = self, self.generation == current else { return }
                if let result = result {
                    self.heard = result.bestTranscription.formattedString
                }
                if error != nil || result?.isFinal == true {
                    self.finish()
                }
            }
        }
    }

    private func beginSenseVoice() {
        guard let senseVoice = SenseVoice.shared else { return }
        generation += 1
        let current = generation
        let listener = SenseVoice.Listener(
            senseVoice, language: language.senseVoice,
            heard: { [weak self] text in
                DispatchQueue.main.async {
                    guard let self = self, self.generation == current else { return }
                    self.heard = Self.join(self.heard, text)
                }
            },
            failed: { [weak self] in
                DispatchQueue.main.async {
                    guard let self = self, self.generation == current else { return }
                    self.problem = "SenseVoice couldn't load its model on this iPhone. What was heard so far is kept."
                    self.stop()
                }
            }
        )

        do {
            var converter: SenseVoice.Converter?
            try startAudio { buffer in
                if converter == nil {
                    converter = SenseVoice.Converter(from: buffer.format)
                }
                if let samples = converter?.samples(buffer) {
                    listener.feed(samples)
                }
            }
        } catch {
            listener.cancel()
            problem = "The microphone couldn't start: \(error.localizedDescription)"
            finish()
            return
        }
        self.listener = listener
        isRecording = true
    }

    private func startAudio(_ tap: @escaping (AVAudioPCMBuffer) -> Void) throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .measurement, options: .duckOthers)
        try session.setActive(true, options: .notifyOthersOnDeactivation)
        let input = engine.inputNode
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0)) { buffer, _ in
            tap(buffer)
        }
        engine.prepare()
        try engine.start()
    }

    /// Stop listening. The recognizer finishes the last words it heard, then
    /// recording ends; what was heard stays.
    func stop() {
        guard isRecording, !isFinishing else { return }
        isFinishing = true
        stopAudio()
        let current = generation
        if let listener = listener {
            listener.finish { [weak self] in
                DispatchQueue.main.async {
                    guard let self = self, self.generation == current else { return }
                    self.finish()
                }
            }
            return
        }
        request?.endAudio()
        // Should the recognizer never say it is done, end anyway.
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self = self, self.generation == current, self.isRecording else { return }
            self.task?.finish()
            self.finish()
        }
    }

    /// Stop and drop what this recording heard, as when the screen closes.
    func cancel() {
        generation += 1
        task?.cancel()
        listener?.cancel()
        heard = ""
        finish()
    }

    private func stopAudio() {
        if engine.isRunning {
            engine.stop()
        }
        engine.inputNode.removeTap(onBus: 0)
    }

    private func finish() {
        stopAudio()
        request?.endAudio()
        request = nil
        task = nil
        listener = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        isFinishing = false
        isRecording = false
    }
}

private extension Character {
    /// Han characters and kana: Chinese and Japanese put no spaces between
    /// words.
    var isWrittenWithoutSpaces: Bool {
        unicodeScalars.contains { scalar in
            switch scalar.value {
            case 0x2E80...0x2FDF, 0x3040...0x30FF, 0x3400...0x4DBF, 0x4E00...0x9FFF, 0xF900...0xFAFF,
                 0x20000...0x2FA1F:
                return true
            default:
                return false
            }
        }
    }

    /// 。，、！？「」 and the other full-width marks, which take no spaces.
    var isFullWidthPunctuation: Bool {
        unicodeScalars.contains { (0x3000...0x303F).contains($0.value) || (0xFF00...0xFFEF).contains($0.value) }
    }

    /// Punctuation that sits right after the word before it.
    var takesNoSpaceBefore: Bool {
        ".,;:!?)]}%".contains(self)
    }
}
