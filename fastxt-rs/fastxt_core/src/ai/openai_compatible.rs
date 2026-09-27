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

//! One transport for every OpenAI-compatible local server: llama.cpp's
//! `llama-server`, Microsoft Foundry Local, LM Studio, vLLM, … The only
//! differences are the default port and the health endpoint.

use super::{AiBackend, AiCheck, GenerateRequest};
use crate::model::AiSettings;
use serde::Deserialize;
use std::time::Duration;

/// Per-flavour defaults: (name, default endpoint, health path).
fn flavour(backend: &str) -> (&'static str, &'static str, &'static str) {
    match backend {
        "llamacpp" => ("llama.cpp", "http://localhost:8080", "/health"),
        "foundry-local" => ("Foundry Local", "http://localhost:5272", "/health"),
        _ => ("OpenAI-compatible server", "", "/v1/models"),
    }
}

/// OpenAI-compatible transport, parameterised by backend name.
pub struct OpenAiCompatibleBackend {
    backend: &'static str,
}

impl OpenAiCompatibleBackend {
    #[must_use]
    pub fn new(backend: &str) -> Self {
        OpenAiCompatibleBackend {
            backend: match backend {
                "llamacpp" => "llamacpp",
                "foundry-local" => "foundry-local",
                _ => "openai",
            },
        }
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

fn endpoint(settings: &AiSettings, backend: &str) -> Result<String, String> {
    let (_, default, _) = flavour(backend);
    let e = settings.endpoint.trim().trim_end_matches('/');
    if e.is_empty() {
        if default.is_empty() {
            return Err("this backend needs an explicit endpoint in AI settings".into());
        }
        return Ok(default.to_string());
    }
    Ok(e.to_string())
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default()
}

impl AiBackend for OpenAiCompatibleBackend {
    fn name(&self) -> &'static str {
        self.backend
    }

    fn check(&self, settings: &AiSettings) -> AiCheck {
        let base = match endpoint(settings, self.backend) {
            Ok(b) => b,
            Err(e) => return AiCheck::fail(e),
        };
        let (label, _, health) = flavour(self.backend);
        let url = format!("{base}{health}");
        let Ok(response) = client().get(&url).timeout(Duration::from_secs(3)).send() else {
            return AiCheck::fail(format!(
                "{label} is not reachable at {base}. Start it first."
            ));
        };
        if !response.status().is_success() {
            return AiCheck::fail(format!("{label} at {base} answered {}", response.status()));
        }
        AiCheck::ok(format!("Connected to {label} at {base}"))
    }

    fn generate(
        &self,
        request: &GenerateRequest<'_>,
        settings: &AiSettings,
    ) -> std::result::Result<String, String> {
        let base = endpoint(settings, self.backend)?;
        let url = format!("{base}/v1/chat/completions");
        let mut messages = Vec::new();
        if let Some(system) = request.system {
            messages.push(serde_json::json!({ "role": "system", "content": system }));
        }
        // JSON mode plus the schema in the prompt: servers implement
        // `response_format` inconsistently, but all follow the prompt.
        let content = if let Some(schema) = request.json_schema {
            format!(
                "{}\n\nReply with JSON matching: {}",
                request.prompt,
                serde_json::to_string(schema).unwrap_or_default()
            )
        } else {
            request.prompt.to_string()
        };
        messages.push(serde_json::json!({ "role": "user", "content": content }));
        let mut body = serde_json::json!({
            "messages": messages,
            "temperature": 0.3,
            "max_tokens": request.max_tokens,
            "stream": false,
        });
        if !settings.model.is_empty() {
            body["model"] = serde_json::json!(settings.model);
        }
        if request.json_schema.is_some() {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        let response = client()
            .post(&url)
            .json(&body)
            .timeout(Duration::from_secs(settings.timeout_secs.max(30)))
            .send()
            .map_err(|e| format!("request to {url} failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("server answered {}", response.status()));
        }
        let parsed: ChatResponse = response
            .json()
            .map_err(|e| format!("unreadable response: {e}"))?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| "no choices in response".to_string())
    }

    fn embed(
        &self,
        text: &str,
        model: &str,
        settings: &AiSettings,
    ) -> std::result::Result<Vec<f32>, String> {
        let base = endpoint(settings, self.backend)?;
        let url = format!("{base}/v1/embeddings");
        let mut body = serde_json::json!({ "input": text });
        if !model.is_empty() {
            body["model"] = serde_json::json!(model);
        }
        let response = client()
            .post(&url)
            .json(&body)
            .timeout(Duration::from_secs(settings.timeout_secs.max(30)))
            .send()
            .map_err(|e| format!("request to {url} failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("server answered {}", response.status()));
        }
        let parsed: EmbeddingsResponse = response
            .json()
            .map_err(|e| format!("unreadable response: {e}"))?;
        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| "no embedding in response".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_requires_an_explicit_endpoint() {
        let s = AiSettings {
            backend: "openai".into(),
            endpoint: String::new(),
            ..AiSettings::default()
        };
        let check = OpenAiCompatibleBackend::new("openai").check(&s);
        assert!(!check.ok);
        assert!(check.message.contains("endpoint"), "{}", check.message);
    }

    #[test]
    fn llamacpp_defaults_its_endpoint() {
        let check = OpenAiCompatibleBackend::new("llamacpp").check(&AiSettings::default());
        assert!(!check.ok, "nothing listens on :8080 in CI");
        assert!(check.message.contains("8080"), "{}", check.message);
    }
}
