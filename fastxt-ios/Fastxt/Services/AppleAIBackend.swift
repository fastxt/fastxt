//
//  AppleAIBackend.swift
//  Fastxt
//
//  On-device AI: Apple Foundation Models on iOS 26+ where available,
//  Natural Language heuristics everywhere else. All processing stays on-device.
//

import Foundation
import NaturalLanguage
#if canImport(FoundationModels)
import FoundationModels
#endif

/// Response type for AI tag suggestions.
struct AiTagsResponse: Codable {
    let tags: [String]
    let available: Bool
    let error: String?
}

/// Response type for AI summarization.
struct AiSummaryResponse: Codable {
    let summary: String?
    let available: Bool
    let error: String?
}

#if canImport(FoundationModels)
/// Structured tag output for guided generation on iOS 26+.
@available(iOS 26.0, macOS 26.0, *)
@Generable
struct TagSuggestions {
    @Guide(description: "3 to 7 short lowercase tags, 1-3 words each", .count(5))
    var tags: [String]
}
#endif

/// Heuristic on-device backend (works on every device the app runs on).
private enum HeuristicAI {
    /// Extract tags with the Natural Language framework.
    static func extractTags(from text: String) -> [String] {
        var tags: Set<String> = []
        // .lexicalClass must be enabled here; the old build asked for it below
        // without enabling it, so noun/adjective extraction silently did nothing.
        let tagger = NLTagger(tagSchemes: [.nameType, .lexicalClass, .language])
        tagger.string = text
        let options: NLTagger.Options = [.omitPunctuation, .omitWhitespace, .joinNames]

        tagger.enumerateTags(in: text.startIndex..<text.endIndex, unit: .word, scheme: .nameType, options: options) { tag, range in
            if let tag = tag {
                let word = String(text[range]).lowercased()
                switch tag {
                case .personalName, .organizationName, .placeName:
                    if word.count > 2 && word.count < 30 {
                        tags.insert(word)
                    }
                default:
                    break
                }
            }
            return true
        }

        tagger.enumerateTags(in: text.startIndex..<text.endIndex, unit: .word, scheme: .lexicalClass, options: options) { tag, range in
            if tag == .noun || tag == .adjective {
                let word = String(text[range]).lowercased()
                if word.count > 3 && word.count < 24 && !isCommonWord(word) {
                    tags.insert(word)
                }
            }
            return true
        }

        // CJK has no spaces: slide a 2-character window over CJK runs.
        for run in cjkRuns(in: text).prefix(6) {
            if run.count >= 2 {
                tags.insert(String(run.prefix(4)))
            }
        }

        if let language = tagger.tag(at: text.startIndex, unit: .paragraph, scheme: .language).0 {
            tags.insert(language.rawValue)
        }

        return Array(tags.prefix(7))
    }

    /// Contiguous CJK runs (Hiragana/Katakana/CJK Unified) of length >= 2.
    static func cjkRuns(in text: String) -> [String] {
        var runs: [String] = []
        var current = ""
        for ch in text {
            if ch.unicodeScalars.contains(where: { scalar in
                (0x3040...0x30FF).contains(scalar.value) || (0x4E00...0x9FFF).contains(scalar.value)
            }) {
                current.append(ch)
            } else if current.count >= 2 {
                runs.append(current)
                current = ""
            } else {
                current = ""
            }
        }
        if current.count >= 2 {
            runs.append(current)
        }
        return runs
    }

    /// Score-based extractive summary (top sentences by word frequency).
    static func generateSummary(from text: String) -> String? {
        let sentences = text.components(separatedBy: CharacterSet(charactersIn: ".!?\n。！？"))
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { $0.count > 20 }

        guard !sentences.isEmpty else { return nil }
        if text.count < 200 || sentences.count <= 2 { return sentences.first }

        let wordFrequency = calculateWordFrequency(in: text)
        let scored = sentences
            .map { ($0, scoreSentence($0, wordFrequency: wordFrequency)) }
            .sorted { $0.1 > $1.1 }
        return scored.prefix(2).map { $0.0 }.joined(separator: ". ") + "."
    }

    private static func calculateWordFrequency(in text: String) -> [String: Double] {
        let words = tokenize(text)
        var frequency: [String: Int] = [:]
        for word in words { frequency[word, default: 0] += 1 }
        let total = Double(words.count)
        return frequency.mapValues { Double($0) / total }
    }

    private static func scoreSentence(_ sentence: String, wordFrequency: [String: Double]) -> Double {
        let words = tokenize(sentence)
        guard !words.isEmpty else { return 0 }
        return words.reduce(0.0) { $0 + (wordFrequency[$1] ?? 0) } / sqrt(Double(words.count))
    }

