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

//! A deterministic backend for tests and previews: no network, no model.

use super::{AiBackend, AiCheck, GenerateRequest};
use crate::model::AiSettings;

/// Answers the orchestrator's prompts with predictable, parseable output.
pub struct MockBackend;

/// Answers "unavailable" to every probe (the no-`ai`-feature backend).
pub struct UnavailableBackend;

fn hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

impl AiBackend for MockBackend {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn check(&self, _settings: &AiSettings) -> AiCheck {
        AiCheck::ok("mock backend")
    }

    fn generate(
        &self,
        request: &GenerateRequest<'_>,
        _settings: &AiSettings,
    ) -> std::result::Result<String, String> {
        let prompt = request.prompt;
        if prompt.contains("tags for") || (prompt.contains("Suggest") && prompt.contains("tags")) {
            return Ok(r#"{"tags": ["mock", "notes"]}"#.into());
        }
        if prompt.contains("categories") || prompt.contains("categor") {
            // The prompt numbers the notes "1. …"; answer one category each.
            let count = prompt
                .lines()
                .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()) && l.contains(". "))
                .count();
            let categories: Vec<String> = (0..count)
                .map(|i| if i % 2 == 0 { "work" } else { "other" }.to_string())
                .collect();
            return Ok(serde_json::json!({ "categories": categories }).to_string());
        }
        if prompt.contains("Summarize") || prompt.contains("summar") {
            return Ok("A mock summary of the note.".into());
        }
        Ok("mock generation".into())
    }

    fn embed(
        &self,
        text: &str,
        _model: &str,
        _settings: &AiSettings,
    ) -> std::result::Result<Vec<f32>, String> {
        // Deterministic 8-dimensional unit vector from the text.
        let h = hash(text);
        let mut v: Vec<f32> = (0..8).map(|i| ((h >> (i * 8)) & 0xff) as f32).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Ok(v)
    }
}

impl AiBackend for UnavailableBackend {
    fn name(&self) -> &'static str {
        "unavailable"
    }

    fn check(&self, _settings: &AiSettings) -> AiCheck {
        AiCheck::fail("this build has no AI backends (rebuild with --features ai)")
    }

    fn generate(
        &self,
        _request: &GenerateRequest<'_>,
        _settings: &AiSettings,
    ) -> std::result::Result<String, String> {
        Err("no AI backends in this build".into())
    }

    fn embed(
        &self,
        _text: &str,
        _model: &str,
        _settings: &AiSettings,
    ) -> std::result::Result<Vec<f32>, String> {
        Err("no AI backends in this build".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_answers_every_prompt_shape() {
        let s = AiSettings::default();
        let b = MockBackend;
        assert_eq!(
            b.generate(
                &GenerateRequest {
                    prompt: "Suggest 3 to 7 tags for the note below.\nNote:\nrust note",
                    system: None,
                    json_schema: None,
                    model: "m",
                    max_tokens: 64,
                },
                &s
            )
            .unwrap(),
            r#"{"tags": ["mock", "notes"]}"#
        );
        let summary = b
            .generate(
                &GenerateRequest {
                    prompt: "Summarize this note in 1-2 plain sentences:\n\nhello",
                    system: None,
                    json_schema: None,
                    model: "m",
                    max_tokens: 64,
                },
                &s,
            )
            .unwrap();
        assert!(summary.contains("mock summary"));
    }

    #[test]
    fn mock_embeddings_are_deterministic_unit_vectors() {
        let s = AiSettings::default();
        let a = MockBackend.embed("same text", "m", &s).unwrap();
        let b = MockBackend.embed("same text", "m", &s).unwrap();
        let c = MockBackend.embed("other text", "m", &s).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn unavailable_backend_refuses_everything() {
        let s = AiSettings::default();
        assert!(!UnavailableBackend.check(&s).ok);
        assert!(UnavailableBackend.embed("x", "m", &s).is_err());
    }
}
