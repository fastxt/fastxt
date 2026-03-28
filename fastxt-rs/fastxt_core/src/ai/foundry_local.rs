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

//! Microsoft Foundry Local backend for on-device AI via OpenAI-compatible HTTP API.
//!
//! This backend connects to a locally running Foundry Local server
//! (default: localhost:5272) which exposes an OpenAI-compatible API for
//! chat completions and embeddings, optimized for Windows NPU inference.
//!
//! # Setup
//! 1. Install Foundry Local: https://github.com/microsoft/foundry-local
//! 2. Start the service (it listens on port 5272 by default)
//! 3. Models are managed through the Foundry Local catalog

use super::ollama::truncate_str;
use super::{AiBackend, AiConfig, AiError, AiResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// OpenAI-compatible chat message.
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// OpenAI-compatible chat completion request.
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// OpenAI-compatible chat completion response.
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

/// OpenAI-compatible embedding request.
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

/// OpenAI-compatible embedding response.
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// Foundry Local server health response.
#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
}

/// Microsoft Foundry Local backend implementation.
///
/// Foundry Local provides NPU-optimized inference on Windows devices,
/// exposing an OpenAI-compatible HTTP API on port 5272.
pub struct FoundryLocalBackend {
    client: reqwest::blocking::Client,
}

impl FoundryLocalBackend {
    /// Create a new Foundry Local backend with default HTTP client.
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        FoundryLocalBackend { client }
    }

    /// Create a new Foundry Local backend with custom timeout.
    pub fn with_timeout(timeout_secs: u64) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        FoundryLocalBackend { client }
    }

    /// Get the API base URL from config or default (Foundry Local default: port 5272).
    fn get_base_url(config: &AiConfig) -> String {
        config
            .endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:5272".to_string())
    }

    /// Get the model name from config.
    fn get_model(config: &AiConfig) -> Option<String> {
        config.model.clone()
    }

    /// Check if Foundry Local server is running and healthy.
    pub fn check_health(&self, config: &AiConfig) -> AiResult<()> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/health", base_url);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .map_err(|e| AiError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AiError::ConnectionError(format!(
                "Foundry Local returned status {}",
                response.status()
            )));
        }

        let health: HealthResponse = response
            .json()
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;

        if health.status == "ok" {
            Ok(())
        } else {
            Err(AiError::ConnectionError(format!(
                "Foundry Local status: {}",
                health.status
            )))
        }
    }

    /// Send a chat completion request to Foundry Local.
    fn chat_complete(&self, system: &str, user: &str, config: &AiConfig) -> AiResult<String> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/v1/chat/completions", base_url);

        let request = ChatCompletionRequest {
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user.to_string(),
                },
            ],
            model: Self::get_model(config),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
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
                "Foundry Local returned status {}",
                response.status()
            )));
        }

        let result: ChatCompletionResponse = response
            .json()
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;

        result
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| AiError::InvalidResponse("No choices in response".to_string()))
    }

    /// Parse tags from AI response.
    /// Handles various formats: comma-separated, newline-separated, JSON arrays.
    fn parse_tags(response: &str) -> Vec<String> {
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(response) {
            return tags;
        }

        let cleaned = response
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim();

        cleaned
            .split([',', '\n', ';'])
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty() && s.len() < 50)
            .take(10)
            .collect()
    }

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

impl Default for FoundryLocalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AiBackend for FoundryLocalBackend {
    fn is_available(&self) -> bool {
        let config = AiConfig {
            endpoint: Some("http://localhost:5272".to_string()),
            ..Default::default()
        };
        self.check_health(&config).is_ok()
    }

    fn suggest_tags(&self, text: &str, config: &AiConfig) -> AiResult<Vec<String>> {
        let truncated = truncate_str(text, 8000);

        let system = "You are a tagging assistant. Return ONLY a JSON array of 3-7 short lowercase tags (1-2 words each). No explanation.";
        let user = format!("Suggest tags for this text:\n\n{}", truncated);

        let response = self.chat_complete(system, &user, config)?;
        Ok(Self::parse_tags(&response))
    }

    fn summarize(&self, text: &str, config: &AiConfig) -> AiResult<String> {
        let truncated = truncate_str(text, 12000);

        let system = "You are a summarization assistant. Provide a concise 1-2 sentence summary.";
        let user = format!("Summarize this text:\n\n{}", truncated);

        self.chat_complete(system, &user, config)
    }

