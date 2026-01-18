# Scanning

Scanning indexes your photos into Clepho's database, extracting metadata, generating thumbnails, and computing hashes for duplicate detection.

## Overview

When you scan a directory, Clepho:

1. Discovers all image files (based on configured extensions)
2. Extracts EXIF metadata from each file
3. Computes hash values (MD5, SHA256, perceptual)
4. Generates thumbnails for preview
5. Records file modification times for change detection
6. Stores everything in the SQLite database

## Starting a Scan

### Manual Scan

Press `s` in normal mode to scan the current directory.

```
Scanning /home/user/Photos...
[████████████████████████████████████████] 100% (1234/1234)
Scan complete: 1234 scanned, 1200 new, 34 updated
```

### Background Processing

Scans run in the background, allowing you to continue browsing:

- Status bar shows progress: `[S:45%]`
- Press `T` to view task list with details
- Press `Ctrl+c` in task list to cancel

## What Gets Extracted

### File Information

| Field | Description |
|-------|-------------|
| `path` | Full file path |
| `filename` | File name only |
| `directory` | Parent directory |
| `size_bytes` | File size |
| `modified_at` | Filesystem modification time |
| `scanned_at` | When Clepho scanned the file |

### Image Metadata

| Field | Source | Example |
|-------|--------|---------|
| `width` | Image header | 4032 |
| `height` | Image header | 3024 |
| `format` | File analysis | JPEG |

### EXIF Data

| Field | EXIF Tag | Example |
|-------|----------|---------|
| `camera_make` | Make | Canon |
| `camera_model` | Model | EOS R5 |
| `lens` | LensModel | RF 24-70mm F2.8 |
| `focal_length` | FocalLength | 50.0 |
| `aperture` | FNumber | 2.8 |
| `shutter_speed` | ExposureTime | 1/250 |
| `iso` | ISOSpeedRatings | 400 |
| `taken_at` | DateTimeOriginal | 2024-01-15 14:32:00 |
| `gps_latitude` | GPSLatitude | 37.7749 |
| `gps_longitude` | GPSLongitude | -122.4194 |
| `all_exif` | All tags | JSON blob |

### Hash Values

| Hash | Purpose | Algorithm |
|------|---------|-----------|
| `md5_hash` | Quick comparison | MD5 |
| `sha256_hash` | Exact duplicate detection | SHA-256 |
| `perceptual_hash` | Similar image detection | pHash |

## Scan Behavior

### New Files

Files not in the database are fully processed:
- All metadata extracted
- All hashes computed
- Thumbnail generated

### Existing Files

Files already in the database are checked for changes:
- If `modified_at` changed: re-scan completely
- If unchanged: skip (fast)

### Recursive Scanning

Scans are **recursive** by default - all subdirectories are processed.

```
/Photos/
├── 2023/           ← Scanned
│   ├── january/    ← Scanned
│   └── february/   ← Scanned
├── 2024/           ← Scanned
└── vacation.jpg    ← Scanned
```

## Performance

### Parallel Processing

Clepho uses parallel processing for scanning:
- Multiple files processed simultaneously
- Database writes are serialized (SQLite safety)
- CPU cores utilized efficiently

### Scan Speed Factors

| Factor | Impact |
|--------|--------|
| SSD vs HDD | Major - SSD 5-10x faster |
| File count | Linear scaling |
| File sizes | Minimal (streaming) |
| EXIF complexity | Minor |
| Network storage | Significant slowdown |

### Typical Performance

| Collection Size | SSD Time | HDD Time |
|----------------|----------|----------|
| 1,000 photos | ~10 sec | ~30 sec |
| 10,000 photos | ~2 min | ~5 min |
| 100,000 photos | ~15 min | ~45 min |

## Thumbnails

### Generation

Thumbnails are generated during scanning:
- Stored in `~/.cache/clepho/thumbnails/`
- Named by content hash (deduplicates automatically)
- Default size: 256x256 pixels

### Cache Structure

```
~/.cache/clepho/thumbnails/
├── a1/
│   └── a1b2c3d4e5f6...jpg
├── b2/
│   └── b2c3d4e5f6g7...jpg
└── ...
```

### Regeneration

Thumbnails are regenerated when:
- File modification time changes
- Cache is cleared
- Thumbnail is missing

## Interrupting Scans

### Graceful Cancellation

1. Press `T` to open task list
2. Select the scan task
3. Press `c` to cancel

The scan stops after the current file, preserving all progress.

### Resuming

Simply press `s` again - already-scanned files are skipped.

## Scan Scheduling

Schedule scans for later execution:

1. Press `@` to open schedule dialog
2. Select "Directory Scan" as task type
3. Set date and time
4. Optionally set hours of operation
5. Press Enter to schedule

See [Scheduling](scheduling.md) for details.

## Database Integration

### Querying Scanned Photos

After scanning, you can:
- View metadata in preview pane
- Search by AI descriptions (`/`)
- Find duplicates (`d`)
- Export metadata (`e`)

### Re-scanning

Force a complete re-scan by:
1. Deleting the database entry (not recommended)
2. Touching the file to update mtime
3. Using change detection (`c`) after external modifications

## Troubleshooting

### Scan Stuck

If progress stops:
- Check disk space
- Check file permissions
- Cancel and check error message

### Missing Files

If files aren't being scanned:
- Check file extension is in config
- Check file permissions
- Check file isn't corrupted

### Slow Scans

Optimize slow scans:
- Use SSD storage
- Reduce thumbnail size in config
- Ensure database is on local storage

### Corrupt EXIF

Some files have malformed EXIF:
- Clepho continues with partial data
- Check `all_exif` field for raw data
- Some cameras write non-standard tags

## Best Practices

1. **Initial Scan**: Scan your entire collection once
2. **Incremental Updates**: Use change detection for ongoing management
3. **Scheduled Scans**: Set up nightly scans for auto-import folders
4. **Verify Scans**: Check status bar for completion
5. **Backup Database**: Periodically backup `clepho.db`
