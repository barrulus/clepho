use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use image::GenericImageView;
use image::codecs::jpeg::JpegEncoder;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;

/// Detected face information from LLM (reserved for LLM-based face detection)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DetectedFace {
    /// Bounding box as percentage of image dimensions (0-100)
    pub x_percent: f32,
    pub y_percent: f32,
    pub width_percent: f32,
    pub height_percent: f32,
    /// Optional description of the face (age, expression, etc.)
    pub description: Option<String>,
    /// Confidence score (0-1)
    pub confidence: f32,
}

/// Response from face detection (reserved for LLM-based face detection)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FaceDetectionResponse {
    pub faces: Vec<DetectedFace>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
}

/// Trait for LLM providers that can describe images
pub trait LlmProvider: Send + Sync {
    /// Describe an image at the given path
    fn describe_image(&self, image_path: &Path) -> Result<String>;

    /// Get the provider name for display
    fn provider_name(&self) -> &'static str;

    /// Get text embedding for semantic search (optional)
    fn get_text_embedding(&self, _text: &str) -> Result<Vec<f32>> {
        Err(anyhow!("Embeddings not supported by this provider"))
    }

    /// Check if this provider supports embeddings
    fn supports_embeddings(&self) -> bool {
        false
    }

    /// Detect faces in an image (optional, reserved for future implementation)
    #[allow(dead_code)]
    fn detect_faces(&self, image_path: &Path) -> Result<FaceDetectionResponse> {
        // Default implementation that extracts faces from image description
        let _ = image_path;
        Err(anyhow!("Face detection not supported by this provider"))
    }

    /// Check if this provider supports face detection
    #[allow(dead_code)]
    fn supports_face_detection(&self) -> bool {
        false
    }
}

// ============================================================================
// OpenAI-compatible provider (works with LM Studio, OpenAI, and compatible APIs)
// ============================================================================

pub struct OpenAICompatibleProvider {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    embedding_model: String,
    custom_prompt: Option<String>,
    base_prompt: Option<String>,
    json_mode: bool,
    agent: ureq::Agent,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: OpenAIContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAIContent {
    Text(String),
    Parts(Vec<OpenAIContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OpenAIContentPart {
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
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    content: String,
}

// Embedding request/response structs
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAICompatibleProvider {
    pub fn new(endpoint: &str, model: &str, api_key: Option<&str>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .build();
        Self {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
            api_key: api_key.map(|s| s.to_string()),
            embedding_model: "text-embedding-ada-002".to_string(),
            custom_prompt: None,
            base_prompt: None,
            json_mode: false,
            agent,
        }
    }

    #[allow(dead_code)]
    pub fn with_embedding_model(mut self, model: &str) -> Self {
        self.embedding_model = model.to_string();
        self
    }

    pub fn with_custom_prompt(mut self, prompt: Option<String>) -> Self {
        self.custom_prompt = prompt;
        self
    }

    pub fn with_base_prompt(mut self, prompt: Option<String>) -> Self {
        self.base_prompt = prompt;
        self
    }

    pub fn with_json_mode(mut self, json_mode: bool) -> Self {
        self.json_mode = json_mode;
        self
    }

    fn get_image_prompt(&self) -> String {
        build_image_prompt(self.custom_prompt.as_deref(), self.base_prompt.as_deref())
    }
}

impl LlmProvider for OpenAICompatibleProvider {
    fn describe_image(&self, image_path: &Path) -> Result<String> {
        let (base64_image, mime_type) = load_and_encode_image(image_path, 1024)?;
        let data_url = format!("data:{};base64,{}", mime_type, base64_image);

        let response_format = if self.json_mode {
            Some(ResponseFormat { format_type: "json_object".to_string() })
        } else {
            None
        };

        let request = OpenAIChatRequest {
            model: self.model.clone(),
            messages: vec![
                OpenAIMessage {
                    role: "system".to_string(),
                    content: OpenAIContent::Text(SYSTEM_PROMPT.to_string()),
                },
                OpenAIMessage {
                    role: "user".to_string(),
                    content: OpenAIContent::Parts(vec![
                        OpenAIContentPart::Text {
                            text: self.get_image_prompt(),
                        },
                        OpenAIContentPart::ImageUrl {
                            image_url: ImageUrl { url: data_url },
                        },
                    ]),
                },
            ],
            max_tokens: 500,
            temperature: 0.3,
            response_format,
        };

        let url = format!("{}/chat/completions", self.endpoint);

        let mut req = self.agent.post(&url).set("Content-Type", "application/json");

        if let Some(ref api_key) = self.api_key {
            req = req.set("Authorization", &format!("Bearer {}", api_key));
        }

        let response = req
            .send_json(&request)
            .map_err(|e| anyhow!("LLM request failed: {}", e))?;

        let chat_response: OpenAIChatResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse LLM response: {}", e))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow!("No response from LLM"))
    }

    fn provider_name(&self) -> &'static str {
        "OpenAI-compatible"
    }

    fn get_text_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingRequest {
            model: self.embedding_model.clone(),
            input: text.to_string(),
        };

        let url = format!("{}/embeddings", self.endpoint);

        let mut req = self.agent.post(&url).set("Content-Type", "application/json");

        if let Some(ref api_key) = self.api_key {
            req = req.set("Authorization", &format!("Bearer {}", api_key));
        }

        let response = req
            .send_json(&request)
            .map_err(|e| anyhow!("Embedding request failed: {}", e))?;

        let embedding_response: EmbeddingResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse embedding response: {}", e))?;

        embedding_response
            .data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| anyhow!("No embedding in response"))
    }

