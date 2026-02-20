use anyhow::Result;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

use crate::config::LlmConfig;
use super::provider::{create_provider, extract_json, LlmProvider};

/// Structured response from the LLM for image description and tagging
#[derive(Debug, Deserialize)]
pub struct ImageDescription {
    pub description: String,
    pub tags: Vec<String>,
}

/// LLM client that wraps a provider implementation
pub struct LlmClient {
    provider: Arc<dyn LlmProvider>,
}

impl LlmClient {
    /// Create a new LlmClient from configuration
    pub fn from_config(config: &LlmConfig) -> Self {
        let provider = create_provider(config);

        Self {
            provider: Arc::from(provider),
        }
    }

    /// Describe an image and generate tags in a single LLM call.
    ///
    /// Uses a three-tier parsing strategy:
    /// 1. Direct JSON parse of the response
    /// 2. Extract JSON from markdown code blocks, then parse
    /// 3. Fall back to TAGS: delimiter parsing (legacy format)
    pub fn describe_and_tag_image(&self, image_path: &Path) -> Result<(String, Vec<String>)> {
        let response = self.provider.describe_image(image_path)?;

        // Tier 1: Try direct JSON parse
        if let Ok(parsed) = serde_json::from_str::<ImageDescription>(&response) {
            return Ok((parsed.description, parsed.tags));
        }

        // Tier 2: Try extracting JSON from code blocks
        let extracted = extract_json(&response);
        if extracted != response.trim() {
            if let Ok(parsed) = serde_json::from_str::<ImageDescription>(&extracted) {
                tracing::warn!("LLM response required code block extraction to parse JSON");
                return Ok((parsed.description, parsed.tags));
            }
        }

        // Tier 3: Fall back to TAGS: delimiter parsing
        tracing::warn!("LLM response is not valid JSON, falling back to TAGS: delimiter parsing");
        Self::parse_tags_delimiter(&response)
    }

    /// Legacy TAGS: delimiter parsing for non-JSON responses
    fn parse_tags_delimiter(response: &str) -> Result<(String, Vec<String>)> {
        let tags_pos = response.lines().enumerate().find_map(|(_, line)| {
            let trimmed = line.trim().trim_start_matches('*');
            if trimmed.len() >= 5 && trimmed[..5].eq_ignore_ascii_case("tags:") {
                let line_start = line.as_ptr() as usize - response.as_ptr() as usize;
                let prefix_offset = line.len() - trimmed.len();
                if let Some(colon) = trimmed.find(':') {
                    Some((line_start, prefix_offset + colon + 1))
                } else {
                    None
                }
            } else {
                None
            }
        });

        if let Some((line_start, tags_content_offset)) = tags_pos {
            let description = response[..line_start].trim().to_string();
            let tags_str = response[line_start + tags_content_offset..].trim().trim_end_matches('*');
            let tags: Vec<String> = tags_str
                .split(',')
                .map(|t| t.trim().trim_matches('*').to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            Ok((description, tags))
        } else {
            Ok((response.to_string(), Vec::new()))
        }
    }

    /// Get text embedding for semantic search
    pub fn get_text_embedding(&self, text: &str) -> Result<Vec<f32>> {
        self.provider.get_text_embedding(text)
    }

    /// Check if the provider supports embeddings
    pub fn supports_embeddings(&self) -> bool {
        self.provider.supports_embeddings()
    }
}

impl Clone for LlmClient {
    fn clone(&self) -> Self {
        Self {
            provider: Arc::clone(&self.provider),
        }
    }
}
