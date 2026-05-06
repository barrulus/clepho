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
    /// Uses a three-tier parsing strategy via `parse_describe_and_tag_response`:
    /// 1. Direct JSON parse of the response
    /// 2. Extract JSON from markdown code blocks, then parse
    /// 3. Fall back to TAGS: delimiter parsing (legacy format)
    pub fn describe_and_tag_image(&self, image_path: &Path) -> Result<(String, Vec<String>)> {
        let response = self.provider.describe_image(image_path)?;
        Ok(parse_describe_and_tag_response(&response))
    }

    /// Like `describe_and_tag_image` but lets the caller override the custom
    /// prompt for this single call. Used by the LLM stage so per-folder
    /// prompts can flow through without rebuilding the provider.
    pub fn describe_and_tag_image_with_prompt(
        &self,
        image_path: &Path,
        custom_prompt: Option<&str>,
    ) -> Result<(String, Vec<String>)> {
        let response = self
            .provider
            .describe_image_with_prompt(image_path, custom_prompt)?;
        Ok(parse_describe_and_tag_response(&response))
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

/// Parse a raw LLM response into (description, tags). Three-tier strategy:
///   1. Direct JSON parse against ImageDescription.
///   2. Strip markdown code fences (via provider::extract_json), then parse.
///   3. Legacy TAGS: line — split the description and tags on a TAGS: line,
///      lowercasing tags and trimming markdown bullets/asterisks.
///
/// Malformed input (no JSON, no TAGS line) returns the full response as the
/// description with an empty tags list — callers can decide what to do with
/// that.
pub fn parse_describe_and_tag_response(raw: &str) -> (String, Vec<String>) {
    // Tier 1: direct JSON
    if let Ok(parsed) = serde_json::from_str::<ImageDescription>(raw) {
        return (parsed.description, parsed.tags);
    }

    // Tier 2: code-fenced JSON
    let extracted = extract_json(raw);
    if extracted != raw.trim() {
        if let Ok(parsed) = serde_json::from_str::<ImageDescription>(&extracted) {
            tracing::warn!("LLM response required code block extraction to parse JSON");
            return (parsed.description, parsed.tags);
        }
    }

    // Tier 3: legacy TAGS: delimiter
    tracing::warn!("LLM response is not valid JSON, falling back to TAGS: delimiter parsing");
    parse_tags_delimiter(raw)
}

fn parse_tags_delimiter(response: &str) -> (String, Vec<String>) {
    let tags_pos = response.lines().find_map(|line| {
        let trimmed = line.trim().trim_start_matches('*');
        if trimmed.len() >= 5 && trimmed[..5].eq_ignore_ascii_case("tags:") {
            let line_start = line.as_ptr() as usize - response.as_ptr() as usize;
            let prefix_offset = line.len() - trimmed.len();
            trimmed.find(':').map(|colon| (line_start, prefix_offset + colon + 1))
        } else {
            None
        }
    });

    if let Some((line_start, tags_content_offset)) = tags_pos {
        let description = response[..line_start].trim().to_string();
        let tags_str = response[line_start + tags_content_offset..]
            .trim()
            .trim_end_matches('*');
        let tags: Vec<String> = tags_str
            .split(',')
            .map(|t| t.trim().trim_matches('*').to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        (description, tags)
    } else {
        (response.to_string(), Vec::new())
    }
}
