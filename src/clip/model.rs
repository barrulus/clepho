//! CLIP model implementation using ONNX Runtime

use anyhow::{anyhow, Result};
use image::DynamicImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// CLIP embedding (512-dimensional vector for ViT-B/32)
pub type ClipEmbedding = Vec<f32>;

/// CLIP visual encoder model
static VISUAL_MODEL: OnceLock<Mutex<Session>> = OnceLock::new();

/// CLIP text encoder model (optional, for text-to-image search)
static TEXT_MODEL: OnceLock<Mutex<Session>> = OnceLock::new();

/// CLIP model wrapper
pub struct ClipModel {
    _initialized: bool,
}

impl ClipModel {
    /// Create a new CLIP model instance
    pub fn new() -> Self {
        Self { _initialized: false }
    }

    /// Initialize CLIP models (downloads if needed)
    pub fn init(&mut self) -> Result<()> {
        init_visual_model()?;
        self._initialized = true;
        Ok(())
    }

    /// Check if models are ready
    pub fn is_ready(&self) -> bool {
        VISUAL_MODEL.get().is_some()
    }

    /// Generate embedding for an image file
    pub fn embed_image_file(&self, path: &Path) -> Result<ClipEmbedding> {
        let img = load_image_for_clip(path)?;
        self.embed_image(&img)
    }

    /// Generate embedding for a DynamicImage
    pub fn embed_image(&self, img: &DynamicImage) -> Result<ClipEmbedding> {
        if !self.is_ready() {
            init_visual_model()?;
        }

        run_visual_encoder(img)
    }

    /// Generate embedding for text (for text-to-image search)
    pub fn embed_text(&self, text: &str) -> Result<ClipEmbedding> {
        if TEXT_MODEL.get().is_none() {
            init_text_model()?;
        }

        run_text_encoder(text)
    }
}

impl Default for ClipModel {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the models directory path
fn get_models_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir()
        .ok_or_else(|| anyhow!("Could not find local data directory"))?;
    let models_dir = data_dir.join("clepho").join("models");
    std::fs::create_dir_all(&models_dir)?;
    Ok(models_dir)
}

/// Download a model file if it doesn't exist
fn ensure_model(filename: &str, url: &str) -> Result<PathBuf> {
    let models_dir = get_models_dir()?;
    let model_path = models_dir.join(filename);

    if !model_path.exists() {
        tracing::info!(model = %filename, "Downloading CLIP model...");
        let response = ureq::get(url)
            .call()
            .map_err(|e| anyhow!("Failed to download model: {}", e))?;

        let mut file = std::fs::File::create(&model_path)?;
        std::io::copy(&mut response.into_reader(), &mut file)?;
        tracing::info!(model = %filename, path = ?model_path, "CLIP model downloaded");
    }

    Ok(model_path)
}

/// Initialize the CLIP visual encoder
fn init_visual_model() -> Result<()> {
    if VISUAL_MODEL.get().is_some() {
        return Ok(());
    }

    // Using Qdrant's CLIP ViT-B/32 visual encoder (ONNX)
    // Source: https://huggingface.co/Qdrant/clip-ViT-B-32-vision
    let model_path = ensure_model(
        "clip-vit-b32-vision.onnx",
        "https://huggingface.co/Qdrant/clip-ViT-B-32-vision/resolve/main/model.onnx"
    )?;

    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(&model_path)?;

    let _ = VISUAL_MODEL.set(Mutex::new(session));
    Ok(())
}

/// Initialize the CLIP text encoder
fn init_text_model() -> Result<()> {
    if TEXT_MODEL.get().is_some() {
        return Ok(());
    }

    // Using Qdrant's CLIP ViT-B/32 text encoder (ONNX)
    // Source: https://huggingface.co/Qdrant/clip-ViT-B-32-text
    let model_path = ensure_model(
        "clip-vit-b32-text.onnx",
        "https://huggingface.co/Qdrant/clip-ViT-B-32-text/resolve/main/model.onnx"
    )?;

    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(&model_path)?;

    let _ = TEXT_MODEL.set(Mutex::new(session));
    Ok(())
}

/// Load image optimized for CLIP (224x224)
fn load_image_for_clip(path: &Path) -> Result<DynamicImage> {
    image::open(path).map_err(|e| anyhow!("Failed to load image: {}", e))
}

