use anyhow::{anyhow, Result};
use image::{DynamicImage, GenericImageView};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::db::BoundingBox;

/// A detected face with bounding box and embedding
#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub bbox: BoundingBox,
    pub embedding: Vec<f32>,
    pub confidence: f32,
}

/// Face detection model (UltraFace - lightweight and fast)
static DETECTION_MODEL: OnceLock<Mutex<Session>> = OnceLock::new();
/// Face embedding model (ArcFace - generates 512-dim embeddings)
static EMBEDDING_MODEL: OnceLock<Mutex<Session>> = OnceLock::new();

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
        tracing::info!(model = %filename, "Downloading model...");
        let response = ureq::get(url)
            .call()
            .map_err(|e| anyhow!("Failed to download model: {}", e))?;

        let mut file = std::fs::File::create(&model_path)?;
        std::io::copy(&mut response.into_reader(), &mut file)?;
        tracing::info!(model = %filename, path = ?model_path, "Model downloaded");
    }

    Ok(model_path)
}

/// Initialize face detection model only (fast - just UltraFace)
pub fn init_detection_model() -> Result<()> {
    if DETECTION_MODEL.get().is_some() {
        return Ok(());
    }

    // UltraFace model for detection (320x240 version - fast)
    let detection_model_path = ensure_model(
        "ultraface-320.onnx",
        "https://github.com/onnx/models/raw/main/validated/vision/body_analysis/ultraface/models/version-RFB-320.onnx"
    )?;

    let detection_session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(&detection_model_path)?;

    let _ = DETECTION_MODEL.set(Mutex::new(detection_session));
    Ok(())
}

/// Initialize face embedding model (slower - ArcFace ResNet100)
fn init_embedding_model() -> Result<()> {
    if EMBEDDING_MODEL.get().is_some() {
        return Ok(());
    }

    // ArcFace model for embeddings
    let embedding_model_path = ensure_model(
        "arcface-resnet100.onnx",
        "https://github.com/onnx/models/raw/main/validated/vision/body_analysis/arcface/model/arcfaceresnet100-11-int8.onnx"
    )?;

    let embedding_session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(&embedding_model_path)?;

    let _ = EMBEDDING_MODEL.set(Mutex::new(embedding_session));
    Ok(())
}

/// Initialize both face detection models (for backwards compatibility)
pub fn init_models() -> Result<()> {
    init_detection_model()?;
    init_embedding_model()?;
    Ok(())
}

/// Check if models are initialized
pub fn models_initialized() -> bool {
    DETECTION_MODEL.get().is_some() && EMBEDDING_MODEL.get().is_some()
}

/// Check if detection model is initialized
pub fn detection_model_initialized() -> bool {
    DETECTION_MODEL.get().is_some()
}

/// Detect faces in an image file (with embeddings - slower)
pub fn detect_faces(image_path: &Path) -> Result<Vec<DetectedFace>> {
    if !models_initialized() {
        init_models()?;
    }

    let img = load_image_for_detection(image_path)?;
    detect_faces_in_image(&img)
}

/// Detect faces in an image file (fast mode - no embeddings)
/// Use this for initial scanning, then generate embeddings later when needed
pub fn detect_faces_fast(image_path: &Path) -> Result<Vec<DetectedFace>> {
    if !detection_model_initialized() {
        init_detection_model()?;
    }

    let img = load_image_for_detection(image_path)?;
    detect_faces_only(&img)
}

/// Load image optimized for face detection
fn load_image_for_detection(path: &Path) -> Result<DynamicImage> {
    image::open(path).map_err(|e| anyhow!("Failed to load image: {}", e))
}

/// Detect faces in a DynamicImage (with embeddings - slower)
pub fn detect_faces_in_image(img: &DynamicImage) -> Result<Vec<DetectedFace>> {
    detect_faces_in_image_impl(img, true)
}

/// Detect faces without generating embeddings (fast mode)
pub fn detect_faces_only(img: &DynamicImage) -> Result<Vec<DetectedFace>> {
    detect_faces_in_image_impl(img, false)
}

