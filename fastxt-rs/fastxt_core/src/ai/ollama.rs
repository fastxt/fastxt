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

//! Ollama backend for on-device AI via HTTP API.
//!
//! This backend connects to a locally running Ollama server (default: localhost:11434)
//! to perform AI operations. Ollama must be installed and running separately.
//!
//! # Setup
//! 1. Install Ollama: https://ollama.ai
//! 2. Pull a model: `ollama pull llama3.2`
//! 3. Ollama runs automatically as a background service

use super::{AiBackend, AiConfig, AiError, AiResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Truncate a string to at most `max_chars` bytes, ensuring the cut
/// falls on a valid UTF-8 character boundary to prevent panics.
pub fn truncate_str(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    // Walk backwards from max_chars to find a char boundary
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Ollama API request for generating text.
#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<GenerateOptions>,
    /// JSON schema for structured output. When set, Ollama constrains the
    /// response to match this schema instead of returning free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct GenerateOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    /// Reasoning effort level for models that support it ("low", "medium", "high").
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

/// Structured response for tag suggestions.
#[derive(Debug, Deserialize)]
struct TagsResponse {
    tags: Vec<String>,
}

/// Structured response for categorization.
#[derive(Debug, Deserialize)]
struct CategoriesResponse {
    categories: Vec<String>,
}

/// Ollama API response for generating text.
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
    #[allow(dead_code)]
    done: bool,
}

/// Ollama API request for embeddings.
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

/// Ollama API response for embeddings.
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

/// Ollama backend implementation.
pub struct OllamaBackend {
    client: reqwest::blocking::Client,
}

impl OllamaBackend {
    /// Create a new Ollama backend with default HTTP client.
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        OllamaBackend { client }
    }

    /// Create a new Ollama backend with custom timeout.
    pub fn with_timeout(timeout_secs: u64) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        OllamaBackend { client }
    }

    /// Get the API base URL from config or default.
    fn get_base_url(config: &AiConfig) -> String {
        config
            .endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    /// Get the model name from config or default.
    fn get_model(config: &AiConfig) -> String {
        config
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string())
    }

    /// Check if Ollama server is running and responsive.
    pub fn check_connection(&self, config: &AiConfig) -> AiResult<()> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/api/tags", base_url);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .map_err(|e| AiError::ConnectionError(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(AiError::ConnectionError(format!(
                "Ollama returned status {}",
                response.status()
            )))
        }
    }

    /// Generate text completion from Ollama.
    ///
    /// `model_override` allows per-task model selection.
    /// `format` provides a JSON schema for structured output.
    fn generate(
        &self,
        prompt: &str,
        config: &AiConfig,
        model_override: Option<&str>,
        format: Option<serde_json::Value>,
    ) -> AiResult<String> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/api/generate", base_url);

        let model = match model_override {
            Some(m) => m.to_string(),
            None => Self::get_model(config),
        };

        let request = GenerateRequest {
            model,
            prompt: prompt.to_string(),
            stream: Some(false),
            options: Some(GenerateOptions {
                temperature: config.temperature,
                num_predict: config.max_tokens.map(|t| t as i32),
                reasoning_effort: config.reasoning_effort.clone(),
            }),
            format,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(config.timeout_secs.unwrap_or(30)))
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout
                } else {
                    AiError::ConnectionError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(AiError::ConnectionError(format!(
                "Ollama returned status {}",
                response.status()
            )));
        }

        let result: GenerateResponse = response
            .json()
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;

        Ok(result.response.trim().to_string())
    }

    /// Build the JSON schema for structured tag output.
    fn tags_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["tags"]
        })
    }

    /// Build the JSON schema for structured categorization output.
    fn categories_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "categories": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["categories"]
        })
    }

    /// Parse tags from AI response.
    /// Handles various formats: comma-separated, newline-separated, JSON arrays.
    fn parse_tags(response: &str) -> Vec<String> {
        // Try to parse as JSON array first
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(response) {
            return tags;
        }

        // Try to extract tags from various text formats
        let cleaned = response
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim();

        // Split by common delimiters
        let tags: Vec<String> = cleaned
            .split([',', '\n', ';'])
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty() && s.len() < 50) // Filter out garbage
            .take(10) // Limit to 10 tags
            .collect();

        tags
    }
}

impl Default for OllamaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AiBackend for OllamaBackend {
    fn is_available(&self) -> bool {
        let config = AiConfig::default();
        self.check_connection(&config).is_ok()
    }

