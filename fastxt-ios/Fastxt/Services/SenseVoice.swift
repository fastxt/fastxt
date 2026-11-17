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
import SherpaOnnxC
import UIKit

/// SenseVoice Small, the speech model Fastxt carries, turning speech into
/// text on this iPhone through sherpa-onnx.
///
/// It hears Chinese and English, also mixed in one sentence. A voice
/// activity detector (Silero VAD) cuts a recording where the speaker pauses,
/// and each stretch of speech becomes text once it ends, so text comes at the
/// pauses rather than word by word. Audio stays in memory only until its
/// stretch is recognized; it is never stored or sent.
///
/// The models are in the app's `SpeechModels` folder when
/// `script/fetch-speech-models.sh` ran before the build. Without them
/// `shared` is nil and dictation uses only the recognizer built into iOS.
final class SenseVoice {
    /// What SenseVoice is told it will hear.
    enum Language: String {
        case auto
        case en
        case zh
    }

    /// The model files inside the app.
    struct Files {
        let model: String
        let tokens: String
        let vad: String

        /// The files in `bundle`'s `SpeechModels` folder, when all are there.
        static func inBundle(_ bundle: Bundle = .main) -> Files? {
            guard let folder = bundle.url(forResource: "SpeechModels", withExtension: nil) else {
                return nil
            }
            let files = Files(
                model: folder.appendingPathComponent("sense-voice/model.int8.onnx").path,
                tokens: folder.appendingPathComponent("sense-voice/tokens.txt").path,
                vad: folder.appendingPathComponent("silero_vad.onnx").path
            )
            let present = [files.model, files.tokens, files.vad].allSatisfy {
                FileManager.default.fileExists(atPath: $0)
            }
            return present ? files : nil
        }
    }

    /// What SenseVoice takes: 16 kHz, one channel, samples in [-1, 1].
    static let sampleRate = 16_000

    /// SenseVoice, when the app carries its models.
    static let shared: SenseVoice? = Files.inBundle().map(SenseVoice.init)

    let files: Files
    /// Everything that touches the models happens here, one thing at a time.
    let queue = DispatchQueue(label: "app.fastxt.sense-voice", qos: .userInitiated)
    /// Loaded on first use and kept, since loading takes a moment; on `queue`.
    private var recognizer: Recognizer?

    init(files: Files) {
        self.files = files
        // The model takes a few hundred megabytes: give them back when iOS
        // asks, and load it again when it's next needed.
        NotificationCenter.default.addObserver(
            forName: UIApplication.didReceiveMemoryWarningNotification, object: nil, queue: nil
        ) { [weak self] _ in
            guard let self = self else { return }
            self.queue.async { self.recognizer = nil }
        }
    }

    /// Load the model now, so the first stretch of speech needn't wait for it.
    func prepare(_ language: Language) {
        queue.async { _ = self.recognizer(for: language) }
    }

    /// The text in `samples` (16 kHz, one channel), with punctuation; nil
    /// when the model couldn't be loaded. Call on `queue`.
    func recognize(_ samples: [Float], language: Language) -> String? {
        dispatchPrecondition(condition: .onQueue(queue))
        return recognizer(for: language)?.text(of: samples)
    }

    private func recognizer(for language: Language) -> Recognizer? {
        if let recognizer = recognizer {
            recognizer.set(language)
            return recognizer
        }
        recognizer = Recognizer(files, language)
        return recognizer
    }
}

// MARK: - sherpa-onnx

/// Copies of Swift strings for C, freed together.
private final class CStrings {
    private var copies: [UnsafeMutablePointer<CChar>] = []

    func callAsFunction(_ string: String) -> UnsafePointer<CChar>? {
        guard let copy = strdup(string) else { return nil }
        copies.append(copy)
        return UnsafePointer(copy)
    }

    deinit {
        copies.forEach { free($0) }
    }
}