/// Run the visual encoder on an image
fn run_visual_encoder(img: &DynamicImage) -> Result<ClipEmbedding> {
    const INPUT_SIZE: u32 = 224;

    let mut model = VISUAL_MODEL.get()
        .ok_or_else(|| anyhow!("Visual model not initialized"))?
        .lock()
        .map_err(|e| anyhow!("Failed to lock model: {}", e))?;

    // Resize to CLIP input size (224x224)
    let resized = img.resize_exact(INPUT_SIZE, INPUT_SIZE, image::imageops::FilterType::Triangle);
    let rgb = resized.to_rgb8();

    // CLIP normalization constants (ImageNet stats)
    let mean = [0.48145466, 0.4578275, 0.40821073];
    let std = [0.26862954, 0.26130258, 0.27577711];

    // Convert to tensor (NCHW format, normalized)
    let mut input_data = vec![0.0f32; (3 * INPUT_SIZE * INPUT_SIZE) as usize];

    for y in 0..INPUT_SIZE as usize {
        for x in 0..INPUT_SIZE as usize {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            let idx = y * INPUT_SIZE as usize + x;

            // Normalize: (pixel/255 - mean) / std
            input_data[idx] = ((pixel[0] as f32 / 255.0) - mean[0]) / std[0]; // R
            input_data[INPUT_SIZE as usize * INPUT_SIZE as usize + idx] =
                ((pixel[1] as f32 / 255.0) - mean[1]) / std[1]; // G
            input_data[2 * INPUT_SIZE as usize * INPUT_SIZE as usize + idx] =
                ((pixel[2] as f32 / 255.0) - mean[2]) / std[2]; // B
        }
    }

    // Create tensor
    let input_tensor = Tensor::from_array((
        [1usize, 3, INPUT_SIZE as usize, INPUT_SIZE as usize],
        input_data.into_boxed_slice()
    ))?;

    // Run inference
    let outputs = model.run(ort::inputs!["pixel_values" => input_tensor])?;

    // Get embedding output
    let embedding_output = outputs.iter().next()
        .ok_or_else(|| anyhow!("No embedding output"))?;

    let (_shape, embedding_data) = embedding_output.1
        .try_extract_tensor::<f32>()?;

    // L2 normalize the embedding
    let embedding: Vec<f32> = embedding_data.to_vec();
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm > 0.0 {
        Ok(embedding.iter().map(|x| x / norm).collect())
    } else {
        Ok(embedding)
    }
}

/// Run the text encoder on a string
fn run_text_encoder(text: &str) -> Result<ClipEmbedding> {
    let mut model = TEXT_MODEL.get()
        .ok_or_else(|| anyhow!("Text model not initialized"))?
        .lock()
        .map_err(|e| anyhow!("Failed to lock model: {}", e))?;

    // Simple tokenization (CLIP uses BPE, this is a simplified version)
    // For full accuracy, we'd need the CLIP tokenizer
    let tokens = simple_tokenize(text);

    // Pad/truncate to 77 tokens (CLIP's context length)
    let mut input_ids = vec![49406i64]; // Start token
    input_ids.extend(tokens.iter().take(75).cloned());
    input_ids.push(49407); // End token

    // Pad to 77
    while input_ids.len() < 77 {
        input_ids.push(0);
    }

    let input_tensor = Tensor::from_array((
        [1usize, 77],
        input_ids.into_boxed_slice()
    ))?;

    // Run inference
    let outputs = model.run(ort::inputs!["input_ids" => input_tensor])?;

    // Get embedding
    let embedding_output = outputs.iter().next()
        .ok_or_else(|| anyhow!("No embedding output"))?;

    let (_shape, embedding_data) = embedding_output.1
        .try_extract_tensor::<f32>()?;

    // L2 normalize
    let embedding: Vec<f32> = embedding_data.to_vec();
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm > 0.0 {
        Ok(embedding.iter().map(|x| x / norm).collect())
    } else {
        Ok(embedding)
    }
}

/// Simple tokenization for common words (placeholder - real CLIP uses BPE)
fn simple_tokenize(text: &str) -> Vec<i64> {
    // This is a very simplified tokenizer
    // Real CLIP uses BPE tokenization with a specific vocabulary
    // For prototype, we just convert chars to pseudo-tokens
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .take(75)
        .map(|c| c as i64)
        .collect()
}

/// Calculate cosine similarity between two CLIP embeddings
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);
    }
}