    fn suggest_tags(&self, text: &str, config: &AiConfig) -> AiResult<Vec<String>> {
        // Truncate text if too long (rough token estimate: ~4 chars per token)
        let truncated = truncate_str(text, 8000);
        let model = config.model_for_task("tagging");

        let prompt = format!(
            r#"Analyze the following text and suggest 3-7 relevant short lowercase tags (1-2 words each).

Text:
{}"#,
            truncated
        );

        // Try structured output first
        let response = self.generate(&prompt, config, Some(&model), Some(Self::tags_schema()))?;

        if let Ok(parsed) = serde_json::from_str::<TagsResponse>(&response) {
            if !parsed.tags.is_empty() {
                return Ok(parsed.tags);
            }
        }

        // Fallback: try without structured output for older Ollama versions
        let fallback_prompt = format!(
            r#"Analyze the following text and suggest relevant tags.
Return ONLY a JSON array of 3-7 short lowercase tags (1-2 words each).
Do not include any explanation or additional text.

Text:
{}

Tags:"#,
            truncated
        );

        let fallback_response = self.generate(&fallback_prompt, config, Some(&model), None)?;
        Ok(Self::parse_tags(&fallback_response))
    }

    fn summarize(&self, text: &str, config: &AiConfig) -> AiResult<String> {
        // Truncate text if too long
        let truncated = truncate_str(text, 12000);
        let model = config.model_for_task("summarize");

        let prompt = format!(
            r#"Summarize the following text in 1-2 sentences.
Be concise and capture the main points.

Text:
{}

Summary:"#,
            truncated
        );

        self.generate(&prompt, config, Some(&model), None)
    }

    fn embed(&self, text: &str, config: &AiConfig) -> AiResult<Vec<f32>> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/api/embeddings", base_url);

        // Truncate text if too long
        let truncated = truncate_str(text, 8000);

        let request = EmbeddingRequest {
            model: config.model_for_task("embedding"),
            prompt: truncated.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(config.timeout_secs.unwrap_or(30)))
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout
                } else {
                    AiError::ConnectionError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(AiError::ConnectionError(format!(
                "Ollama returned status {}",
                response.status()
            )));
        }

        let result: EmbeddingResponse = response
            .json()
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;

        Ok(result.embedding)
    }

    fn categorize(&self, texts: &[&str], config: &AiConfig) -> AiResult<Vec<String>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let model = config.model_for_task("categorize");

        // Build a prompt that asks for categories
        let text_list: String = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let truncated = truncate_str(t, 200);
                format!("{}. {}\n", i + 1, truncated)
            })
            .collect();

        let prompt = format!(
            r#"Categorize each text into one of these categories: work, personal, reference, idea, task, other.
Return one category per text in the categories array.

Texts:
{}"#,
            text_list
        );

        // Try structured output first
        let response = self.generate(
            &prompt,
            config,
            Some(&model),
            Some(Self::categories_schema()),
        )?;

        if let Ok(parsed) = serde_json::from_str::<CategoriesResponse>(&response) {
            if parsed.categories.len() == texts.len() {
                return Ok(parsed.categories);
            }
        }

        // Fallback: try without structured output for older Ollama versions
        let fallback_prompt = format!(
            r#"Categorize each text into one of these categories: work, personal, reference, idea, task, other.
Return ONLY a JSON array of category strings, one for each text.

Texts:
{}

Categories:"#,
            text_list
        );

        let fallback_response = self.generate(&fallback_prompt, config, Some(&model), None)?;

        // Parse categories from response
        if let Ok(categories) = serde_json::from_str::<Vec<String>>(&fallback_response) {
            if categories.len() == texts.len() {
                return Ok(categories);
            }
        }

        // Fallback: try to extract categories line by line
        let categories: Vec<String> = fallback_response
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(
                        trimmed
                            .trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == ' ')
                            .to_lowercase(),
                    )
                }
            })
            .take(texts.len())
            .collect();

        if categories.len() == texts.len() {
            Ok(categories)
        } else {
            // Final fallback: return "other" for all
            Ok(texts.iter().map(|_| "other".to_string()).collect())
        }
    }

    fn simplify(&self, text: &str, config: &AiConfig) -> AiResult<String> {
        let truncated = truncate_str(text, 12000);
        let prompt = format!(
            "Rewrite the following text in plain, simple language. Use short sentences and common words. Keep the same meaning but make it easier to understand:\n\n{}",
            truncated
        );
        self.generate(
            &prompt,
            config,
            Some(&config.model_for_task("simplify")),
            None,
        )
    }

    fn key_points(&self, text: &str, config: &AiConfig) -> AiResult<Vec<String>> {
        let truncated = truncate_str(text, 12000);
        let prompt = format!(
            "Extract the key points from the following text. Return each key point on its own line, prefixed with a dash (-):\n\n{}",
            truncated
        );
        let response = self.generate(
            &prompt,
            config,
            Some(&config.model_for_task("key_points")),
            None,
        )?;
        Ok(Self::parse_bullet_points(&response))
    }

    fn backend_name(&self) -> &str {
        "ollama"
    }
}