extension SenseVoice {
    /// The model loaded in sherpa-onnx's offline recognizer.
    fileprivate final class Recognizer {
        private let pointer: OpaquePointer
        private let files: Files
        private var language: Language

        init?(_ files: Files, _ language: Language) {
            self.files = files
            self.language = language
            let strings = CStrings()
            var config = Self.config(files, language, strings)
            guard let pointer = withExtendedLifetime(strings, { SherpaOnnxCreateOfflineRecognizer(&config) }) else {
                return nil
            }
            self.pointer = pointer
        }

        deinit {
            SherpaOnnxDestroyOfflineRecognizer(pointer)
        }

        /// The language is read at each decode, so changing it needs no
        /// reload.
        func set(_ language: Language) {
            guard language != self.language else { return }
            let strings = CStrings()
            var config = Self.config(files, language, strings)
            withExtendedLifetime(strings) { SherpaOnnxOfflineRecognizerSetConfig(pointer, &config) }
            self.language = language
        }

        func text(of samples: [Float]) -> String {
            guard let stream = SherpaOnnxCreateOfflineStream(pointer) else { return "" }
            defer { SherpaOnnxDestroyOfflineStream(stream) }
            samples.withUnsafeBufferPointer {
                SherpaOnnxAcceptWaveformOffline(stream, Int32(SenseVoice.sampleRate), $0.baseAddress, Int32($0.count))
            }
            SherpaOnnxDecodeOfflineStream(pointer, stream)
            guard let result = SherpaOnnxGetOfflineStreamResult(stream) else { return "" }
            defer { SherpaOnnxDestroyOfflineRecognizerResult(result) }
            guard let text = result.pointee.text else { return "" }
            return String(cString: text).trimmingCharacters(in: .whitespacesAndNewlines)
        }

        /// Fields left zero take sherpa-onnx's defaults.
        private static func config(
            _ files: Files, _ language: Language, _ strings: CStrings
        ) -> SherpaOnnxOfflineRecognizerConfig {
            var config = SherpaOnnxOfflineRecognizerConfig()
            config.feat_config.sample_rate = Int32(SenseVoice.sampleRate)
            config.feat_config.feature_dim = 80
            config.model_config.tokens = strings(files.tokens)
            config.model_config.num_threads = 2
            config.model_config.sense_voice.model = strings(files.model)
            config.model_config.sense_voice.language = strings(language.rawValue)
            // Punctuation and numbers written as numbers.
            config.model_config.sense_voice.use_itn = 1
            return config
        }
    }

    /// sherpa-onnx's voice activity detector, with Silero's model.
    fileprivate final class VoiceActivityDetector {
        static let window = 512
        private let pointer: OpaquePointer

        init?(_ files: Files) {
            let strings = CStrings()
            var config = SherpaOnnxVadModelConfig()
            config.silero_vad.model = strings(files.vad)
            config.silero_vad.threshold = 0.5
            // A pause this long ends a stretch; shorter ones stay inside it,
            // so a sentence isn't cut up.
            config.silero_vad.min_silence_duration = 0.6
            config.silero_vad.min_speech_duration = 0.25
            config.silero_vad.window_size = Int32(Self.window)
            config.silero_vad.max_speech_duration = 20
            config.sample_rate = Int32(SenseVoice.sampleRate)
            config.num_threads = 1
            guard let pointer = withExtendedLifetime(strings, { SherpaOnnxCreateVoiceActivityDetector(&config, 60) })
            else { return nil }
            self.pointer = pointer
        }

        deinit {
            SherpaOnnxDestroyVoiceActivityDetector(pointer)
        }

        func accept(_ samples: ArraySlice<Float>) {
            samples.withUnsafeBufferPointer {
                SherpaOnnxVoiceActivityDetectorAcceptWaveform(pointer, $0.baseAddress, Int32($0.count))
            }
        }

        /// No more audio: what is held becomes the last stretch.
        func flush() {
            SherpaOnnxVoiceActivityDetectorFlush(pointer)
        }

        /// The next stretch of speech that has ended, if any.
        func next() -> [Float]? {
            guard SherpaOnnxVoiceActivityDetectorEmpty(pointer) == 0,
                  let segment = SherpaOnnxVoiceActivityDetectorFront(pointer)
            else { return nil }
            defer {
                SherpaOnnxDestroySpeechSegment(segment)
                SherpaOnnxVoiceActivityDetectorPop(pointer)
            }
            return Array(UnsafeBufferPointer(start: segment.pointee.samples, count: Int(segment.pointee.n)))
        }
    }
}

// MARK: - Listening