    fn supports_embeddings(&self) -> bool {
        self.api_key.is_some() // Embeddings typically require API key
    }

    fn detect_faces(&self, image_path: &Path) -> Result<FaceDetectionResponse> {
        let (base64_image, mime_type) = load_and_encode_image(image_path, 2048)?;
        let data_url = format!("data:{};base64,{}", mime_type, base64_image);

        let request = OpenAIChatRequest {
            model: self.model.clone(),
            messages: vec![
                OpenAIMessage {
                    role: "system".to_string(),
                    content: OpenAIContent::Text(SYSTEM_PROMPT.to_string()),
                },
                OpenAIMessage {
                    role: "user".to_string(),
                    content: OpenAIContent::Parts(vec![
                        OpenAIContentPart::Text {
                            text: FACE_DETECTION_PROMPT.to_string(),
                        },
                        OpenAIContentPart::ImageUrl {
                            image_url: ImageUrl { url: data_url },
                        },
                    ]),
                },
            ],
            max_tokens: 1000,
            temperature: 0.3,
            response_format: None,
        };

        let url = format!("{}/chat/completions", self.endpoint);

        let mut req = self.agent.post(&url).set("Content-Type", "application/json");

        if let Some(ref api_key) = self.api_key {
            req = req.set("Authorization", &format!("Bearer {}", api_key));
        }

        let response = req
            .send_json(&request)
            .map_err(|e| anyhow!("Face detection request failed: {}", e))?;

        let chat_response: OpenAIChatResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse face detection response: {}", e))?;

        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow!("No response from LLM"))?;

        // Parse the JSON response
        // Try to extract JSON from the response (handle markdown code blocks)
        let json_str = extract_json(&content);

        let detection: FaceDetectionResponse = serde_json::from_str(&json_str)
            .map_err(|e| anyhow!("Failed to parse face detection JSON: {} - Response was: {}", e, content))?;

        Ok(detection)
    }

    fn supports_face_detection(&self) -> bool {
        true
    }
}

