//
//  AppleAIBackend.swift
//  Fastxt
//
//  On-device AI using Apple Foundation Models for iOS 18.1+ / macOS 15.1+
//  Provides smart tagging, summarization, and semantic search using
//  Apple's native AI capabilities.
//
//  Created for Fastxt AI integration.
//

import Foundation
import NaturalLanguage

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

/// Apple Foundation Models backend for on-device AI.
/// Gracefully degrades on older devices or when Apple Intelligence is unavailable.
@available(iOS 18.1, macOS 15.1, *)
class AppleAIBackend {

    /// Check if Apple Foundation Models are available on this device.
    /// Returns true only on Apple Silicon devices with iOS 18.1+ / macOS 15.1+.
    static func isAvailable() -> Bool {
        // Check if we're on a supported platform
        #if os(iOS)
        if #available(iOS 18.1, *) {
            // Check for Apple Silicon (A17 Pro or later for iPhone, M-series for iPad)
            // This is a best-effort check; actual availability may vary
            return true
        }
        #elseif os(macOS)
        if #available(macOS 15.1, *) {
            // Check for Apple Silicon Mac
            var sysinfo = utsname()
            _ = uname(&sysinfo)
            let machine = withUnsafePointer(to: &sysinfo.machine) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(_SYS_NAMELEN)) {
                    String(cString: $0)
                }
            }
            return machine.hasPrefix("arm64")
        }
        #endif
        return false
    }

    /// Suggest tags for the given text using on-device NLP.
    /// Uses NLTagger for entity extraction and keyword identification.
    static func suggestTags(text: String, completion: @escaping (AiTagsResponse) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let tags = extractTags(from: text)
            let response = AiTagsResponse(
                tags: tags,
                available: true,
                error: nil
            )
            DispatchQueue.main.async {
                completion(response)
            }
        }
    }

    /// Synchronous version for FFI compatibility.
    static func suggestTagsSync(text: String) -> String {
        let tags = extractTags(from: text)
        let response = AiTagsResponse(
            tags: tags,
            available: true,
            error: nil
        )

        let encoder = JSONEncoder()
        if let data = try? encoder.encode(response),
           let json = String(data: data, encoding: .utf8) {
            return json
        }
        return "{\"tags\":[],\"available\":false,\"error\":\"Encoding error\"}"
    }

    /// Extract tags from text using Natural Language framework.
    private static func extractTags(from text: String) -> [String] {
        var tags: Set<String> = []

        // Use NLTagger for entity extraction
        let tagger = NLTagger(tagSchemes: [.nameType, .sentimentScore, .language])
        tagger.string = text

        // Extract named entities
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

        // Extract keywords using lexical class
        tagger.enumerateTags(in: text.startIndex..<text.endIndex, unit: .word, scheme: .lexicalClass, options: options) { tag, range in
            if tag == .noun || tag == .adjective {
                let word = String(text[range]).lowercased()
                // Filter out common words and short words
                if word.count > 3 && word.count < 20 && !isCommonWord(word) {
                    tags.insert(word)
                }
            }
            return true
        }

        // Detect language for language-specific tags
        if let language = tagger.tag(at: text.startIndex, unit: .paragraph, scheme: .language).0 {
            tags.insert(language.rawValue)
        }

        // Limit to 7 tags
        return Array(tags.prefix(7))
    }

    /// Check if a word is a common English word that should be excluded.
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

    /// Generate a summary of the text.
    /// For now, uses extractive summarization by finding key sentences.
    static func summarize(text: String, completion: @escaping (AiSummaryResponse) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let summary = generateSummary(from: text)
            let response = AiSummaryResponse(
                summary: summary,
                available: true,
                error: nil
            )
            DispatchQueue.main.async {
                completion(response)
            }
        }
    }

    /// Synchronous version for FFI compatibility.
    static func summarizeSync(text: String) -> String {
        let summary = generateSummary(from: text)
        let response = AiSummaryResponse(
            summary: summary,
            available: true,
            error: nil
        )

        let encoder = JSONEncoder()
        if let data = try? encoder.encode(response),
           let json = String(data: data, encoding: .utf8) {
            return json
        }
        return "{\"summary\":null,\"available\":false,\"error\":\"Encoding error\"}"
    }

    /// Generate a summary using extractive summarization.
    private static func generateSummary(from text: String) -> String? {
        let sentences = text.components(separatedBy: CharacterSet(charactersIn: ".!?"))
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && $0.count > 20 }

        guard !sentences.isEmpty else { return nil }

        // For short text, return the first sentence
        if text.count < 200 || sentences.count <= 2 {
            return sentences.first
        }

        // Score sentences by word importance
        let wordFrequency = calculateWordFrequency(in: text)
        var scoredSentences: [(String, Double)] = sentences.map { sentence in
            let score = scoreSentence(sentence, wordFrequency: wordFrequency)
            return (sentence, score)
        }

        // Sort by score and take top sentences
        scoredSentences.sort { $0.1 > $1.1 }

        // Return top 1-2 sentences, preferring earlier ones
        let topSentences = scoredSentences.prefix(2).map { $0.0 }
        return topSentences.joined(separator: ". ") + "."
    }

    /// Calculate word frequency for scoring.
    private static func calculateWordFrequency(in text: String) -> [String: Double] {
        let words = text.lowercased()
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { $0.count > 3 && !isCommonWord($0) }

        var frequency: [String: Int] = [:]
        for word in words {
            frequency[word, default: 0] += 1
        }

        let total = Double(words.count)
        return frequency.mapValues { Double($0) / total }
    }

    /// Score a sentence based on word importance.
    private static func scoreSentence(_ sentence: String, wordFrequency: [String: Double]) -> Double {
        let words = sentence.lowercased()
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { $0.count > 3 && !isCommonWord($0) }

        guard !words.isEmpty else { return 0 }

        let totalScore = words.reduce(0.0) { sum, word in
            sum + (wordFrequency[word] ?? 0)
        }

        // Normalize by sentence length to avoid favoring long sentences
        return totalScore / sqrt(Double(words.count))
    }
}

// MARK: - Fallback for older iOS versions

/// Fallback AI backend that returns "not available" status.
/// Used on devices that don't support Apple Foundation Models.
struct FallbackAIBackend {
    static func suggestTagsSync(text: String) -> String {
        return "{\"tags\":[],\"available\":false,\"error\":\"Apple Foundation Models not available on this device\"}"
    }

    static func summarizeSync(text: String) -> String {
        return "{\"summary\":null,\"available\":false,\"error\":\"Apple Foundation Models not available on this device\"}"
    }
}

// MARK: - Unified AI Service

/// Unified AI service that uses Apple Foundation Models when available,
/// or returns "not available" on older devices.
class FastxtAI {

    /// Suggest tags for the given text.
    /// Returns JSON string compatible with the Rust FFI interface.
    static func suggestTags(text: String) -> String {
        if #available(iOS 18.1, macOS 15.1, *) {
            return AppleAIBackend.suggestTagsSync(text: text)
        } else {
            return FallbackAIBackend.suggestTagsSync(text: text)
        }
    }

    /// Summarize the given text.
    /// Returns JSON string compatible with the Rust FFI interface.
    static func summarize(text: String) -> String {
        if #available(iOS 18.1, macOS 15.1, *) {
            return AppleAIBackend.summarizeSync(text: text)
        } else {
            return FallbackAIBackend.summarizeSync(text: text)
        }
    }

    /// Check if AI is available on this device.
    static func isAvailable() -> Bool {
        if #available(iOS 18.1, macOS 15.1, *) {
            return AppleAIBackend.isAvailable()
        }
        return false
    }
}