    private static func tokenize(_ text: String) -> [String] {
        let words = text.lowercased()
            .components(separatedBy: CharacterSet.letters.inverted)
            .filter { $0.count > 3 && !isCommonWord($0) }
        return words + cjkRuns(in: text).flatMap { run in
            (0..<max(0, run.count - 1)).map { String(run[run.index(run.startIndex, offsetBy: $0)..<run.index(run.startIndex, offsetBy: $0 + 2)]) }
        }
    }

    private static func isCommonWord(_ word: String) -> Bool {
        let commonWords: Set<String> = [
            "this", "that", "these", "those", "with", "from", "have", "been",
            "were", "they", "their", "what", "when", "where", "which", "while",
            "about", "would", "could", "should", "there", "other", "into", "more",
            "some", "such", "than", "them", "then", "very", "just", "over",
            "after", "before", "being", "through", "during", "between", "under",
            "again", "further", "once", "here", "also", "both", "each", "only",
            "even", "most", "made", "make", "many", "much", "same", "time",
            "first", "last", "long", "great", "little", "own", "good", "new",
            "well", "back", "still", "way", "get", "go", "see", "know", "take",
            "come", "think", "look", "want", "give", "use", "find", "tell"
        ]
        return commonWords.contains(word)
    }
}

#if canImport(FoundationModels)
/// Apple Foundation Models on iOS 26+: true on-device generative AI.
@available(iOS 26.0, macOS 26.0, *)
private enum FoundationModelAI {
    static var isAvailable: Bool {
        if case .available = SystemLanguageModel.default.availability {
            return true
        }
        return false
    }

    static func suggestTags(text: String, completion: @escaping (AiTagsResponse) -> Void) {
        Task {
            do {
                let session = LanguageModelSession(
                    instructions: "You tag notes. Suggest short lowercase tags (1-3 words each)."
                )
                let prompt = "Suggest 3 to 7 tags for this note:\n\n\(text.prefix(4000))"
                let response = try await session.respond(to: prompt, generating: TagSuggestions.self)
                completion(AiTagsResponse(tags: response.content.tags, available: true, error: nil))
            } catch {
                completion(AiTagsResponse(tags: HeuristicAI.extractTags(from: text), available: true, error: nil))
            }
        }
    }

    static func summarize(text: String, completion: @escaping (AiSummaryResponse) -> Void) {
        Task {
            do {
                let session = LanguageModelSession(
                    instructions: "You summarize notes in 1-2 plain sentences."
                )
                let response = try await session.respond(to: "Summarize:\n\n\(text.prefix(6000))")
                completion(AiSummaryResponse(summary: response.content, available: true, error: nil))
            } catch {
                let summary = HeuristicAI.generateSummary(from: text)
                completion(AiSummaryResponse(summary: summary, available: summary != nil, error: summary == nil ? "Summarization failed" : nil))
            }
        }
    }
}
#endif

/// Unified AI service: Foundation Models where available, heuristics otherwise.
class FastxtAI {

    /// Suggest tags for the given text; returns the JSON the FFI contract uses.
    static func suggestTags(text: String) -> String {
        if #available(iOS 26.0, macOS 26.0, *) {
            #if canImport(FoundationModels)
            if FoundationModelAI.isAvailable {
                let box = DispatchGroup()
                var result = AiTagsResponse(tags: [], available: false, error: "timeout")
                box.enter()
                FoundationModelAI.suggestTags(text: text) { response in
                    result = response
                    box.leave()
                }
                _ = box.wait(timeout: .now() + 30)
                return encode(result)
            }
            #endif
        }
        return encode(AiTagsResponse(tags: HeuristicAI.extractTags(from: text), available: true, error: nil))
    }

    /// Summarize the given text; returns the JSON the FFI contract uses.
    static func summarize(text: String) -> String {
        if #available(iOS 26.0, macOS 26.0, *) {
            #if canImport(FoundationModels)
            if FoundationModelAI.isAvailable {
                let box = DispatchGroup()
                var result = AiSummaryResponse(summary: nil, available: false, error: "timeout")
                box.enter()
                FoundationModelAI.summarize(text: text) { response in
                    result = response
                    box.leave()
                }
                _ = box.wait(timeout: .now() + 60)
                return encode(result)
            }
            #endif
        }
        let summary = HeuristicAI.generateSummary(from: text)
        return encode(AiSummaryResponse(summary: summary, available: summary != nil, error: summary == nil ? "Could not summarize this text" : nil))
    }

    /// Whether on-device AI works here (heuristics always do).
    static func isAvailable() -> Bool {
        return true
    }

    private static func encode<T: Encodable>(_ value: T) -> String {
        if let data = try? JSONEncoder().encode(value), let json = String(data: data, encoding: .utf8) {
            return json
        }
        return "{\"error\":\"encoding failed\"}"
    }
}