/// Load an image, resize if either dimension exceeds `max_dimension`, re-encode as JPEG,
/// and return the base64-encoded string along with the MIME type.
fn load_and_encode_image(image_path: &Path, max_dimension: u32) -> Result<(String, &'static str)> {
    let img = image::open(image_path)
        .map_err(|e| anyhow!("Failed to open image {}: {}", image_path.display(), e))?;

    let (width, height) = img.dimensions();
    let img = if width > max_dimension || height > max_dimension {
        img.resize(
            max_dimension,
            max_dimension,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, 85);
    img.write_with_encoder(encoder)
        .map_err(|e| anyhow!("Failed to encode image as JPEG: {}", e))?;

    let base64_image = BASE64.encode(buf.into_inner());
    Ok((base64_image, "image/jpeg"))
}

/// The face detection prompt shared across all providers.
/// System prompt for photo cataloguing tasks.
const SYSTEM_PROMPT: &str = "You are a photo cataloguing assistant. Describe images factually and concisely. Always respond with valid JSON in the exact format requested.";

const FACE_DETECTION_PROMPT: &str = r#"Analyze this image and detect all human faces present.

For each face found, provide:
1. The approximate bounding box as percentages of the image (x, y, width, height from 0-100)
2. A brief description (age estimate, expression, any notable features)
3. Your confidence level (0-1)

Return the results as JSON in this exact format:
{
  "faces": [
    {
      "x_percent": <number 0-100>,
      "y_percent": <number 0-100>,
      "width_percent": <number 0-100>,
      "height_percent": <number 0-100>,
      "description": "<brief description>",
      "confidence": <number 0-1>
    }
  ]
}

If no faces are found, return: {"faces": []}

Return ONLY the JSON, no other text."#;

/// Returns the base image description prompt.
/// The LLM is asked to return a JSON object with description and tags fields.
fn base_image_prompt() -> &'static str {
    "Describe this image in detail. Include information about:\n\
     1) The main subject or scene\n\
     2) Notable objects, people, or elements\n\
     3) Colors, lighting, and mood\n\
     4) Any text visible in the image\n\
     Keep the description concise but informative.\n\n\
     Respond with a JSON object containing exactly two fields:\n\
     - \"description\": your image description as a single string\n\
     - \"tags\": an array of lowercase tag strings for organizing this photo\n\n\
     Example: {\"description\": \"A golden sunset over mountain peaks...\", \"tags\": [\"nature\", \"sunset\", \"mountain\", \"landscape\"]}\n\n\
     Return ONLY the JSON object, no other text."
}

/// Builds the full prompt with optional custom context and optional base prompt override
fn build_image_prompt(custom_prompt: Option<&str>, base_prompt: Option<&str>) -> String {
    let base = base_prompt.unwrap_or_else(|| base_image_prompt());
    match custom_prompt {
        Some(context) => format!("Context: {}\n\n{}", context, base),
        None => base.to_string(),
    }
}

/// Extract JSON from a string that might contain markdown code blocks
pub(crate) fn extract_json(content: &str) -> String {
    let trimmed = content.trim();

    // Check for markdown code block
    if trimmed.starts_with("```") {
        // Find the end of the code block
        if let Some(start) = trimmed.find('\n') {
            let after_first_line = &trimmed[start + 1..];
            if let Some(end) = after_first_line.rfind("```") {
                return after_first_line[..end].trim().to_string();
            }
        }
    }

    // Already plain JSON
    trimmed.to_string()
}

// ============================================================================
// Anthropic Claude provider
// ============================================================================

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    custom_prompt: Option<String>,
    base_prompt: Option<String>,
    agent: ureq::Agent,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
}

#[derive(Debug, Serialize)]
struct AnthropicImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponseContent {
    text: Option<String>,
}

impl AnthropicProvider {
    pub fn new(api_key: &str, model: Option<&str>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .build();
        Self {
            api_key: api_key.to_string(),
            model: model.unwrap_or("claude-sonnet-4-20250514").to_string(),
            custom_prompt: None,
            base_prompt: None,
            agent,
        }
    }

    pub fn with_custom_prompt(mut self, prompt: Option<String>) -> Self {
        self.custom_prompt = prompt;
        self
    }

    pub fn with_base_prompt(mut self, prompt: Option<String>) -> Self {
        self.base_prompt = prompt;
        self
    }
}

