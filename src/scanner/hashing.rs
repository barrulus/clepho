use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
use sha2::Sha256;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct HashResult {
    pub md5: String,
    pub sha256: String,
    pub perceptual: Option<String>,
}

pub fn calculate_hashes(path: &PathBuf) -> Result<HashResult> {
    // Calculate cryptographic hashes
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut md5_hasher = Md5::new();
    let mut sha256_hasher = Sha256::new();

    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        md5_hasher.update(&buffer[..bytes_read]);
        sha256_hasher.update(&buffer[..bytes_read]);
    }

    let md5 = format!("{:x}", md5_hasher.finalize());
    let sha256 = format!("{:x}", sha256_hasher.finalize());

    // Calculate perceptual hash for images
    let perceptual = calculate_perceptual_hash(path).ok();

    Ok(HashResult {
        md5,
        sha256,
        perceptual,
    })
}

fn calculate_perceptual_hash(path: &PathBuf) -> Result<String> {
    use img_hash::HasherConfig;

    // Open and decode the image using img_hash's re-exported image crate
    let image = img_hash::image::open(path)?;

    // Create a hasher with default configuration
    let hasher = HasherConfig::new()
        .hash_size(16, 16) // 16x16 = 256 bits for better precision
        .to_hasher();

    // Calculate the hash
    let hash = hasher.hash_image(&image);

    // Convert to base64 string for storage
    Ok(hash.to_base64())
}

pub fn hamming_distance(hash1: &str, hash2: &str) -> Result<u32> {
    use img_hash::ImageHash;

    let h1 = ImageHash::<Box<[u8]>>::from_base64(hash1)
        .map_err(|e| anyhow!("Invalid hash1: {:?}", e))?;
    let h2 = ImageHash::<Box<[u8]>>::from_base64(hash2)
        .map_err(|e| anyhow!("Invalid hash2: {:?}", e))?;

    Ok(h1.dist(&h2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_hashes() {
        // This test would need actual image files
        // For now, we just verify the function signatures are correct
    }
}
