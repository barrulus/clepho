use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::config::LlmConfig;
use super::provider::{create_provider, LlmProvider};

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
    /// The LLM response is expected to contain a description followed by a `TAGS:` line.
    /// Falls back to using the full response as description with empty tags if delimiter not found.
    pub fn describe_and_tag_image(&self, image_path: &Path) -> Result<(String, Vec<String>)> {
        let response = self.provider.describe_image(image_path)?;

        // Find TAGS: delimiter case-insensitively, anchored to line start.
        // Also handles markdown bold like **TAGS:** or **Tags:**
        let tags_pos = response.lines().enumerate().find_map(|(_, line)| {
            let trimmed = line.trim().trim_start_matches('*');
            if trimmed.len() >= 5 && trimmed[..5].eq_ignore_ascii_case("tags:") {
                // Return byte offset of the tags content after "TAGS:"
                let line_start = line.as_ptr() as usize - response.as_ptr() as usize;
                let prefix_offset = line.len() - trimmed.len();
                // Find the colon position to get content after it
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
            Ok((response, Vec::new()))
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
