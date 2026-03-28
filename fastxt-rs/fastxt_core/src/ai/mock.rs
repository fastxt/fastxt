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

//! Mock AI backend for testing.
//!
//! This module provides a mock implementation of `AiBackend` that returns
//! predictable results without requiring an actual AI model. Useful for
//! unit tests and integration tests.

use super::{AiBackend, AiConfig, AiError, AiResult};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A mock AI backend for testing purposes.
///
/// Returns predictable responses based on the input text,
/// allowing tests to verify AI-dependent code without a real backend.
pub struct MockBackend {
    /// Whether the backend reports as available
    available: bool,
    /// Tags to return for any input
    default_tags: Vec<String>,
    /// Call counter for varying output in self-consistency tests.
    /// Each call to `suggest_tags` increments this counter and
    /// slightly varies the returned tags.
    call_counter: Arc<AtomicUsize>,
    /// When true, vary tags on each call for consistency testing
    vary_tags: bool,
}

impl Default for MockBackend {
    fn default() -> Self {
        MockBackend {
            available: true,
            default_tags: vec!["mock-tag".to_string()],
            call_counter: Arc::new(AtomicUsize::new(0)),
            vary_tags: false,
        }
    }
}

impl MockBackend {
    /// Create a new mock backend with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a mock backend that reports as unavailable.
    pub fn unavailable() -> Self {
        MockBackend {
            available: false,
            default_tags: vec![],
            call_counter: Arc::new(AtomicUsize::new(0)),
            vary_tags: false,
        }
    }

    /// Create a mock backend with custom tags.
    pub fn with_tags(tags: Vec<&str>) -> Self {
        MockBackend {
            available: true,
            default_tags: tags.into_iter().map(|s| s.to_string()).collect(),
            call_counter: Arc::new(AtomicUsize::new(0)),
            vary_tags: false,
        }
    }

    /// Create a mock backend that varies tags on each call.
    ///
    /// Uses the provided base tags and swaps some out on each call.
    /// Useful for testing self-consistency voting:
    /// - Call 0: base tags as-is
    /// - Call 1: replaces last tag with "variant-1"
    /// - Call 2: replaces second-to-last tag with "variant-2"
    ///
    /// Tags that appear in all calls will pass majority voting,
    /// while variant tags will be filtered out.
    pub fn with_varying_tags(tags: Vec<&str>) -> Self {
        MockBackend {
            available: true,
            default_tags: tags.into_iter().map(|s| s.to_string()).collect(),
            call_counter: Arc::new(AtomicUsize::new(0)),
            vary_tags: true,
        }
    }
}

impl AiBackend for MockBackend {
    fn is_available(&self) -> bool {
        self.available
    }

    fn suggest_tags(&self, text: &str, _config: &AiConfig) -> AiResult<Vec<String>> {
        if !self.available {
            return Err(AiError::Unavailable);
        }

        let call_num = self.call_counter.fetch_add(1, Ordering::Relaxed);

        // Return default tags plus any words that look like keywords (>4 chars, capitalized)
        let mut tags = self.default_tags.clone();

        // When vary_tags is enabled, swap out some tags based on call number
        if self.vary_tags && !tags.is_empty() {
            match call_num % 3 {
                0 => {
                    // Call 0: base tags as-is
                }
                1 => {
                    // Call 1: replace last tag with a variant
                    if let Some(last) = tags.last_mut() {
                        *last = "variant-1".to_string();
                    }
                }
                2 => {
                    // Call 2: replace second-to-last tag with a variant
                    let len = tags.len();
                    if len >= 2 {
                        tags[len - 2] = "variant-2".to_string();
                    } else if let Some(last) = tags.last_mut() {
                        *last = "variant-2".to_string();
                    }
                }
                _ => {}
            }
        }

        for word in text.split_whitespace() {
            let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
            if cleaned.len() > 4
                && let Some(first) = cleaned.chars().next()
                && first.is_uppercase()
            {
                tags.push(cleaned.to_lowercase());
            }
        }

        // Deduplicate and limit
        tags.sort();
        tags.dedup();
        tags.truncate(10);

        Ok(tags)
    }

    fn summarize(&self, text: &str, _config: &AiConfig) -> AiResult<String> {
        if !self.available {
            return Err(AiError::Unavailable);
        }

        // Return first sentence or first 100 chars
        let first_sentence = text.split(['.', '!', '?']).next().unwrap_or(text);

        if first_sentence.chars().count() > 100 {
            let truncated: String = first_sentence.chars().take(100).collect();
            Ok(format!("{}...", truncated))
        } else {
            Ok(first_sentence.to_string())
        }
    }

