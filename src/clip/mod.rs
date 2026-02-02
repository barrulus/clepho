//! CLIP (Contrastive Language-Image Pre-training) embeddings module
//!
//! Provides unified image embeddings for:
//! - Semantic search (text-to-image)
//! - Image similarity (image-to-image)
//! - General image understanding

mod model;

pub use model::ClipModel;
