//! EXIF stage: extract date, GPS, camera metadata into singleton columns on `photos`.

use crate::db::clock::Clock;
use crate::pipeline::stages::{pending_default, PendingPhoto, Stage, StageId, StageOutcome};
use anyhow::Result;
use rusqlite::Connection;
use std::fs::File;
use std::io::BufReader;

#[allow(dead_code)]
pub struct ExifStage;

impl Stage for ExifStage {
    fn id(&self) -> StageId {
        StageId::Exif
    }

    fn pending(
        &self,
        conn: &Connection,
        folder: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingPhoto>> {
        pending_default(conn, "exif_done_at", Some("scan_done_at"), folder, limit)
    }

    fn process_one(
        &self,
        conn: &Connection,
        photo: &PendingPhoto,
        _clock: &dyn Clock,
    ) -> Result<StageOutcome> {
        let f = match File::open(&photo.path) {
            Ok(f) => f,
            Err(e) => {
                return Ok(StageOutcome::Err {
                    error_class: "fs_missing".into(),
                    message: format!("open: {}", e),
                });
            }
        };

        let mut br = BufReader::new(f);
        let parsed = exif::Reader::new().read_from_container(&mut br);

        // No EXIF is not an error — clean negative.
        let mut taken_at: Option<String> = None;
        let mut gps_lat: Option<f64> = None;
        let mut gps_lon: Option<f64> = None;
        let mut gps_alt: Option<f64> = None;
        let mut camera_make: Option<String> = None;
        let mut camera_model: Option<String> = None;
        let mut camera_lens: Option<String> = None;

        if let Ok(reader) = parsed {
            taken_at = field_string(&reader, exif::Tag::DateTimeOriginal)
                .and_then(|s| normalise_exif_datetime(&s));
            gps_lat = gps_decimal(
                &reader,
                exif::Tag::GPSLatitude,
                exif::Tag::GPSLatitudeRef,
                ['N', 'S'],
            );
            gps_lon = gps_decimal(
                &reader,
                exif::Tag::GPSLongitude,
                exif::Tag::GPSLongitudeRef,
                ['E', 'W'],
            );
            gps_alt = field_string(&reader, exif::Tag::GPSAltitude)
                .and_then(|s| s.parse::<f64>().ok());
            camera_make = field_string(&reader, exif::Tag::Make);
            camera_model = field_string(&reader, exif::Tag::Model);
            camera_lens = field_string(&reader, exif::Tag::LensModel)
                .or_else(|| field_string(&reader, exif::Tag::LensMake));
        }

        conn.execute(
            "UPDATE photos
             SET taken_at=?1, gps_lat=?2, gps_lon=?3, gps_alt=?4,
                 camera_make=?5, camera_model=?6, camera_lens=?7,
                 updated_at=CURRENT_TIMESTAMP
             WHERE id=?8",
            rusqlite::params![
                taken_at,
                gps_lat,
                gps_lon,
                gps_alt,
                camera_make,
                camera_model,
                camera_lens,
                photo.id
            ],
        )?;
        Ok(StageOutcome::Ok)
    }
}

fn field_string(reader: &exif::Exif, tag: exif::Tag) -> Option<String> {
    reader
        .get_field(tag, exif::In::PRIMARY)
        .map(|f| {
            f.display_value()
                .with_unit(reader)
                .to_string()
                .trim_matches('"')
                .to_string()
        })
        .filter(|s| !s.is_empty())
}

/// Convert "YYYY:MM:DD HH:MM:SS" (EXIF) to "YYYY-MM-DDTHH:MM:SS" (ISO-ish).
fn normalise_exif_datetime(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let (date, time) = s.split_at(10);
    let date = date.replace(':', "-");
    Some(format!("{}T{}", date, &time[1..]))
}

fn gps_decimal(
    reader: &exif::Exif,
    coord: exif::Tag,
    refr: exif::Tag,
    signs: [char; 2],
) -> Option<f64> {
    let coord_field = reader.get_field(coord, exif::In::PRIMARY)?;
    let ref_field = reader.get_field(refr, exif::In::PRIMARY)?;

    let dms = if let exif::Value::Rational(ref v) = coord_field.value {
        if v.len() < 3 {
            return None;
        }
        let d = v[0].num as f64 / v[0].denom as f64;
        let m = v[1].num as f64 / v[1].denom as f64;
        let s = v[2].num as f64 / v[2].denom as f64;
        d + m / 60.0 + s / 3600.0
    } else {
        return None;
    };

    let ref_str = ref_field.display_value().to_string();
    let neg = ref_str.contains(signs[1]);
    Some(if neg { -dms } else { dms })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_exif_datetime_canonical() {
        assert_eq!(
            normalise_exif_datetime("2024:06:12 19:23:45"),
            Some("2024-06-12T19:23:45".to_string())
        );
    }

    #[test]
    fn normalise_exif_datetime_too_short() {
        assert_eq!(normalise_exif_datetime("2024:06:12"), None);
    }
}
