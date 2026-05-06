//! Bridges the existing `crate::llm::client::LlmClient` to the narrower
//! `LlmDescribeClient` trait the LLM stage uses. Keeps the stage trait small
//! and mockable; the daemon driver wraps the real `LlmClient` in this adapter
//! before handing it to `LlmStage`.

use crate::llm::client::LlmClient;
use crate::pipeline::stages::llm::LlmDescribeClient;
use anyhow::Result;
use std::path::Path;

#[allow(dead_code)]
pub struct LlmClientAdapter(pub LlmClient);

impl LlmDescribeClient for LlmClientAdapter {
    fn describe_and_tag_image(
        &self,
        image_path: &Path,
        custom_prompt: Option<&str>,
    ) -> Result<(String, Vec<String>)> {
        self.0
            .describe_and_tag_image_with_prompt(image_path, custom_prompt)
    }

    fn text_embedding(&self, text: &str) -> Result<Option<Vec<f32>>> {
        if !self.0.supports_embeddings() {
            return Ok(None);
        }
        self.0.get_text_embedding(text).map(Some)
    }

    fn embedding_model_name(&self) -> &'static str {
        // The existing LlmClient doesn't expose the embedding model name as
        // a stable string. The pipeline log records the model in
        // pipeline_events so a missing label here is recoverable, just
        // suboptimal. Fix by threading a getter through LlmClient/LlmProvider
        // when embeddings become a first-class concern.
        ""
    }
}