/// Internal implementation with optional embedding generation
fn detect_faces_in_image_impl(img: &DynamicImage, generate_embeddings: bool) -> Result<Vec<DetectedFace>> {
    // Always need detection model
    if DETECTION_MODEL.get().is_none() {
        init_detection_model()?;
    }

    let mut detection_model = DETECTION_MODEL.get()
        .ok_or_else(|| anyhow!("Detection model not initialized"))?
        .lock()
        .map_err(|e| anyhow!("Failed to lock detection model: {}", e))?;

    let (orig_width, orig_height) = img.dimensions();

    // Detect faces using UltraFace
    let face_boxes = run_ultraface_detection(&mut *detection_model, img)?;

    if face_boxes.is_empty() {
        return Ok(Vec::new());
    }

    // If not generating embeddings, return faces with empty embeddings
    if !generate_embeddings {
        return Ok(face_boxes
            .into_iter()
            .filter(|(bbox, _)| bbox.width > 0 && bbox.height > 0)
            .map(|(bbox, confidence)| DetectedFace {
                bbox,
                embedding: Vec::new(), // Empty - will be generated later if needed
                confidence,
            })
            .collect());
    }

    // Generate embeddings (slower path)
    if EMBEDDING_MODEL.get().is_none() {
        init_embedding_model()?;
    }

    let mut embedding_model = EMBEDDING_MODEL.get()
        .ok_or_else(|| anyhow!("Embedding model not initialized"))?
        .lock()
        .map_err(|e| anyhow!("Failed to lock embedding model: {}", e))?;

    let mut detected_faces = Vec::new();

    for (bbox, confidence) in face_boxes {
        // Validate bbox
        if bbox.width <= 0 || bbox.height <= 0 {
            continue;
        }

        // Crop face region for embedding (with some padding)
        let face_crop = crop_face(img, &bbox, orig_width, orig_height);

        // Generate embedding
        let embedding = match run_arcface_embedding(&mut *embedding_model, &face_crop) {
            Ok(emb) => emb,
            Err(_) => {
                // If embedding fails, still return the face with empty embedding
                vec![0.0; 512]
            }
        };

        detected_faces.push(DetectedFace {
            bbox,
            embedding,
            confidence,
        });
    }

    Ok(detected_faces)
}