impl LlmProvider for AnthropicProvider {
    fn describe_image(&self, image_path: &Path) -> Result<String> {
        let (base64_image, media_type) = load_and_encode_image(image_path, 1024)?;

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 500,
            system: SYSTEM_PROMPT.to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![
                    AnthropicContent::Image {
                        source: AnthropicImageSource {
                            source_type: "base64".to_string(),
                            media_type: media_type.to_string(),
                            data: base64_image,
                        },
                    },
                    AnthropicContent::Text {
                        text: build_image_prompt(self.custom_prompt.as_deref(), self.base_prompt.as_deref()),
                    },
                ],
            }],
        };

        let response = self.agent.post("https://api.anthropic.com/v1/messages")
            .set("Content-Type", "application/json")
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .send_json(&request)
            .map_err(|e| anyhow!("Anthropic request failed: {}", e))?;

        let anthropic_response: AnthropicResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse Anthropic response: {}", e))?;

        anthropic_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .ok_or_else(|| anyhow!("No response from Anthropic"))
    }

    fn provider_name(&self) -> &'static str {
        "Anthropic Claude"
    }

    fn detect_faces(&self, image_path: &Path) -> Result<FaceDetectionResponse> {
        let (base64_image, media_type) = load_and_encode_image(image_path, 2048)?;

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1000,
            system: SYSTEM_PROMPT.to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![
                    AnthropicContent::Image {
                        source: AnthropicImageSource {
                            source_type: "base64".to_string(),
                            media_type: media_type.to_string(),
                            data: base64_image,
                        },
                    },
                    AnthropicContent::Text {
                        text: FACE_DETECTION_PROMPT.to_string(),
                    },
                ],
            }],
        };

        let response = self.agent.post("https://api.anthropic.com/v1/messages")
            .set("Content-Type", "application/json")
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .send_json(&request)
            .map_err(|e| anyhow!("Anthropic face detection request failed: {}", e))?;

        let anthropic_response: AnthropicResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse Anthropic face detection response: {}", e))?;

        let content = anthropic_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .ok_or_else(|| anyhow!("No response from Anthropic"))?;

        let json_str = extract_json(&content);

        let detection: FaceDetectionResponse = serde_json::from_str(&json_str)
            .map_err(|e| anyhow!("Failed to parse face detection JSON: {} - Response was: {}", e, content))?;

        Ok(detection)
    }

    fn supports_face_detection(&self) -> bool {
        true
    }
}

// ============================================================================
// Ollama provider
// ============================================================================

pub struct OllamaProvider {
    endpoint: String,
    model: String,
    embedding_model: String,
    custom_prompt: Option<String>,
    base_prompt: Option<String>,
    json_mode: bool,
    agent: ureq::Agent,
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: String,
    images: Vec<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

impl OllamaProvider {
    pub fn new(endpoint: Option<&str>, model: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(180))
            .build();
        Self {
            endpoint: endpoint.unwrap_or("http://localhost:11434").to_string(),
            model: model.to_string(),
            embedding_model: "nomic-embed-text".to_string(), // Default embedding model
            custom_prompt: None,
            base_prompt: None,
            json_mode: false,
            agent,
        }
    }

    #[allow(dead_code)]
    pub fn with_embedding_model(mut self, model: &str) -> Self {
        self.embedding_model = model.to_string();
        self
    }

    pub fn with_custom_prompt(mut self, prompt: Option<String>) -> Self {
        self.custom_prompt = prompt;
        self
    }

    pub fn with_base_prompt(mut self, prompt: Option<String>) -> Self {
        self.base_prompt = prompt;
        self
    }

    pub fn with_json_mode(mut self, json_mode: bool) -> Self {
        self.json_mode = json_mode;
        self
    }
}

impl LlmProvider for OllamaProvider {
    fn describe_image(&self, image_path: &Path) -> Result<String> {
        let (base64_image, _mime_type) = load_and_encode_image(image_path, 1024)?;

        let format = if self.json_mode {
            Some(serde_json::json!("json"))
        } else {
            None
        };

        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: build_image_prompt(self.custom_prompt.as_deref(), self.base_prompt.as_deref()),
            system: SYSTEM_PROMPT.to_string(),
            images: vec![base64_image],
            stream: false,
            format,
        };