extension SenseVoice {
    /// Hears one recording as it comes, and turns each stretch of speech
    /// into text when the speaker pauses.
    final class Listener {
        private let senseVoice: SenseVoice
        private let language: Language
        private let heard: (String) -> Void
        private let failed: () -> Void
        private var detector: VoiceActivityDetector?
        /// Audio not yet given to the detector, which takes 512 samples at a
        /// time.
        private var pending: [Float] = []
        private let lock = NSLock()
        private var stopped = false

        /// `heard` gets each stretch's text, in order, and `failed` is called
        /// if the models can't be loaded; both on SenseVoice's queue.
        init(
            _ senseVoice: SenseVoice, language: Language,
            heard: @escaping (String) -> Void, failed: @escaping () -> Void
        ) {
            self.senseVoice = senseVoice
            self.language = language
            self.heard = heard
            self.failed = failed
            senseVoice.queue.async { [self] in
                detector = VoiceActivityDetector(senseVoice.files)
                if detector == nil {
                    fail()
                }
            }
            senseVoice.prepare(language)
        }

        /// More of the recording, 16 kHz and one channel; from any thread.
        func feed(_ samples: [Float]) {
            guard !samples.isEmpty else { return }
            senseVoice.queue.async { [self] in
                guard !isStopped, let detector = detector else { return }
                pending.append(contentsOf: samples)
                let whole = pending.count - pending.count % VoiceActivityDetector.window
                for start in stride(from: 0, to: whole, by: VoiceActivityDetector.window) {
                    detector.accept(pending[start..<(start + VoiceActivityDetector.window)])
                }
                pending.removeFirst(whole)
                recognizeEndedSpeech(detector)
            }
        }

        /// The recording is over: turn what is left into text, then call
        /// `done`, on SenseVoice's queue.
        func finish(_ done: @escaping () -> Void) {
            senseVoice.queue.async { [self] in
                if !isStopped, let detector = detector {
                    if !pending.isEmpty {
                        let silence = [Float](repeating: 0, count: VoiceActivityDetector.window - pending.count)
                        detector.accept((pending + silence)[...])
                        pending = []
                    }
                    detector.flush()
                    recognizeEndedSpeech(detector)
                }
                done()
            }
        }

        /// Drop the rest of the recording, whether heard or not.
        func cancel() {
            lock.lock()
            stopped = true
            lock.unlock()
        }

        private var isStopped: Bool {
            lock.lock()
            defer { lock.unlock() }
            return stopped
        }

        private func fail() {
            cancel()
            failed()
        }

        private func recognizeEndedSpeech(_ detector: VoiceActivityDetector) {
            while let samples = detector.next() {
                guard !isStopped else { return }
                guard let text = senseVoice.recognize(samples, language: language) else {
                    return fail()
                }
                // A cough or a click can come back as punctuation alone.
                if text.contains(where: { $0.isLetter || $0.isNumber }) {
                    heard(text)
                }
            }
        }
    }

    /// Turns the microphone's audio into what SenseVoice takes.
    final class Converter {
        private let converter: AVAudioConverter
        private let format: AVAudioFormat

        init?(from input: AVAudioFormat) {
            guard
                let format = AVAudioFormat(
                    commonFormat: .pcmFormatFloat32, sampleRate: Double(SenseVoice.sampleRate),
                    channels: 1, interleaved: false),
                let converter = AVAudioConverter(from: input, to: format)
            else { return nil }
            self.converter = converter
            self.format = format
        }

        /// `buffer` as 16 kHz samples on one channel. What the converter can't
        /// finish yet comes out with the next buffer.
        func samples(_ buffer: AVAudioPCMBuffer) -> [Float] {
            let ratio = format.sampleRate / buffer.format.sampleRate
            let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 64
            guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return [] }
            var given = false
            var error: NSError?
            let status = converter.convert(to: output, error: &error) { _, inputStatus in
                if given {
                    inputStatus.pointee = .noDataNow
                    return nil
                }
                given = true
                inputStatus.pointee = .haveData
                return buffer
            }
            guard status != .error, let channel = output.floatChannelData?[0] else { return [] }
            return Array(UnsafeBufferPointer(start: channel, count: Int(output.frameLength)))
        }
    }
}