/// Run UltraFace detection model
fn run_ultraface_detection(session: &mut Session, img: &DynamicImage) -> Result<Vec<(BoundingBox, f32)>> {
    const INPUT_WIDTH: u32 = 320;
    const INPUT_HEIGHT: u32 = 240;
    const CONFIDENCE_THRESHOLD: f32 = 0.7;
    const NMS_THRESHOLD: f32 = 0.3;

    let (orig_width, orig_height) = img.dimensions();

    // Resize image to model input size (use Triangle/bilinear for speed)
    let resized = img.resize_exact(INPUT_WIDTH, INPUT_HEIGHT, image::imageops::FilterType::Triangle);
    let rgb = resized.to_rgb8();

    // Convert to tensor (NCHW format, normalized)
    let mut input_data = vec![0.0f32; (3 * INPUT_HEIGHT * INPUT_WIDTH) as usize];

    for y in 0..INPUT_HEIGHT as usize {
        for x in 0..INPUT_WIDTH as usize {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            let idx = y * INPUT_WIDTH as usize + x;
            input_data[idx] = (pixel[0] as f32 - 127.0) / 128.0; // R
            input_data[INPUT_HEIGHT as usize * INPUT_WIDTH as usize + idx] = (pixel[1] as f32 - 127.0) / 128.0; // G
            input_data[2 * INPUT_HEIGHT as usize * INPUT_WIDTH as usize + idx] = (pixel[2] as f32 - 127.0) / 128.0; // B
        }
    }

    // Create tensor
    let input_tensor = Tensor::from_array(([1usize, 3, INPUT_HEIGHT as usize, INPUT_WIDTH as usize], input_data.into_boxed_slice()))?;

    // Run inference
    let outputs = session.run(ort::inputs!["input" => input_tensor])?;

    // Parse outputs - UltraFace outputs: scores and boxes
    let scores_value = outputs.get("scores")
        .ok_or_else(|| anyhow!("No scores output"))?;
    let boxes_value = outputs.get("boxes")
        .ok_or_else(|| anyhow!("No boxes output"))?;

    let (scores_shape, scores_data) = scores_value.try_extract_tensor::<f32>()?;
    let (_boxes_shape, boxes_data) = boxes_value.try_extract_tensor::<f32>()?;

    let mut face_boxes = Vec::new();

    // scores shape: [1, num_anchors, 2] (background, face)
    // boxes shape: [1, num_anchors, 4] (x1, y1, x2, y2 normalized)
    let num_anchors = scores_shape[1] as usize;

    for i in 0..num_anchors {
        // Flat index: scores_data[i * 2 + class]
        let confidence = scores_data[i * 2 + 1]; // Face confidence (class 1)

        if confidence > CONFIDENCE_THRESHOLD {
            // Flat index: boxes_data[i * 4 + coord]
            let x1 = (boxes_data[i * 4 + 0] * orig_width as f32) as i32;
            let y1 = (boxes_data[i * 4 + 1] * orig_height as f32) as i32;
            let x2 = (boxes_data[i * 4 + 2] * orig_width as f32) as i32;
            let y2 = (boxes_data[i * 4 + 3] * orig_height as f32) as i32;

            let bbox = BoundingBox {
                x: x1.max(0),
                y: y1.max(0),
                width: (x2 - x1).max(1),
                height: (y2 - y1).max(1),
            };

            face_boxes.push((bbox, confidence));
        }
    }

    // Apply non-maximum suppression
    face_boxes = nms(face_boxes, NMS_THRESHOLD);

    Ok(face_boxes)
}

/// Non-maximum suppression to remove overlapping detections
fn nms(mut boxes: Vec<(BoundingBox, f32)>, threshold: f32) -> Vec<(BoundingBox, f32)> {
    // Sort by confidence descending
    boxes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut keep = Vec::new();
    let mut suppressed = vec![false; boxes.len()];

    for i in 0..boxes.len() {
        if suppressed[i] {
            continue;
        }

        keep.push(boxes[i].clone());

        for j in (i + 1)..boxes.len() {
            if suppressed[j] {
                continue;
            }

            let iou = compute_iou(&boxes[i].0, &boxes[j].0);
            if iou > threshold {
                suppressed[j] = true;
            }
        }
    }

    keep
}

/// Compute Intersection over Union between two bounding boxes
fn compute_iou(a: &BoundingBox, b: &BoundingBox) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    let intersection = ((x2 - x1).max(0) * (y2 - y1).max(0)) as f32;
    let area_a = (a.width * a.height) as f32;
    let area_b = (b.width * b.height) as f32;
    let union = area_a + area_b - intersection;

    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

/// Crop face region from image with padding
fn crop_face(img: &DynamicImage, bbox: &BoundingBox, img_width: u32, img_height: u32) -> DynamicImage {
    // Add 20% padding around the face
    let padding_x = (bbox.width as f32 * 0.2) as i32;
    let padding_y = (bbox.height as f32 * 0.2) as i32;

    let x = (bbox.x - padding_x).max(0) as u32;
    let y = (bbox.y - padding_y).max(0) as u32;
    let w = ((bbox.width + padding_x * 2) as u32).min(img_width - x);
    let h = ((bbox.height + padding_y * 2) as u32).min(img_height - y);

    img.crop_imm(x, y, w.max(1), h.max(1))
}

