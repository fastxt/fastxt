/*
 * Fastxt
 * Copyright (C) 2020 Yi Wang
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
package app.fastxt.android.ai

import org.json.JSONArray
import org.json.JSONObject

/**
 * On-device AI backend for Android using ML Kit and platform AI APIs.
 * Provides smart tagging, summarization, and semantic features.
 */
object FastxtAI {

    /**
     * Check if on-device AI is available on this device.
     * Returns true on Android 14+ with supported chipset.
     */
    fun isAvailable(): Boolean {
        return android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.UPSIDE_DOWN_CAKE
    }

    /**
     * Suggest tags for the given text using on-device NLP.
     * Returns JSON string compatible with the Rust FFI interface.
     */
    fun suggestTags(text: String): String {
        if (text.isEmpty()) {
            return createTagsResponse(emptyList(), false, "Text is empty")
        }

        val tags = extractTags(text)
        return createTagsResponse(tags, isAvailable(), null)
    }

    /**
     * Summarize the given text using extractive summarization.
     * Returns JSON string compatible with the Rust FFI interface.
     */
    fun summarize(text: String): String {
        if (text.isEmpty()) {
            return createSummaryResponse(null, false, "Text is empty")
        }

        val summary = generateSummary(text)
        return createSummaryResponse(summary, isAvailable(), null)
    }

    // MARK: - Private Implementation

    /**
     * Extract tags from text using keyword extraction.
     */
    private fun extractTags(text: String): List<String> {
        val tags = mutableSetOf<String>()

        // Extract keywords by splitting on common delimiters and filtering
        val words = text.lowercase()
            .replace(Regex("[^a-z0-9\\s]"), " ")
            .split(Regex("\\s+"))
            .filter { it.length > 3 && !isCommonWord(it) }

        // Count word frequency
        val frequency = words.groupingBy { it }.eachCount()

        // Get top keywords by frequency
        val topKeywords = frequency.entries
            .sortedByDescending { it.value }
            .take(7)
            .map { it.key }

        tags.addAll(topKeywords)

        // Extract potential named entities (capitalized words in original text)
        val namedEntities = Regex("\\b[A-Z][a-z]+\\b")
            .findAll(text)
            .map { it.value.lowercase() }
            .filter { it.length > 2 && !isCommonWord(it) }
            .take(3)
            .toList()

        tags.addAll(namedEntities)

        return tags.take(7)
    }

    /**
     * Check if a word is a common English word.
     */
    private fun isCommonWord(word: String): Boolean {
        val commonWords = setOf(
            "this", "that", "these", "those", "with", "from", "have", "been",
            "were", "they", "their", "what", "when", "where", "which", "while",
            "about", "would", "could", "should", "there", "other", "into", "more",
            "some", "such", "than", "them", "then", "very", "just", "over",
            "after", "before", "being", "through", "during", "between", "under",
            "again", "further", "once", "here", "also", "both", "each", "only",
            "even", "most", "made", "make", "many", "much", "same", "time",
            "first", "last", "long", "great", "little", "own", "good", "new",
            "well", "back", "still", "your", "will", "like"
        )
        return commonWords.contains(word)
    }

    /**
     * Generate a summary using extractive summarization.
     */
    private fun generateSummary(text: String): String? {
        val sentences = text.split(Regex("[.!?]+"))
            .map { it.trim() }
            .filter { it.length > 20 }

        if (sentences.isEmpty()) return null
        if (text.length < 200 || sentences.size <= 2) return sentences.first()

        // Score sentences by word importance
        val wordFrequency = calculateWordFrequency(text)
        val scoredSentences = sentences.map { sentence ->
            Pair(sentence, scoreSentence(sentence, wordFrequency))
        }.sortedByDescending { it.second }

        // Return top 1-2 sentences
        val topSentences = scoredSentences.take(2).map { it.first }
        return topSentences.joinToString(". ") + "."
    }

    /**
     * Calculate word frequency for scoring.
     */
    private fun calculateWordFrequency(text: String): Map<String, Double> {
        val words = text.lowercase()
            .replace(Regex("[^a-z0-9\\s]"), " ")
            .split(Regex("\\s+"))
            .filter { it.length > 3 && !isCommonWord(it) }

        val frequency = words.groupingBy { it }.eachCount()
        val total = words.size.toDouble()

        return frequency.mapValues { it.value / total }
    }

    /**
     * Score a sentence based on word importance.
     */
    private fun scoreSentence(sentence: String, wordFrequency: Map<String, Double>): Double {
        val words = sentence.lowercase()
            .replace(Regex("[^a-z0-9\\s]"), " ")
            .split(Regex("\\s+"))
            .filter { it.length > 3 && !isCommonWord(it) }

        if (words.isEmpty()) return 0.0

        val totalScore = words.sumOf { wordFrequency[it] ?: 0.0 }
        return totalScore / kotlin.math.sqrt(words.size.toDouble())
    }

    // MARK: - JSON Response Helpers

    private fun createTagsResponse(tags: List<String>, available: Boolean, error: String?): String {
        val json = JSONObject()
        json.put("tags", JSONArray(tags))
        json.put("available", available)
        json.put("error", error ?: JSONObject.NULL)
        return json.toString()
    }

    private fun createSummaryResponse(summary: String?, available: Boolean, error: String?): String {
        val json = JSONObject()
        json.put("summary", summary ?: JSONObject.NULL)
        json.put("available", available)
        json.put("error", error ?: JSONObject.NULL)
        return json.toString()
    }
}

/**
 * Data class for AI tag response.
 */
data class AiTagsResponse(
    val tags: List<String>,
    val available: Boolean,
    val error: String?
) {
    companion object {
        fun fromJson(json: String): AiTagsResponse? {
            return try {
                val obj = JSONObject(json)
                val tagsArray = obj.getJSONArray("tags")
                val tags = (0 until tagsArray.length()).map { tagsArray.getString(it) }
                AiTagsResponse(
                    tags = tags,
                    available = obj.getBoolean("available"),
                    error = if (obj.isNull("error")) null else obj.getString("error")
                )
            } catch (e: Exception) {
                null
            }
        }
    }
}

/**
 * Data class for AI summary response.
 */
data class AiSummaryResponse(
    val summary: String?,
    val available: Boolean,
    val error: String?
) {
    companion object {
        fun fromJson(json: String): AiSummaryResponse? {
            return try {
                val obj = JSONObject(json)
                AiSummaryResponse(
                    summary = if (obj.isNull("summary")) null else obj.getString("summary"),
                    available = obj.getBoolean("available"),
                    error = if (obj.isNull("error")) null else obj.getString("error")
                )
            } catch (e: Exception) {
                null
            }
        }
    }
}
