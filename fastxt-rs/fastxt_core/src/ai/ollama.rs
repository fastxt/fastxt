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

/// Ollama API request for generating text.
#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<GenerateOptions>,
}

#[derive(Debug, Serialize)]
struct GenerateOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
}

/// Ollama API response for generating text.
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
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
    fn generate(&self, prompt: &str, config: &AiConfig) -> AiResult<String> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/api/generate", base_url);

        let request = GenerateRequest {
            model: Self::get_model(config),
            prompt: prompt.to_string(),
            stream: Some(false),
            options: Some(GenerateOptions {
                temperature: config.temperature,
                num_predict: config.max_tokens.map(|t| t as i32),
            }),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(
                config.timeout_secs.unwrap_or(30),
            ))
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
            .split(|c| c == ',' || c == '\n' || c == ';')
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
        let max_chars = 8000;
        let truncated = if text.len() > max_chars {
            &text[..max_chars]
        } else {
            text
        };

        let prompt = format!(
            r#"Analyze the following text and suggest relevant tags.
Return ONLY a JSON array of 3-7 short lowercase tags (1-2 words each).
Do not include any explanation or additional text.

Text:
{}

Tags:"#,
            truncated
        );

        let response = self.generate(&prompt, config)?;
        Ok(Self::parse_tags(&response))
    }

    fn summarize(&self, text: &str, config: &AiConfig) -> AiResult<String> {
        // Truncate text if too long
        let max_chars = 12000;
        let truncated = if text.len() > max_chars {
            &text[..max_chars]
        } else {
            text
        };

        let prompt = format!(
            r#"Summarize the following text in 1-2 sentences.
Be concise and capture the main points.

Text:
{}

Summary:"#,
            truncated
        );

        self.generate(&prompt, config)
    }

    fn embed(&self, text: &str, config: &AiConfig) -> AiResult<Vec<f32>> {
        let base_url = Self::get_base_url(config);
        let url = format!("{}/api/embeddings", base_url);

        // Truncate text if too long
        let max_chars = 8000;
        let truncated = if text.len() > max_chars {
            &text[..max_chars]
        } else {
            text
        };

        let request = EmbeddingRequest {
            model: Self::get_model(config),
            prompt: truncated.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(
                config.timeout_secs.unwrap_or(30),
            ))
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

        // Build a prompt that asks for categories
        let text_list: String = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let truncated = if t.len() > 200 { &t[..200] } else { t };
                format!("{}. {}\n", i + 1, truncated)
            })
            .collect();

        let prompt = format!(
            r#"Categorize each text into one of these categories: work, personal, reference, idea, task, other.
Return ONLY a JSON array of category strings, one for each text.

Texts:
{}

Categories:"#,
            text_list
        );

        let response = self.generate(&prompt, config)?;

        // Parse categories from response
        if let Ok(categories) = serde_json::from_str::<Vec<String>>(&response) {
            if categories.len() == texts.len() {
                return Ok(categories);
            }
        }

        // Fallback: try to extract categories line by line
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
            // Final fallback: return "other" for all
            Ok(texts.iter().map(|_| "other".to_string()).collect())
        }
    }

    fn backend_name(&self) -> &str {
        "ollama"
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
}
