# Export

Clepho can export photo metadata to CSV or JSON formats for use in other applications, spreadsheets, or archival purposes.

## Overview

Export functionality allows you to:

1. **Export metadata** - All scanned information about photos
2. **Multiple formats** - CSV for spreadsheets, JSON for programming
3. **Selective export** - Export selected files only
4. **Full database export** - Export entire collection

## Starting Export

### Opening Export Dialog

1. Optionally select specific files (or select none for all)
2. Press `e`
3. Choose export options
4. Confirm export

### Export Dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Export Metadata                                            │
├─────────────────────────────────────────────────────────────┤
│ Files: 150 selected (or "All 5,234 photos in database")    │
├─────────────────────────────────────────────────────────────┤
│ Format:                                                    │
│   > CSV (Spreadsheet compatible)                           │
│     JSON (Structured data)                                 │
│                                                             │
│ Output: /home/user/export.csv                              │
├─────────────────────────────────────────────────────────────┤
│ j/k:format Tab:edit path Enter:export Esc:cancel          │
└─────────────────────────────────────────────────────────────┘
```

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Select format |
| `Tab` | Edit output path |
| `Enter` | Start export |
| `Esc` | Cancel |

## Export Formats

### CSV Format

Comma-separated values, compatible with:
- Microsoft Excel
- Google Sheets
- LibreOffice Calc
- Any spreadsheet application

#### CSV Structure

```csv
path,filename,directory,size_bytes,width,height,camera_make,camera_model,...
/home/user/photo.jpg,photo.jpg,/home/user,3245678,4032,3024,Canon,EOS R5,...
```

#### CSV Fields

| Field | Description |
|-------|-------------|
| `path` | Full file path |
| `filename` | File name only |
| `directory` | Parent directory |
| `size_bytes` | File size in bytes |
| `width` | Image width in pixels |
| `height` | Image height in pixels |
| `format` | Image format (JPEG, PNG, etc.) |
| `camera_make` | Camera manufacturer |
| `camera_model` | Camera model |
| `lens` | Lens model |
| `focal_length` | Focal length in mm |
| `aperture` | F-number |
| `shutter_speed` | Exposure time |
| `iso` | ISO sensitivity |
| `taken_at` | Date/time photo was taken |
| `gps_latitude` | GPS latitude |
| `gps_longitude` | GPS longitude |
| `sha256_hash` | File hash (duplicate detection) |
| `description` | AI-generated description |
| `scanned_at` | When Clepho scanned the file |

### JSON Format

Structured data format, ideal for:
- Programming and scripting
- Data processing pipelines
- Database import
- API integration

#### JSON Structure

```json
{
  "export_date": "2024-01-20T15:30:00",
  "photo_count": 150,
  "photos": [
    {
      "path": "/home/user/photo.jpg",
      "filename": "photo.jpg",
      "directory": "/home/user",
      "size_bytes": 3245678,
      "dimensions": {
        "width": 4032,
        "height": 3024
      },
      "camera": {
        "make": "Canon",
        "model": "EOS R5",
        "lens": "RF 24-70mm F2.8"
      },
      "exposure": {
        "focal_length": 50.0,
        "aperture": 2.8,
        "shutter_speed": "1/250",
        "iso": 400
      },
      "taken_at": "2024-01-15T14:32:00",
      "gps": {
        "latitude": 37.7749,
        "longitude": -122.4194
      },
      "hashes": {
        "sha256": "abc123...",
        "perceptual": "def456..."
      },
      "ai": {
        "description": "A scenic beach at sunset..."
      },
      "metadata": {
        "scanned_at": "2024-01-16T10:00:00"
      }
    }
  ]
}
```

## Selective Export

### Export Selected Files

1. Select files using `Space`, `v`, or `V`
2. Press `e`
3. Dialog shows "N selected files"
4. Export only those files

### Export All Files

1. Ensure no files are selected
2. Press `e`
3. Dialog shows "All N photos in database"
4. Exports entire database

### Export Current Directory

1. Select all in current directory (`V`)
2. Press `e`
3. Exports only current directory photos

## Output Options

### Default Location

Export files default to current directory:
```
/current/directory/export.csv
/current/directory/export.json
```

### Custom Path

1. Press `Tab` in export dialog
2. Edit the output path
3. Can specify any writable location

### Filename Conventions

Suggested naming:
```
photos_export_2024-01-20.csv
collection_backup.json
vacation_photos.csv
```

## Use Cases

### Spreadsheet Analysis

1. Export to CSV
2. Open in Excel/Sheets
3. Sort by date, camera, location
4. Create pivot tables
5. Generate statistics

### Backup Metadata

1. Export to JSON
2. Store with photo backups
3. Metadata survives if database lost
4. Can reimport in future

### Migration

1. Export entire database
2. Move to new system
3. Import into new photo manager
4. Or rebuild Clepho database

### Data Processing

```python
import json

with open('export.json') as f:
    data = json.load(f)

# Find all photos from specific camera
canon_photos = [p for p in data['photos']
                if p['camera']['make'] == 'Canon']

# Calculate average file size
avg_size = sum(p['size_bytes'] for p in data['photos']) / len(data['photos'])
```

### GPS Mapping

1. Export to CSV with GPS data
2. Import into mapping tool
3. Visualize photo locations
4. Create travel maps

## Export Contents

### What's Included

- All metadata from database
- File paths and sizes
- EXIF data (camera, settings, dates)
- GPS coordinates
- Hash values
- AI descriptions
- Scan timestamps

### What's NOT Included

- Actual image files
- Thumbnail images
- Face detection data (separate export TBD)
- Scheduled task history

## Performance

### Export Speed

| Collection Size | CSV Time | JSON Time |
|----------------|----------|-----------|
| 1,000 photos | < 1 sec | < 1 sec |
| 10,000 photos | ~2 sec | ~3 sec |
| 100,000 photos | ~15 sec | ~20 sec |

### File Sizes

| Photos | CSV Size | JSON Size |
|--------|----------|-----------|
| 1,000 | ~500 KB | ~1 MB |
| 10,000 | ~5 MB | ~10 MB |
| 100,000 | ~50 MB | ~100 MB |

## Tips

### Regular Backups

Schedule periodic exports:
```bash
# Add to crontab
0 2 * * 0 cd ~/Photos && clepho --export json > /backup/photos_$(date +%Y%m%d).json
```
(Note: CLI export mode planned for future)

### Data Validation

After export, verify:
1. File count matches expectation
2. Open and spot-check data
3. Ensure special characters handled

### Large Exports

For very large collections:
- Use JSON for streaming parsers
- Consider splitting by year/directory
- Export during off-hours

## Troubleshooting

### Export Failed

| Error | Cause | Solution |
|-------|-------|----------|
| Permission denied | Can't write to location | Choose writable path |
| Disk full | No space | Free space or different drive |
| Path not found | Directory doesn't exist | Create directory first |

### Missing Data

If fields are empty:
- Photo may not be scanned
- EXIF data may not exist
- AI description not generated

### Encoding Issues

CSV special characters:
- Quotes are escaped
- Newlines in descriptions handled
- UTF-8 encoding used

### Large File Won't Open

If Excel struggles with large CSV:
- Use JSON with streaming parser
- Split into smaller files
- Use database tools instead

## Future Enhancements

Planned improvements:
- Face data export
- Custom field selection
- Multiple format output
- CLI export mode
- Scheduled exports
- Cloud storage export