        let url = format!("{}/api/generate", self.endpoint);

        let response = self.agent.post(&url)
            .set("Content-Type", "application/json")
            .send_json(&request)
            .map_err(|e| anyhow!("Ollama request failed: {}", e))?;

        let ollama_response: OllamaResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse Ollama response: {}", e))?;

        Ok(ollama_response.response)
    }

    fn provider_name(&self) -> &'static str {
        "Ollama"
    }

    fn get_text_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = OllamaEmbeddingRequest {
            model: self.embedding_model.clone(),
            prompt: text.to_string(),
        };

        let url = format!("{}/api/embeddings", self.endpoint);

        let response = self.agent.post(&url)
            .set("Content-Type", "application/json")
            .send_json(&request)
            .map_err(|e| anyhow!("Ollama embedding request failed: {}", e))?;

        let embedding_response: OllamaEmbeddingResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse Ollama embedding response: {}", e))?;

        Ok(embedding_response.embedding)
    }

    fn supports_embeddings(&self) -> bool {
        true
    }

    fn detect_faces(&self, image_path: &Path) -> Result<FaceDetectionResponse> {
        let (base64_image, _mime_type) = load_and_encode_image(image_path, 2048)?;

        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: FACE_DETECTION_PROMPT.to_string(),
            system: SYSTEM_PROMPT.to_string(),
            images: vec![base64_image],
            stream: false,
            format: None,
        };

        let url = format!("{}/api/generate", self.endpoint);

        let response = self.agent.post(&url)
            .set("Content-Type", "application/json")
            .send_json(&request)
            .map_err(|e| anyhow!("Ollama face detection request failed: {}", e))?;

        let ollama_response: OllamaResponse = response
            .into_json()
            .map_err(|e| anyhow!("Failed to parse Ollama face detection response: {}", e))?;

        let json_str = extract_json(&ollama_response.response);

        let detection: FaceDetectionResponse = serde_json::from_str(&json_str)
            .map_err(|e| anyhow!("Failed to parse face detection JSON: {} - Response was: {}", e, ollama_response.response))?;

        Ok(detection)
    }

    fn supports_face_detection(&self) -> bool {
        true
    }
}

// ============================================================================
// Factory function
// ============================================================================

use crate::config::{LlmConfig, LlmProviderType};

/// Create an LLM provider based on configuration
pub fn create_provider(config: &LlmConfig) -> Box<dyn LlmProvider> {
    let custom_prompt = config.custom_prompt.clone();
    let base_prompt = config.base_prompt.clone();
    let json_mode = config.json_mode;

    match config.provider {
        LlmProviderType::LmStudio => Box::new(
            OpenAICompatibleProvider::new(
                &config.endpoint,
                &config.model,
                config.api_key.as_deref(),
            )
            .with_custom_prompt(custom_prompt)
            .with_base_prompt(base_prompt)
            .with_json_mode(json_mode),
        ),
        LlmProviderType::OpenAI => Box::new(
            OpenAICompatibleProvider::new(
                "https://api.openai.com/v1",
                &config.model,
                config.api_key.as_deref(),
            )
            .with_custom_prompt(custom_prompt)
            .with_base_prompt(base_prompt)
            .with_json_mode(json_mode),
        ),
        LlmProviderType::Anthropic => {
            let api_key = config.api_key.as_deref().unwrap_or("");
            Box::new(
                AnthropicProvider::new(api_key, Some(&config.model))
                    .with_custom_prompt(custom_prompt)
                    .with_base_prompt(base_prompt),
            )
        }
        LlmProviderType::Ollama => Box::new(
            OllamaProvider::new(Some(&config.endpoint), &config.model)
                .with_custom_prompt(custom_prompt)
                .with_base_prompt(base_prompt)
                .with_json_mode(json_mode),
        ),
    }
}