    fn embed(&self, text: &str, _config: &AiConfig) -> AiResult<Vec<f32>> {
        if !self.available {
            return Err(AiError::Unavailable);
        }

        // Generate a deterministic pseudo-embedding based on text hash
        // This is NOT a real embedding, just a predictable vector for testing
        let hash = Self::simple_hash(text);
        let dimension = 384; // Common embedding dimension

        Ok((0..dimension)
            .map(|i| ((hash.wrapping_add(i as u64)) % 1000) as f32 / 1000.0)
            .collect())
    }

    fn categorize(&self, texts: &[&str], _config: &AiConfig) -> AiResult<Vec<String>> {
        if !self.available {
            return Err(AiError::Unavailable);
        }

        // Simple keyword-based categorization for testing
        Ok(texts
            .iter()
            .map(|text| {
                let lower = text.to_lowercase();
                if lower.contains("code") || lower.contains("programming") {
                    "programming"
                } else if lower.contains("meeting") || lower.contains("schedule") {
                    "work"
                } else if lower.contains("buy") || lower.contains("shop") {
                    "shopping"
                } else {
                    "general"
                }
                .to_string()
            })
            .collect())
    }

    fn simplify(&self, text: &str, _config: &AiConfig) -> AiResult<String> {
        if !self.available {
            return Err(AiError::Unavailable);
        }
        Ok(text.to_lowercase())
    }

    fn key_points(&self, text: &str, _config: &AiConfig) -> AiResult<Vec<String>> {
        if !self.available {
            return Err(AiError::Unavailable);
        }
        Ok(text
            .split(['.', '!', '?'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .take(5)
            .collect())
    }

    fn backend_name(&self) -> &str {
        "mock"
    }
}

impl MockBackend {
    /// Simple string hash for deterministic test embeddings.
    fn simple_hash(s: &str) -> u64 {
        let mut hash: u64 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_backend_available() {
        let backend = MockBackend::new();
        assert!(backend.is_available());
    }

    #[test]
    fn test_mock_backend_unavailable() {
        let backend = MockBackend::unavailable();
        assert!(!backend.is_available());
    }

    #[test]
    fn test_mock_suggest_tags() {
        let backend = MockBackend::with_tags(vec!["test"]);
        let config = AiConfig::default();
        let tags = backend.suggest_tags("Hello World Python", &config).unwrap();
        assert!(tags.contains(&"test".to_string()));
    }

    #[test]
    fn test_mock_summarize() {
        let backend = MockBackend::new();
        let config = AiConfig::default();
        let summary = backend
            .summarize("This is a test sentence. And another one.", &config)
            .unwrap();
        assert_eq!(summary, "This is a test sentence");
    }

    #[test]
    fn test_mock_embed() {
        let backend = MockBackend::new();
        let config = AiConfig::default();
        let embedding = backend.embed("test text", &config).unwrap();
        assert_eq!(embedding.len(), 384);
        // Same text should produce same embedding
        let embedding2 = backend.embed("test text", &config).unwrap();
        assert_eq!(embedding, embedding2);
    }

    #[test]
    fn test_mock_categorize() {
        let backend = MockBackend::new();
        let config = AiConfig::default();
        let categories = backend
            .categorize(&["code review", "meeting notes", "buy milk"], &config)
            .unwrap();
        assert_eq!(categories, vec!["programming", "work", "shopping"]);
    }

    #[test]
    fn test_mock_unavailable_errors() {
        let backend = MockBackend::unavailable();
        let config = AiConfig::default();

        assert!(matches!(
            backend.suggest_tags("test", &config),
            Err(AiError::Unavailable)
        ));
        assert!(matches!(
            backend.summarize("test", &config),
            Err(AiError::Unavailable)
        ));
        assert!(matches!(
            backend.embed("test", &config),
            Err(AiError::Unavailable)
        ));
        assert!(matches!(
            backend.categorize(&["test"], &config),
            Err(AiError::Unavailable)
        ));
        assert!(matches!(
            backend.simplify("test", &config),
            Err(AiError::Unavailable)
        ));
        assert!(matches!(
            backend.key_points("test", &config),
            Err(AiError::Unavailable)
        ));
    }

    #[test]
    fn test_mock_simplify() {
        let backend = MockBackend::new();
        let config = AiConfig::default();
        let result = backend.simplify("Hello World", &config).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_mock_key_points() {
        let backend = MockBackend::new();
        let config = AiConfig::default();
        let points = backend
            .key_points("First point. Second point. Third point.", &config)
            .unwrap();
        assert_eq!(points, vec!["First point", "Second point", "Third point"]);
    }

    #[test]
    fn test_mock_key_points_limits_to_five() {
        let backend = MockBackend::new();
        let config = AiConfig::default();
        let points = backend.key_points("A. B. C. D. E. F. G.", &config).unwrap();
        assert_eq!(points.len(), 5);
    }
}