/// Run ArcFace embedding model
fn run_arcface_embedding(session: &mut Session, face_img: &DynamicImage) -> Result<Vec<f32>> {
    const INPUT_SIZE: u32 = 112;

    // Resize to ArcFace input size (use Triangle/bilinear for speed)
    let resized = face_img.resize_exact(INPUT_SIZE, INPUT_SIZE, image::imageops::FilterType::Triangle);
    let rgb = resized.to_rgb8();

    // Convert to tensor (NCHW format, normalized)
    let mut input_data = vec![0.0f32; (3 * INPUT_SIZE * INPUT_SIZE) as usize];

    for y in 0..INPUT_SIZE as usize {
        for x in 0..INPUT_SIZE as usize {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            let idx = y * INPUT_SIZE as usize + x;
            // ArcFace normalization: (pixel - 127.5) / 127.5
            input_data[idx] = (pixel[0] as f32 - 127.5) / 127.5;
            input_data[INPUT_SIZE as usize * INPUT_SIZE as usize + idx] = (pixel[1] as f32 - 127.5) / 127.5;
            input_data[2 * INPUT_SIZE as usize * INPUT_SIZE as usize + idx] = (pixel[2] as f32 - 127.5) / 127.5;
        }
    }

    // Create tensor
    let input_tensor = Tensor::from_array(([1usize, 3, INPUT_SIZE as usize, INPUT_SIZE as usize], input_data.into_boxed_slice()))?;

    // Run inference - ArcFace ONNX model uses "data" as input name
    let outputs = session.run(ort::inputs!["data" => input_tensor])?;

    // Get embedding output
    let embedding_output = outputs.iter().next()
        .ok_or_else(|| anyhow!("No embedding output"))?;

    let (_embedding_shape, embedding_data) = embedding_output.1
        .try_extract_tensor::<f32>()?;

    // Normalize the embedding (L2 normalization)
    let embedding_vec: Vec<f32> = embedding_data.to_vec();
    let norm: f32 = embedding_vec.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm > 0.0 {
        Ok(embedding_vec.iter().map(|x| x / norm).collect())
    } else {
        Ok(embedding_vec)
    }
}

/// Calculate cosine similarity between two face embeddings
/// Returns value between -1 and 1 (higher = more similar)
pub fn embedding_similarity(a: &[f32], b: &[f32]) -> f32 {
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

/// Calculate euclidean distance between two face embeddings
pub fn embedding_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Check if two faces are likely the same person
pub fn faces_match(embedding_a: &[f32], embedding_b: &[f32], threshold: f32) -> bool {
    embedding_similarity(embedding_a, embedding_b) > threshold
}

/// Default matching threshold for normalized embeddings (cosine similarity)
pub const DEFAULT_MATCH_THRESHOLD: f32 = 0.5;

/// Generate embedding for an existing face (given image path and bounding box)
/// This is used for on-demand embedding generation when clustering
pub fn generate_embedding_for_face(image_path: &Path, bbox: &BoundingBox) -> Result<Vec<f32>> {
    // Initialize embedding model if needed
    if EMBEDDING_MODEL.get().is_none() {
        init_embedding_model()?;
    }

    let img = load_image_for_detection(image_path)?;
    let (orig_width, orig_height) = img.dimensions();

    // Crop face region
    let face_crop = crop_face(&img, bbox, orig_width, orig_height);

    // Get embedding model
    let mut embedding_model = EMBEDDING_MODEL.get()
        .ok_or_else(|| anyhow!("Embedding model not initialized"))?
        .lock()
        .map_err(|e| anyhow!("Failed to lock embedding model: {}", e))?;

    // Generate embedding
    run_arcface_embedding(&mut *embedding_model, &face_crop)
}

/// Initialize embedding model (public for on-demand use)
pub fn ensure_embedding_model() -> Result<()> {
    init_embedding_model()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((embedding_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((embedding_similarity(&a, &c) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_iou() {
        let a = BoundingBox { x: 0, y: 0, width: 10, height: 10 };
        let b = BoundingBox { x: 0, y: 0, width: 10, height: 10 };
        assert!((compute_iou(&a, &b) - 1.0).abs() < 0.001);

        let c = BoundingBox { x: 20, y: 20, width: 10, height: 10 };
        assert!((compute_iou(&a, &c) - 0.0).abs() < 0.001);
    }
}