impl OllamaBackend {
    /// Parse bullet points from AI response.
    fn parse_bullet_points(response: &str) -> Vec<String> {
        response
            .lines()
            .map(|line| {
                line.trim()
                    .trim_start_matches(['-', '*', '\u{2022}'])
                    .trim_start_matches(|c: char| c.is_numeric())
                    .trim_start_matches(['.', ')', ':'])
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tags_json_array() {
        let response = r#"["rust", "programming", "tutorial"]"#;
        let tags = OllamaBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming", "tutorial"]);
    }

    #[test]
    fn test_parse_tags_comma_separated() {
        let response = "rust, programming, tutorial";
        let tags = OllamaBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming", "tutorial"]);
    }

    #[test]
    fn test_parse_tags_newline_separated() {
        let response = "rust\nprogramming\ntutorial";
        let tags = OllamaBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming", "tutorial"]);
    }

    #[test]
    fn test_parse_tags_filters_empty() {
        let response = "rust, , programming,";
        let tags = OllamaBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming"]);
    }

    #[test]
    fn test_backend_creation() {
        let backend = OllamaBackend::new();
        assert_eq!(backend.backend_name(), "ollama");
    }

    #[test]
    fn test_tags_schema_structure() {
        let schema = OllamaBackend::tags_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["tags"]["type"], "array");
        assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "tags");
    }

    #[test]
    fn test_categories_schema_structure() {
        let schema = OllamaBackend::categories_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["categories"]["type"], "array");
        assert_eq!(
            schema["properties"]["categories"]["items"]["type"],
            "string"
        );
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "categories");
    }

    #[test]
    fn test_structured_tags_response_parsing() {
        let json = r#"{"tags": ["rust", "programming", "web"]}"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tags, vec!["rust", "programming", "web"]);
    }

    #[test]
    fn test_structured_categories_response_parsing() {
        let json = r#"{"categories": ["work", "personal", "other"]}"#;
        let parsed: CategoriesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.categories, vec!["work", "personal", "other"]);
    }

    #[test]
    fn test_generate_request_serialization_with_format() {
        let request = GenerateRequest {
            model: "llama3.2".to_string(),
            prompt: "test".to_string(),
            stream: Some(false),
            options: Some(GenerateOptions {
                temperature: Some(0.3),
                num_predict: Some(256),
                reasoning_effort: Some("medium".to_string()),
            }),
            format: Some(OllamaBackend::tags_schema()),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json["format"].is_object());
        assert_eq!(json["options"]["reasoning_effort"], "medium");
    }

    #[test]
    fn test_generate_request_serialization_without_format() {
        let request = GenerateRequest {
            model: "llama3.2".to_string(),
            prompt: "test".to_string(),
            stream: Some(false),
            options: Some(GenerateOptions {
                temperature: Some(0.3),
                num_predict: Some(256),
                reasoning_effort: None,
            }),
            format: None,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("format").is_none());
        assert!(json["options"].get("reasoning_effort").is_none());
    }

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn test_parse_bullet_points_dashes() {
        let response = "- First point\n- Second point\n- Third point";
        let points = OllamaBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["First point", "Second point", "Third point"]);
    }

    #[test]
    fn test_parse_bullet_points_numbered() {
        let response = "1. First\n2. Second\n3. Third";
        let points = OllamaBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn test_parse_bullet_points_mixed() {
        let response = "* Point one\n- Point two\n3) Point three";
        let points = OllamaBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["Point one", "Point two", "Point three"]);
    }

    #[test]
    fn test_parse_bullet_points_empty_lines() {
        let response = "- First\n\n- Second\n  \n- Third";
        let points = OllamaBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn test_truncate_str_multibyte() {
        // Each CJK character is 3 bytes in UTF-8
        let text = "\u{4f60}\u{597d}\u{4e16}\u{754c}"; // 12 bytes total
        let result = truncate_str(text, 7);
        // Should cut at char boundary: 6 bytes = 2 chars
        assert_eq!(result, "\u{4f60}\u{597d}");
    }
}
