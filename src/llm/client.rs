use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

use crate::config::LlmConfig;
use super::provider::{create_provider, LlmProvider};

/// LLM client that wraps a provider implementation
pub struct LlmClient {
    provider: Arc<dyn LlmProvider>,
    // Keep the legacy fields for backwards compatibility
    endpoint: String,
    model: String,
}

// Request/response structs for OpenAI-compatible API (used by legacy methods)
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: Vec<ContentPart>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

impl LlmClient {
    /// Create a new LlmClient with legacy endpoint/model parameters
    /// (for backwards compatibility)
    pub fn new(endpoint: &str, model: &str) -> Self {
        // Create a default config for backwards compatibility
        let config = LlmConfig {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
            ..Default::default()
        };
        let provider = create_provider(&config);

        Self {
            provider: Arc::from(provider),
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        }
    }

    /// Create a new LlmClient from configuration
    pub fn from_config(config: &LlmConfig) -> Self {
        let provider = create_provider(config);

        Self {
            provider: Arc::from(provider),
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
        }
    }

    /// Get the provider name
    #[allow(dead_code)]
    pub fn provider_name(&self) -> &'static str {
        self.provider.provider_name()
    }

    /// Describe an image using the configured provider
    pub fn describe_image(&self, image_path: &Path) -> Result<String> {
        self.provider.describe_image(image_path)
    }

    /// Describe an image and generate tags in a single LLM call.
    /// The LLM response is expected to contain a description followed by a `TAGS:` line.
    /// Falls back to using the full response as description with empty tags if delimiter not found.
    pub fn describe_and_tag_image(&self, image_path: &Path) -> Result<(String, Vec<String>)> {
        let response = self.provider.describe_image(image_path)?;

        if let Some(tags_idx) = response.find("TAGS:") {
            let description = response[..tags_idx].trim().to_string();
            let tags_str = response[tags_idx + 5..].trim();
            let tags: Vec<String> = tags_str
                .split(',')
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            Ok((description, tags))
        } else {
            Ok((response, Vec::new()))
        }
    }

    /// Generate tags from a description (uses legacy OpenAI-compatible API)
    #[allow(dead_code)]
    pub fn generate_tags(&self, description: &str) -> Result<Vec<String>> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![ContentPart::Text {
                    text: format!(
                        "Based on this image description, generate a list of relevant tags \
                         for organizing this photo. Return only the tags, one per line, \
                         without numbers or bullet points.\n\nDescription: {}",
                        description
                    ),
                }],
            }],
            max_tokens: 200,
            temperature: 0.5,
        };

        let url = format!("{}/chat/completions", self.endpoint);

        let response = ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_json(&request)
            .map_err(|e| anyhow!("LLM request failed: {}", e))?;

        let chat_response: ChatResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse LLM response: {}", e))?;

        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let tags: Vec<String> = content
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();

        Ok(tags)
    }

    /// Get text embedding for semantic search
    pub fn get_text_embedding(&self, text: &str) -> Result<Vec<f32>> {
        self.provider.get_text_embedding(text)
    }

    /// Check if the provider supports embeddings
    pub fn supports_embeddings(&self) -> bool {
        self.provider.supports_embeddings()
    }

    #[allow(dead_code)]
    pub fn test_connection(&self) -> Result<bool> {
        let url = format!("{}/models", self.endpoint);

        match ureq::get(&url).call() {
            Ok(response) => Ok(response.status() == 200),
            Err(_) => Ok(false),
        }
    }
}

// Make LlmClient Clone by wrapping provider in Arc
impl Clone for LlmClient {
    fn clone(&self) -> Self {
        Self {
            provider: Arc::clone(&self.provider),
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
        }
    }
}
