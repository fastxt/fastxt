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

//! Ollama transport (https://ollama.com), talking to a local server over HTTP.

use super::{AiBackend, AiCheck, GenerateRequest};
use crate::model::AiSettings;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "http://localhost:11434";

/// The Ollama backend. Stateless; one shared blocking HTTP client.
pub struct OllamaBackend;

fn endpoint(settings: &AiSettings) -> String {
    let e = settings.endpoint.trim();
    if e.is_empty() {
        DEFAULT_ENDPOINT.to_string()
    } else {
        e.trim_end_matches('/').to_string()
    }
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagsModel>,
}

#[derive(Deserialize)]
struct TagsModel {
    name: String,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    embedding: Option<Vec<f32>>,
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default()
}

fn post_json(
    url: &str,
    body: &Value,
    timeout: Duration,
) -> std::result::Result<reqwest::blocking::Response, reqwest::Error> {
    client().post(url).json(body).timeout(timeout).send()
}

impl AiBackend for OllamaBackend {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn check(&self, settings: &AiSettings) -> AiCheck {
        let url = format!("{}/api/tags", endpoint(settings));
        let Ok(response) = client().get(&url).timeout(Duration::from_secs(3)).send() else {
            return AiCheck::fail(format!(
                "Ollama is not reachable at {url}. Is it installed and running?"
            ));
        };
        if !response.status().is_success() {
            return AiCheck::fail(format!("Ollama at {url} answered {}", response.status()));
        }
        let listed = response
            .json::<TagsResponse>()
            .map(|r| r.models.into_iter().map(|m| m.name).collect::<Vec<_>>())
            .unwrap_or_default();
        if listed.is_empty() {
            return AiCheck::ok(format!("Connected to {url}; no models pulled yet"));
        }
        // Model names may carry a ":tag" suffix; match on the base name.
        let has = |model: &str| {
            let base = model.split(':').next().unwrap_or(model);
            listed
                .iter()
                .any(|n| n == model || n.split(':').next() == Some(base))
        };
        let mut missing = Vec::new();
        if !has(&settings.model) {
            missing.push(format!("ollama pull {}", settings.model));
        }
        if !has(&settings.embedding_model) {
            missing.push(format!("ollama pull {}", settings.embedding_model));
        }
        if missing.is_empty() {
            AiCheck::ok(format!("Connected to {url}"))
        } else {
            AiCheck::ok(format!(
                "Connected to {url}; missing models: {}",
                missing.join(", ")
            ))
        }
    }

    fn generate(
        &self,
        request: &GenerateRequest<'_>,
        settings: &AiSettings,
    ) -> std::result::Result<String, String> {
        let url = format!("{}/api/generate", endpoint(settings));
        let mut body = serde_json::json!({
            "model": request.model,
            "prompt": request.prompt,
            "stream": false,
            "options": { "temperature": 0.3, "num_predict": request.max_tokens },
        });
        if let Some(system) = request.system {
            body["system"] = Value::String(system.to_string());
        }
        if let Some(schema) = request.json_schema {
            body["format"] = schema.clone();
        }
        let response = post_json(
            &url,
            &body,
            Duration::from_secs(settings.timeout_secs.max(30)),
        )
        .map_err(|e| format!("request to {url} failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("Ollama answered {}", response.status()));
        }
        response
            .json::<GenerateResponse>()
            .map(|r| r.response.trim().to_string())
            .map_err(|e| format!("unreadable response: {e}"))
    }

    fn embed(
        &self,
        text: &str,
        model: &str,
        settings: &AiSettings,
    ) -> std::result::Result<Vec<f32>, String> {
        let base = endpoint(settings);
        let timeout = Duration::from_secs(settings.timeout_secs.max(30));
        // Current API: /api/embed. Older servers only have /api/embeddings.
        let url = format!("{base}/api/embed");
        let body = serde_json::json!({ "model": model, "input": text });
        let response =
            post_json(&url, &body, timeout).map_err(|e| format!("request to {url} failed: {e}"))?;
        if response.status().as_u16() == 404 {
            let url = format!("{base}/api/embeddings");
            let body = serde_json::json!({ "model": model, "prompt": text });
            let response = post_json(&url, &body, timeout)
                .map_err(|e| format!("request to {url} failed: {e}"))?;
            if !response.status().is_success() {
                return Err(format!("Ollama answered {}", response.status()));
            }
            return response
                .json::<EmbedResponse>()
                .map_err(|e| format!("unreadable response: {e}"))
                .and_then(|r| r.embedding.ok_or_else(|| "no embedding in response".into()));
        }
        if !response.status().is_success() {
            return Err(format!("Ollama answered {}", response.status()));
        }
        let parsed: EmbedResponse = response
            .json()
            .map_err(|e| format!("unreadable response: {e}"))?;
        parsed
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "no embedding in response".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_defaults_and_trims_trailing_slash() {
        let mut s = AiSettings::default();
        assert_eq!(endpoint(&s), DEFAULT_ENDPOINT);
        s.endpoint = "http://192.168.1.4:11434/".into();
        assert_eq!(endpoint(&s), "http://192.168.1.4:11434");
    }

    #[test]
    fn check_reports_an_unreachable_endpoint() {
        // Port 1 on localhost refuses connections in every CI environment.
        let s = AiSettings {
            endpoint: "http://127.0.0.1:1".into(),
            ..AiSettings::default()
        };
        let check = OllamaBackend.check(&s);
        assert!(!check.ok);
        assert!(check.message.contains("127.0.0.1:1"), "{}", check.message);
    }
}
