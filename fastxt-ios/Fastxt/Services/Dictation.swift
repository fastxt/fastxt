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
/// Recognition runs with `requiresOnDeviceRecognition`, which keeps the audio
/// from being sent over the network. When this iPhone can't recognize a
/// language on the device, dictation doesn't start: it never falls back to a
/// server. The audio is passed to the recognizer as it comes and never
/// stored.
final class Dictation: ObservableObject {
    enum Language: String, CaseIterable, Identifiable {
        case english = "en-US"
        case chinese = "zh-CN"

        var id: String { rawValue }

        var label: String {
            switch self {
            case .english: return "English"
            case .chinese: return "中文"
            }
        }

        /// What goes between two recordings' text.
        var joiner: String {
            switch self {
            case .english: return " "
            case .chinese: return ""
            }
        }
    }

    @Published var language: Language {
        didSet { UserDefaults.standard.set(language.rawValue, forKey: Self.languageKey) }
    }
    /// What the current recording has recognized so far.
    @Published private(set) var heard = ""
    @Published private(set) var isRecording = false
    /// Why dictation didn't start or stopped, for people; empty when fine.
    @Published private(set) var problem = ""

    private static let languageKey = "dictation.language"
    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    init() {
        let saved = UserDefaults.standard.string(forKey: Self.languageKey).flatMap(Language.init(rawValue:))
        let preferred: Language = Locale.preferredLanguages.first?.hasPrefix("zh") == true ? .chinese : .english
        language = saved ?? preferred
    }

    /// Whether this iPhone can recognize the language without the network.
    static func isOnDevice(_ language: Language) -> Bool {
        SFSpeechRecognizer(locale: Locale(identifier: language.rawValue))?.supportsOnDeviceRecognition ?? false
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
        requestPermissions { [weak self] granted in
            guard let self = self else { return }
            if granted {
                self.begin()
            } else {
                self.problem = "Fastxt needs the microphone and speech recognition: allow them in Settings → Fastxt."
            }
        }
    }

    private func requestPermissions(_ done: @escaping (Bool) -> Void) {
        SFSpeechRecognizer.requestAuthorization { status in
            guard status == .authorized else {
                DispatchQueue.main.async { done(false) }
                return
            }
            let reply: (Bool) -> Void = { granted in DispatchQueue.main.async { done(granted) } }
            if #available(iOS 17.0, *) {
                AVAudioApplication.requestRecordPermission(completionHandler: reply)
            } else {
                AVAudioSession.sharedInstance().requestRecordPermission(reply)
            }
        }
    }

    private func begin() {
        let name = language.label
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: language.rawValue)) else {
            problem = "This iPhone has no speech recognizer for \(name)."
            return
        }
        guard recognizer.supportsOnDeviceRecognition else {
            problem = "This iPhone can't recognize \(name) without sending the audio away, so Fastxt won't. Add \(name) under Settings → General → Keyboard → Dictation Languages, then try again."
            return
        }
        guard recognizer.isAvailable else {
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
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            let input = engine.inputNode
            input.removeTap(onBus: 0)
            input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0)) { buffer, _ in
                request.append(buffer)
            }
            engine.prepare()
            try engine.start()
        } catch {
            problem = "The microphone couldn't start: \(error.localizedDescription)"
            finish()
            return
        }

        isRecording = true
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if let result = result {
                    self.heard = result.bestTranscription.formattedString
                }
                if error != nil || result?.isFinal == true {
                    self.finish()
                }
            }
        }
    }

    /// Stop listening. The recognizer finishes the last words it heard, then
    /// recording ends; what was heard stays.
    func stop() {
        guard isRecording else { return }
        stopAudio()
        request?.endAudio()
        // Should the recognizer never say it is done, end anyway.
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self = self, self.isRecording else { return }
            self.task?.finish()
            self.finish()
        }
    }

    /// Stop and drop what this recording heard, as when the screen closes.
    func cancel() {
        task?.cancel()
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
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        isRecording = false
    }
}
