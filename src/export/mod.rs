use anyhow::Result;
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::db::Database;

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Csv,
    Html,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
            ExportFormat::Html => "html",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ExportFormat::Json => "JSON",
            ExportFormat::Csv => "CSV",
            ExportFormat::Html => "HTML",
        }
    }
}

/// Photo data for export
#[derive(Debug, Serialize)]
pub struct ExportedPhoto {
    pub path: String,
    pub filename: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size: Option<u64>,
    pub sha256: Option<String>,
    pub perceptual_hash: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub date_taken: Option<String>,
    pub description: Option<String>,
    pub scanned_at: Option<String>,
}

/// Export photos from database to a file
pub fn export_photos(db: &Database, output_path: &Path, format: ExportFormat) -> Result<usize> {
    let photos = get_photos_for_export(db)?;
    let count = photos.len();

    match format {
        ExportFormat::Json => export_json(&photos, output_path)?,
        ExportFormat::Csv => export_csv(&photos, output_path)?,
        ExportFormat::Html => export_html(&photos, output_path)?,
    }

    Ok(count)
}

fn get_photos_for_export(db: &Database) -> Result<Vec<ExportedPhoto>> {
    let mut stmt = db.conn.prepare(
        r#"
        SELECT
            path,
            width,
            height,
            file_size,
            sha256,
            perceptual_hash,
            camera_make,
            camera_model,
            date_taken,
            description,
            scanned_at
        FROM photos
        ORDER BY path
        "#,
    )?;

    let photos = stmt
        .query_map([], |row| {
            let path: String = row.get(0)?;
            let filename = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            Ok(ExportedPhoto {
                path,
                filename,
                width: row.get(1)?,
                height: row.get(2)?,
                file_size: row.get(3)?,
                sha256: row.get(4)?,
                perceptual_hash: row.get(5)?,
                camera_make: row.get(6)?,
                camera_model: row.get(7)?,
                date_taken: row.get(8)?,
                description: row.get(9)?,
                scanned_at: row.get(10)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(photos)
}

fn export_json(photos: &[ExportedPhoto], output_path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(photos)?;
    let mut file = File::create(output_path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

fn export_csv(photos: &[ExportedPhoto], output_path: &Path) -> Result<()> {
    let mut wtr = csv::Writer::from_path(output_path)?;

    // Write headers
    wtr.write_record([
        "path",
        "filename",
        "width",
        "height",
        "file_size",
        "sha256",
        "perceptual_hash",
        "camera_make",
        "camera_model",
        "date_taken",
        "description",
        "scanned_at",
    ])?;

    // Write data
    for photo in photos {
        wtr.write_record([
            &photo.path,
            &photo.filename,
            &photo.width.map(|v| v.to_string()).unwrap_or_default(),
            &photo.height.map(|v| v.to_string()).unwrap_or_default(),
            &photo.file_size.map(|v| v.to_string()).unwrap_or_default(),
            photo.sha256.as_deref().unwrap_or(""),
            photo.perceptual_hash.as_deref().unwrap_or(""),
            photo.camera_make.as_deref().unwrap_or(""),
            photo.camera_model.as_deref().unwrap_or(""),
            photo.date_taken.as_deref().unwrap_or(""),
            photo.description.as_deref().unwrap_or(""),
            photo.scanned_at.as_deref().unwrap_or(""),
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

fn export_html(photos: &[ExportedPhoto], output_path: &Path) -> Result<()> {
    let mut html = String::new();

    // HTML header
    html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Clepho Photo Export</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #1a1a1a;
            color: #e0e0e0;
        }
        h1 {
            color: #4fc3f7;
            border-bottom: 2px solid #4fc3f7;
            padding-bottom: 10px;
        }
        .stats {
            background: #2d2d2d;
            padding: 15px;
            border-radius: 8px;
            margin-bottom: 20px;
        }
        .photo-grid {
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
            gap: 20px;
        }
        .photo-card {
            background: #2d2d2d;
            border-radius: 8px;
            padding: 15px;
            border: 1px solid #404040;
        }
        .photo-card h3 {
            color: #81c784;
            margin: 0 0 10px 0;
            font-size: 14px;
            word-break: break-all;
        }
        .photo-card .path {
            font-size: 12px;
            color: #888;
            word-break: break-all;
            margin-bottom: 10px;
        }
        .photo-card .metadata {
            font-size: 13px;
            line-height: 1.6;
        }
        .photo-card .metadata span {
            color: #888;
        }
        .photo-card .description {
            margin-top: 10px;
            padding-top: 10px;
            border-top: 1px solid #404040;
            font-style: italic;
            color: #b0b0b0;
        }
        table {
            width: 100%;
            border-collapse: collapse;
            margin-top: 20px;
        }
        th, td {
            padding: 10px;
            text-align: left;
            border-bottom: 1px solid #404040;
        }
        th {
            background: #2d2d2d;
            color: #4fc3f7;
        }
        tr:hover {
            background: #333;
        }
    </style>
</head>
<body>
    <h1>Clepho Photo Export</h1>
"#);

    // Stats section
    html.push_str(&format!(
        r#"    <div class="stats">
        <strong>Total Photos:</strong> {}
    </div>
"#,
        photos.len()
    ));

    // Photo cards
    html.push_str(r#"    <div class="photo-grid">
"#);

    for photo in photos {
        html.push_str(r#"        <div class="photo-card">
"#);
        html.push_str(&format!(
            r#"            <h3>{}</h3>
"#,
            html_escape(&photo.filename)
        ));
        html.push_str(&format!(
            r#"            <div class="path">{}</div>
"#,
            html_escape(&photo.path)
        ));
        html.push_str(r#"            <div class="metadata">
"#);

        if let (Some(w), Some(h)) = (photo.width, photo.height) {
            html.push_str(&format!(
                r#"                <div><span>Dimensions:</span> {}x{}</div>
"#,
                w, h
            ));
        }

        if let Some(size) = photo.file_size {
            html.push_str(&format!(
                r#"                <div><span>Size:</span> {}</div>
"#,
                format_size(size)
            ));
        }

        if let Some(ref camera) = photo.camera_make {
            let model = photo.camera_model.as_deref().unwrap_or("");
            html.push_str(&format!(
                r#"                <div><span>Camera:</span> {} {}</div>
"#,
                html_escape(camera),
                html_escape(model)
            ));
        }

        if let Some(ref date) = photo.date_taken {
            html.push_str(&format!(
                r#"                <div><span>Taken:</span> {}</div>
"#,
                html_escape(date)
            ));
        }

        html.push_str(r#"            </div>
"#);

        if let Some(ref desc) = photo.description {
            html.push_str(&format!(
                r#"            <div class="description">{}</div>
"#,
                html_escape(desc)
            ));
        }

        html.push_str(r#"        </div>
"#);
    }

    html.push_str(r#"    </div>
</body>
</html>
"#);

    let mut file = File::create(output_path)?;
    file.write_all(html.as_bytes())?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{} bytes", size)
    }
}
