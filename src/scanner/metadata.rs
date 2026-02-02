use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct ImageMetadata {
    // Image dimensions
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,

    // Camera info
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,

    // Exposure settings
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,

    // Date/time
    pub taken_at: Option<String>,

    // GPS
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,

    // EXIF Orientation (1-8, where 1 = normal, 6 = rotated 90 CW, etc.)
    pub orientation: Option<u16>,

    // Complete EXIF data as JSON string
    pub all_exif: Option<String>,
}

/// EXIF Orientation values
/// 1 = Normal (no rotation)
/// 2 = Horizontal flip
/// 3 = Rotate 180
/// 4 = Vertical flip
/// 5 = Transpose (flip + rotate 90 CCW)
/// 6 = Rotate 90 CW
/// 7 = Transverse (flip + rotate 90 CW)
/// 8 = Rotate 90 CCW
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ImageOrientation {
    #[default]
    Normal = 1,
    FlipHorizontal = 2,
    Rotate180 = 3,
    FlipVertical = 4,
    Transpose = 5,
    Rotate90CW = 6,
    Transverse = 7,
    Rotate90CCW = 8,
}

#[allow(dead_code)]
impl ImageOrientation {
    pub fn from_exif(value: u16) -> Self {
        match value {
            1 => Self::Normal,
            2 => Self::FlipHorizontal,
            3 => Self::Rotate180,
            4 => Self::FlipVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90CW,
            7 => Self::Transverse,
            8 => Self::Rotate90CCW,
            _ => Self::Normal,
        }
    }

    pub fn as_u16(&self) -> u16 {
        *self as u16
    }

    /// Get the rotation angle in degrees (clockwise)
    pub fn rotation_degrees(&self) -> i32 {
        match self {
            Self::Normal | Self::FlipHorizontal => 0,
            Self::Rotate180 | Self::FlipVertical => 180,
            Self::Transpose | Self::Rotate90CW => 90,
            Self::Transverse | Self::Rotate90CCW => 270,
        }
    }

    /// Rotate clockwise by 90 degrees
    pub fn rotate_cw(&self) -> Self {
        match self {
            Self::Normal => Self::Rotate90CW,
            Self::Rotate90CW => Self::Rotate180,
            Self::Rotate180 => Self::Rotate90CCW,
            Self::Rotate90CCW => Self::Normal,
            // For flipped images, adjust accordingly
            Self::FlipHorizontal => Self::Transverse,
            Self::Transverse => Self::FlipVertical,
            Self::FlipVertical => Self::Transpose,
            Self::Transpose => Self::FlipHorizontal,
        }
    }

    /// Rotate counter-clockwise by 90 degrees
    pub fn rotate_ccw(&self) -> Self {
        match self {
            Self::Normal => Self::Rotate90CCW,
            Self::Rotate90CCW => Self::Rotate180,
            Self::Rotate180 => Self::Rotate90CW,
            Self::Rotate90CW => Self::Normal,
            // For flipped images
            Self::FlipHorizontal => Self::Transpose,
            Self::Transpose => Self::FlipVertical,
            Self::FlipVertical => Self::Transverse,
            Self::Transverse => Self::FlipHorizontal,
        }
    }
}