    fn embed(&self, text: &str, config: &AiConfig) -> AiResult<Vec<f32>> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/v1/embeddings", base_url);

        let truncated = truncate_str(text, 8000);

        let request = EmbeddingRequest {
            input: truncated.to_string(),
            model: Self::get_model(config),
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
                "Foundry Local returned status {}",
                response.status()
            )));
        }

        let result: EmbeddingResponse = response
            .json()
            .map_err(|e| AiError::InvalidResponse(e.to_string()))?;

        result
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| AiError::InvalidResponse("No embedding data in response".to_string()))
    }

    fn categorize(&self, texts: &[&str], config: &AiConfig) -> AiResult<Vec<String>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let text_list: String = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let truncated = truncate_str(t, 200);
                format!("{}. {}\n", i + 1, truncated)
            })
            .collect();

        let system = "You are a categorization assistant. Categorize each text into one of: work, personal, reference, idea, task, other. Return ONLY a JSON array of category strings.";
        let user = format!("Categorize these texts:\n\n{}", text_list);

        let response = self.chat_complete(system, &user, config)?;

        if let Ok(categories) = serde_json::from_str::<Vec<String>>(&response)
            && categories.len() == texts.len()
        {
            return Ok(categories);
        }

        // Fallback
        let categories: Vec<String> = response
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
            Ok(texts.iter().map(|_| "other".to_string()).collect())
        }
    }

    fn simplify(&self, text: &str, config: &AiConfig) -> AiResult<String> {
        let truncated = truncate_str(text, 12000);

        let system = "You are a plain language assistant. Rewrite text using short sentences and common words so it is easy for everyone to understand. Keep the same meaning.";
        let user = format!("Simplify this text:\n\n{}", truncated);

        self.chat_complete(system, &user, config)
    }

    fn key_points(&self, text: &str, config: &AiConfig) -> AiResult<Vec<String>> {
        let truncated = truncate_str(text, 12000);

        let system = "You are a key points extractor. Return each key point on its own line prefixed with \"- \". Keep each point to one sentence.";
        let user = format!("Extract key points from this text:\n\n{}", truncated);

        let response = self.chat_complete(system, &user, config)?;
        Ok(Self::parse_bullet_points(&response))
    }

    fn backend_name(&self) -> &str {
        "foundry-local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = FoundryLocalBackend::new();
        assert_eq!(backend.backend_name(), "foundry-local");
    }

    #[test]
    fn test_backend_default() {
        let backend = FoundryLocalBackend::default();
        assert_eq!(backend.backend_name(), "foundry-local");
    }

    #[test]
    fn test_get_base_url_default() {
        let config = AiConfig {
            endpoint: None,
            ..Default::default()
        };
        assert_eq!(
            FoundryLocalBackend::get_base_url(&config),
            "http://localhost:5272"
        );
    }

    #[test]
    fn test_get_base_url_custom() {
        let config = AiConfig {
            endpoint: Some("http://192.168.1.100:9090".to_string()),
            ..Default::default()
        };
        assert_eq!(
            FoundryLocalBackend::get_base_url(&config),
            "http://192.168.1.100:9090"
        );
    }

    #[test]
    fn test_parse_tags_json_array() {
        let response = r#"["rust", "programming", "tutorial"]"#;
        let tags = FoundryLocalBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming", "tutorial"]);
    }

    #[test]
    fn test_parse_tags_comma_separated() {
        let response = "rust, programming, tutorial";
        let tags = FoundryLocalBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming", "tutorial"]);
    }

    #[test]
    fn test_parse_tags_filters_empty() {
        let response = "rust, , programming,";
        let tags = FoundryLocalBackend::parse_tags(response);
        assert_eq!(tags, vec!["rust", "programming"]);
    }

    #[test]
    fn test_parse_bullet_points() {
        let response = "- First point\n- Second point\n- Third point";
        let points = FoundryLocalBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["First point", "Second point", "Third point"]);
    }

    #[test]
    fn test_parse_bullet_points_numbered() {
        let response = "1. First\n2. Second\n3. Third";
        let points = FoundryLocalBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn test_parse_bullet_points_mixed() {
        let response = "* Point one\n- Point two\n3) Point three";
        let points = FoundryLocalBackend::parse_bullet_points(response);
        assert_eq!(points, vec!["Point one", "Point two", "Point three"]);
    }

    #[test]
    fn test_with_timeout() {
        let backend = FoundryLocalBackend::with_timeout(120);
        assert_eq!(backend.backend_name(), "foundry-local");
    }
}
