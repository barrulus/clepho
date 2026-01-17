use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone)]
pub struct LlmClient {
    endpoint: String,
    model: String,
}

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
    pub fn new(endpoint: &str, model: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        }
    }

    pub fn describe_image(&self, image_path: &Path) -> Result<String> {
        // Read and encode image as base64
        let image_data = std::fs::read(image_path)?;
        let base64_image = BASE64.encode(&image_data);

        // Determine MIME type from extension
        let mime_type = match image_path.extension().and_then(|e| e.to_str()) {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => "image/jpeg", // Default to JPEG
        };

        let data_url = format!("data:{};base64,{}", mime_type, base64_image);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![
                    ContentPart::Text {
                        text: "Describe this image in detail. Include information about: \
                               1) The main subject or scene \
                               2) Notable objects, people, or elements \
                               3) Colors, lighting, and mood \
                               4) Any text visible in the image \
                               5) Suggested tags for organizing this photo. \
                               Keep the description concise but informative."
                            .to_string(),
                    },
                    ContentPart::ImageUrl {
                        image_url: ImageUrl { url: data_url },
                    },
                ],
            }],
            max_tokens: 500,
            temperature: 0.7,
        };

        let url = format!("{}/chat/completions", self.endpoint);

        let response = ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_json(&request)
            .map_err(|e| anyhow!("LLM request failed: {}", e))?;

        let chat_response: ChatResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse LLM response: {}", e))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow!("No response from LLM"))
    }

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

    #[allow(dead_code)]
    pub fn test_connection(&self) -> Result<bool> {
        let url = format!("{}/models", self.endpoint);

        match ureq::get(&url).call() {
            Ok(response) => Ok(response.status() == 200),
            Err(_) => Ok(false),
        }
    }
}
