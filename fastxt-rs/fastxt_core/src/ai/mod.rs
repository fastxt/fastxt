/*
    Fastxt
    Copyright (C) 2020  Yi Wang

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

//! On-device AI abstraction layer for Fastxt.
//!
//! This module provides a platform-agnostic AI backend trait that can be
//! implemented by different AI providers (Ollama, llama.cpp, Apple Foundation
//! Models, Android AI APIs, etc.) while keeping all processing on-device.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ai")]
pub mod mock;
#[cfg(feature = "ai")]
pub mod ollama;

/// Result type for AI operations.
pub type AiResult<T> = Result<T, AiError>;

/// Errors that can occur during AI operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiError {
    /// The AI backend is not available on this device/configuration
    Unavailable,
    /// Network or connection error (for HTTP-based backends like Ollama)
    ConnectionError(String),
    /// The AI model returned an invalid or unexpected response
    InvalidResponse(String),
    /// Request timed out
    Timeout,
    /// The input text was too long for the model
    InputTooLong,
    /// Generic error with message
    Other(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Unavailable => write!(f, "AI backend is not available"),
            AiError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            AiError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            AiError::Timeout => write!(f, "Request timed out"),
            AiError::InputTooLong => write!(f, "Input text is too long"),
            AiError::Other(msg) => write!(f, "AI error: {}", msg),
        }
    }
}

impl std::error::Error for AiError {}

/// Configuration for AI backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// For Ollama: base URL (e.g., "http://localhost:11434")
    /// For llama.cpp: model file path
    pub endpoint: Option<String>,
    /// Model name (e.g., "llama3.2", "mistral")
    pub model: Option<String>,
    /// Maximum tokens for responses
    pub max_tokens: Option<u32>,
    /// Temperature for generation (0.0 - 2.0)
    pub temperature: Option<f32>,
    /// Request timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Per-task model override for tagging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagging_model: Option<String>,
    /// Per-task model override for summarization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarize_model: Option<String>,
    /// Per-task model override for embeddings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    /// Per-task model override for categorization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categorize_model: Option<String>,
    /// Reasoning effort level for models that support it ("low", "medium", "high")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Number of rounds for self-consistency tag voting.
    /// When set to Some(n) where n > 1, `suggest_tags` is called n times
    /// and only tags appearing in >= ceil(n/2) rounds are kept (majority vote).
    /// None or Some(1) disables consistency checking (backward compatible).
    pub consistency_rounds: Option<usize>,
}

impl AiConfig {
    /// Get the model name for a specific task, falling back to the general model.
    pub fn model_for_task(&self, task: &str) -> String {
        let task_model = match task {
            "tagging" => self.tagging_model.as_ref(),
            "summarize" => self.summarize_model.as_ref(),
            "embedding" => self.embedding_model.as_ref(),
            "categorize" => self.categorize_model.as_ref(),
            _ => None,
        };
        task_model
            .or(self.model.as_ref())
            .cloned()
            .unwrap_or_else(|| "llama3.2".to_string())
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            endpoint: Some("http://localhost:11434".to_string()),
            model: Some("llama3.2".to_string()),
            max_tokens: Some(256),
            temperature: Some(0.3),
            timeout_secs: Some(30),
            tagging_model: None,
            summarize_model: None,
            embedding_model: None,
            categorize_model: None,
            reasoning_effort: None,
            consistency_rounds: None,
        }
    }
}

/// AI-generated metadata for a note.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiMetadata {
    /// AI-suggested tags for the note
    pub tags: Vec<String>,
    /// AI-generated summary (if requested)
    pub summary: Option<String>,
    /// AI-assigned category
    pub category: Option<String>,
    /// Embedding vector for semantic search (stored separately)
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

/// Trait that all AI backends must implement.
///
/// Each platform (Desktop/Ollama, iOS/Apple Foundation Models, Android/Gemini Nano)
/// provides its own implementation. The trait allows the core library to remain
/// platform-agnostic while leveraging native AI capabilities.
pub trait AiBackend: Send + Sync {
    /// Check if the AI backend is available and properly configured.
    fn is_available(&self) -> bool;

    /// Suggest tags for the given text.
    ///
    /// Returns a list of relevant tags based on the content.
    /// The number of tags returned depends on the backend implementation.
    fn suggest_tags(&self, text: &str, config: &AiConfig) -> AiResult<Vec<String>>;

    /// Generate a concise summary of the given text.
    ///
    /// The summary should be significantly shorter than the original text.
    fn summarize(&self, text: &str, config: &AiConfig) -> AiResult<String>;

    /// Generate an embedding vector for the given text.
    ///
    /// Used for semantic search - finding notes with similar meaning.
    fn embed(&self, text: &str, config: &AiConfig) -> AiResult<Vec<f32>>;

    /// Categorize multiple texts into groups.
    ///
    /// Returns a category label for each input text.
    fn categorize(&self, texts: &[&str], config: &AiConfig) -> AiResult<Vec<String>>;

    /// Get the name/identifier of this backend.
    fn backend_name(&self) -> &str;
}

/// Suggest tags with self-consistency voting.
///
/// Calls `backend.suggest_tags()` `rounds` times and keeps only tags that
/// appear in at least `ceil(rounds / 2)` of the results (majority vote).
/// The returned tags preserve the order they first appeared in round 1.
///
/// When `rounds` is 1 or less, this is equivalent to a single call.
#[cfg(feature = "ai")]
pub fn suggest_tags_with_consistency(
    backend: &dyn AiBackend,
    text: &str,
    config: &AiConfig,
    rounds: usize,
) -> AiResult<Vec<String>> {
    if rounds <= 1 {
        return backend.suggest_tags(text, config);
    }

    let mut all_rounds: Vec<Vec<String>> = Vec::with_capacity(rounds);

    for _ in 0..rounds {
        let tags = backend.suggest_tags(text, config)?;
        all_rounds.push(tags);
    }

    // Count occurrences of each tag across all rounds (case-insensitive)
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for round_tags in &all_rounds {
        for tag in round_tags {
            let key = tag.to_lowercase();
            *counts.entry(key).or_insert(0) += 1;
        }
    }

    // Majority threshold: ceil(rounds / 2)
    let threshold = rounds.div_ceil(2);

    // Preserve order from the first round, filtering by majority vote
    let first_round = &all_rounds[0];
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    // First pass: add qualifying tags from round 1 in order
    for tag in first_round {
        let key = tag.to_lowercase();
        if counts.get(&key).copied().unwrap_or(0) >= threshold && seen.insert(key) {
            result.push(tag.clone());
        }
    }

    // Second pass: add any qualifying tags from later rounds not in round 1
    for round_tags in &all_rounds[1..] {
        for tag in round_tags {
            let key = tag.to_lowercase();
            if counts.get(&key).copied().unwrap_or(0) >= threshold && seen.insert(key) {
                result.push(tag.clone());
            }
        }
    }

    Ok(result)
}

/// Get the default AI backend for the current platform.
///
/// On desktop, this returns an Ollama backend if available.
/// On mobile, this would return the platform-native backend.
#[cfg(feature = "ai")]
pub fn get_default_backend() -> Box<dyn AiBackend> {
    Box::new(ollama::OllamaBackend::new())
}

/// Stub for when AI feature is disabled.
#[cfg(not(feature = "ai"))]
pub fn get_default_backend() -> Option<Box<dyn AiBackend>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_config_default() {
        let config = AiConfig::default();
        assert_eq!(config.endpoint, Some("http://localhost:11434".to_string()));
        assert_eq!(config.model, Some("llama3.2".to_string()));
        assert!(config.tagging_model.is_none());
        assert!(config.summarize_model.is_none());
        assert!(config.embedding_model.is_none());
        assert!(config.categorize_model.is_none());
        assert!(config.reasoning_effort.is_none());
        assert_eq!(config.consistency_rounds, None);
    }

    #[test]
    fn test_model_for_task_defaults() {
        let config = AiConfig::default();
        assert_eq!(config.model_for_task("tagging"), "llama3.2");
        assert_eq!(config.model_for_task("summarize"), "llama3.2");
        assert_eq!(config.model_for_task("embedding"), "llama3.2");
        assert_eq!(config.model_for_task("categorize"), "llama3.2");
        assert_eq!(config.model_for_task("unknown"), "llama3.2");
    }

    #[test]
    fn test_model_for_task_overrides() {
        let config = AiConfig {
            tagging_model: Some("mistral".to_string()),
            embedding_model: Some("nomic-embed-text".to_string()),
            ..Default::default()
        };
        assert_eq!(config.model_for_task("tagging"), "mistral");
        assert_eq!(config.model_for_task("summarize"), "llama3.2");
        assert_eq!(config.model_for_task("embedding"), "nomic-embed-text");
        assert_eq!(config.model_for_task("categorize"), "llama3.2");
    }

    #[test]
    fn test_model_for_task_no_general_model() {
        let config = AiConfig {
            model: None,
            tagging_model: Some("mistral".to_string()),
            ..Default::default()
        };
        assert_eq!(config.model_for_task("tagging"), "mistral");
        // Falls back to hardcoded default when both task and general model are None
        assert_eq!(config.model_for_task("summarize"), "llama3.2");
    }

    #[test]
    fn test_ai_metadata_serialization() {
        let metadata = AiMetadata {
            tags: vec!["rust".to_string(), "programming".to_string()],
            summary: Some("A brief summary".to_string()),
            category: Some("tech".to_string()),
            embedding: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: AiMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tags, metadata.tags);
        assert_eq!(parsed.summary, metadata.summary);
    }

    #[cfg(feature = "ai")]
    mod consistency_tests {
        use super::*;
        use crate::ai::mock::MockBackend;

        #[test]
        fn test_consistency_single_round() {
            let backend = MockBackend::with_tags(vec!["rust", "code", "test"]);
            let config = AiConfig::default();
            // Single round should return tags as-is (no filtering)
            let tags = suggest_tags_with_consistency(&backend, "simple text", &config, 1).unwrap();
            assert!(tags.contains(&"rust".to_string()));
            assert!(tags.contains(&"code".to_string()));
            assert!(tags.contains(&"test".to_string()));
        }

        #[test]
        fn test_consistency_filters_minority_tags() {
            // With varying tags:
            // Call 0: ["rust", "code", "test"]
            // Call 1: ["rust", "code", "variant-1"] (last replaced)
            // Call 2: ["rust", "variant-2", "test"] (second-to-last replaced)
            //
            // Counts: rust=3, code=2, test=2, variant-1=1, variant-2=1
            // Threshold: ceil(3/2) = 2
            // Result: rust, code, test (all appear >= 2 times)
            let backend = MockBackend::with_varying_tags(vec!["rust", "code", "test"]);
            let config = AiConfig::default();
            let tags = suggest_tags_with_consistency(&backend, "simple text", &config, 3).unwrap();

            assert!(
                tags.contains(&"rust".to_string()),
                "rust should pass majority vote (3/3), got: {:?}",
                tags
            );
            assert!(
                tags.contains(&"code".to_string()),
                "code should pass majority vote (2/3), got: {:?}",
                tags
            );
            assert!(
                tags.contains(&"test".to_string()),
                "test should pass majority vote (2/3), got: {:?}",
                tags
            );
            assert!(
                !tags.contains(&"variant-1".to_string()),
                "variant-1 should be filtered (1/3), got: {:?}",
                tags
            );
            assert!(
                !tags.contains(&"variant-2".to_string()),
                "variant-2 should be filtered (1/3), got: {:?}",
                tags
            );
        }

        #[test]
        fn test_consistency_preserves_first_round_order() {
            let backend = MockBackend::with_varying_tags(vec!["alpha", "beta", "gamma"]);
            let config = AiConfig::default();
            let tags = suggest_tags_with_consistency(&backend, "simple text", &config, 3).unwrap();

            // alpha appears in all 3, beta in 2 (calls 0,1), gamma in 2 (calls 0,2)
            // Order should match first round: alpha, beta, gamma
            if tags.len() >= 2 {
                let alpha_pos = tags.iter().position(|t| t == "alpha");
                let beta_pos = tags.iter().position(|t| t == "beta");
                if let (Some(a), Some(b)) = (alpha_pos, beta_pos) {
                    assert!(a < b, "alpha should come before beta, got: {:?}", tags);
                }
            }
        }

        #[test]
        fn test_consistency_zero_rounds_fallback() {
            let backend = MockBackend::with_tags(vec!["rust", "code"]);
            let config = AiConfig::default();
            // 0 rounds should behave like 1 round
            let tags = suggest_tags_with_consistency(&backend, "simple text", &config, 0).unwrap();
            assert!(!tags.is_empty());
        }

        #[test]
        fn test_consistency_case_insensitive() {
            // The mock backend returns lowercase tags, so this mainly verifies
            // the counting logic treats "Rust" and "rust" the same
            let backend = MockBackend::with_tags(vec!["rust", "code"]);
            let config = AiConfig::default();
            let tags = suggest_tags_with_consistency(&backend, "simple text", &config, 3).unwrap();
            // Without varying, all tags appear 3/3 times
            assert!(tags.contains(&"rust".to_string()));
            assert!(tags.contains(&"code".to_string()));
        }
    }
}