pub fn extract_metadata(path: &PathBuf) -> Result<ImageMetadata> {
    let mut metadata = ImageMetadata::default();

    // Get image format
    if let Ok(reader) = image::ImageReader::open(path) {
        if let Some(format) = reader.format() {
            metadata.format = Some(format!("{:?}", format));
        }
    }

    // Get image dimensions (open again since into_dimensions consumes the reader)
    if let Ok(reader) = image::ImageReader::open(path) {
        if let Ok(dims) = reader.into_dimensions() {
            metadata.width = Some(dims.0);
            metadata.height = Some(dims.1);
        }
    }

    // Extract EXIF data
    if let Ok(file) = File::open(path) {
        let mut bufreader = BufReader::new(file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut bufreader) {
            // Camera make
            if let Some(field) = exif.get_field(exif::Tag::Make, exif::In::PRIMARY) {
                metadata.camera_make = Some(field.display_value().to_string().trim_matches('"').to_string());
            }

            // Camera model
            if let Some(field) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
                metadata.camera_model = Some(field.display_value().to_string().trim_matches('"').to_string());
            }

            // Lens model
            if let Some(field) = exif.get_field(exif::Tag::LensModel, exif::In::PRIMARY) {
                metadata.lens = Some(field.display_value().to_string().trim_matches('"').to_string());
            }

            // Focal length
            if let Some(field) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
                if let exif::Value::Rational(ref v) = field.value {
                    if let Some(r) = v.first() {
                        metadata.focal_length = Some(r.num as f64 / r.denom as f64);
                    }
                }
            }

            // Aperture (FNumber)
            if let Some(field) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
                if let exif::Value::Rational(ref v) = field.value {
                    if let Some(r) = v.first() {
                        metadata.aperture = Some(r.num as f64 / r.denom as f64);
                    }
                }
            }

            // Shutter speed
            if let Some(field) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
                metadata.shutter_speed = Some(field.display_value().to_string());
            }

            // ISO
            if let Some(field) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
                if let exif::Value::Short(ref v) = field.value {
                    if let Some(&iso) = v.first() {
                        metadata.iso = Some(iso as i32);
                    }
                }
            }

            // Date taken
            if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                metadata.taken_at = Some(field.display_value().to_string().trim_matches('"').to_string());
            }

            // Orientation
            if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
                if let exif::Value::Short(ref v) = field.value {
                    if let Some(&orientation) = v.first() {
                        metadata.orientation = Some(orientation);
                    }
                }
            }

            // GPS coordinates
            if let (Some(lat_field), Some(lat_ref), Some(lon_field), Some(lon_ref)) = (
                exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY),
                exif.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY),
                exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY),
                exif.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY),
            ) {
                if let (exif::Value::Rational(lat_vals), exif::Value::Rational(lon_vals)) =
                    (&lat_field.value, &lon_field.value)
                {
                    if lat_vals.len() >= 3 && lon_vals.len() >= 3 {
                        let lat = dms_to_decimal(
                            lat_vals[0].num as f64 / lat_vals[0].denom as f64,
                            lat_vals[1].num as f64 / lat_vals[1].denom as f64,
                            lat_vals[2].num as f64 / lat_vals[2].denom as f64,
                        );
                        let lon = dms_to_decimal(
                            lon_vals[0].num as f64 / lon_vals[0].denom as f64,
                            lon_vals[1].num as f64 / lon_vals[1].denom as f64,
                            lon_vals[2].num as f64 / lon_vals[2].denom as f64,
                        );

                        let lat_ref_str = lat_ref.display_value().to_string();
                        let lon_ref_str = lon_ref.display_value().to_string();

                        metadata.gps_latitude = Some(if lat_ref_str.contains('S') { -lat } else { lat });
                        metadata.gps_longitude = Some(if lon_ref_str.contains('W') { -lon } else { lon });
                    }
                }
            }

            // Extract all EXIF fields as JSON
            metadata.all_exif = extract_all_exif(&exif);
        }
    }

    Ok(metadata)
}

/// Extract all EXIF fields from the image and serialize to JSON
fn extract_all_exif(exif: &exif::Exif) -> Option<String> {
    use exif::In;

    let mut all_fields: HashMap<String, serde_json::Value> = HashMap::new();

    for field in exif.fields() {
        let tag_name = field.tag.to_string();
        let ifd = match field.ifd_num {
            In::PRIMARY => "primary",
            In::THUMBNAIL => "thumbnail",
            _ => "other",
        };

        let key = format!("{}:{}", ifd, tag_name);
        let value = serialize_exif_value(&field.value);
        all_fields.insert(key, value);
    }

    serde_json::to_string(&all_fields).ok()
}

/// Serialize an EXIF value to a JSON value
fn serialize_exif_value(value: &exif::Value) -> serde_json::Value {
    use exif::Value;
    use serde_json::json;

    match value {
        Value::Byte(v) => json!(v),
        Value::Ascii(v) => {
            let strings: Vec<String> = v.iter()
                .map(|b| String::from_utf8_lossy(b).to_string())
                .collect();
            if strings.len() == 1 {
                json!(strings[0])
            } else {
                json!(strings)
            }
        }
        Value::Short(v) => {
            if v.len() == 1 {
                json!(v[0])
            } else {
                json!(v)
            }
        }
        Value::Long(v) => {
            if v.len() == 1 {
                json!(v[0])
            } else {
                json!(v)
            }
        }
        Value::Rational(v) => {
            let floats: Vec<f64> = v.iter().map(|r| r.num as f64 / r.denom as f64).collect();
            if floats.len() == 1 {
                json!(floats[0])
            } else {
                json!(floats)
            }
        }
        Value::SByte(v) => json!(v),
        Value::Undefined(v, _) => {
            // Skip large binary blobs, just note the size
            if v.len() > 1024 {
                json!({"type": "binary", "size": v.len()})
            } else {
                json!(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, v))
            }
        }
        Value::SShort(v) => {
            if v.len() == 1 {
                json!(v[0])
            } else {
                json!(v)
            }
        }
        Value::SLong(v) => {
            if v.len() == 1 {
                json!(v[0])
            } else {
                json!(v)
            }
        }
        Value::SRational(v) => {
            let floats: Vec<f64> = v.iter().map(|r| r.num as f64 / r.denom as f64).collect();
            if floats.len() == 1 {
                json!(floats[0])
            } else {
                json!(floats)
            }
        }
        Value::Float(v) => {
            if v.len() == 1 {
                json!(v[0])
            } else {
                json!(v)
            }
        }
        Value::Double(v) => {
            if v.len() == 1 {
                json!(v[0])
            } else {
                json!(v)
            }
        }
        Value::Unknown(t, c, o) => json!({"unknown_type": t, "count": c, "offset": o}),
    }
}

fn dms_to_decimal(degrees: f64, minutes: f64, seconds: f64) -> f64 {
    degrees + minutes / 60.0 + seconds / 3600.0
}
