# Duplicate Detection

Clepho can find duplicate photos in your collection using multiple detection methods, helping you reclaim disk space while keeping the best versions.

## Overview

Duplicate detection works by comparing:

1. **Exact duplicates**: Identical files (same SHA-256 hash)
2. **Perceptual duplicates**: Visually similar images (perceptual hash)

## Starting Duplicate Detection

Press `d` in normal mode to find duplicates in scanned photos.

```
Finding duplicates...
Found 15 groups with 42 duplicate photos
```

## Duplicates View

The duplicates view shows groups of similar photos:

```
┌─────────────────────────────────────────────────────────────┐
│ Duplicates: Group 3/15 (Exact) - 3 photos                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                     │
│  │         │  │         │  │  [X]    │                     │
│  │  Photo  │  │  Photo  │  │  Photo  │                     │
│  │    1    │  │    2    │  │    3    │                     │
│  │         │  │         │  │         │                     │
│  └─────────┘  └─────────┘  └─────────┘                     │
│   > Keep       Keep        Delete                          │
│                                                             │
│  photo_001.jpg    IMG_1234.jpg    photo_001_copy.jpg       │
│  4032x3024        4032x3024       4032x3024                │
│  3.2 MB           3.2 MB          3.2 MB                   │
│  Canon EOS R5     Canon EOS R5    Canon EOS R5             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ j/k:photo J/K:group Space:mark a:auto x:trash ?:help Esc   │
└─────────────────────────────────────────────────────────────┘
```

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate between photos in current group |
| `J` / `K` | Navigate between duplicate groups |
| `h` / `l` | Also navigate photos (vim-style) |

### Marking for Deletion

| Key | Action |
|-----|--------|
| `Space` | Toggle mark on current photo |
| `a` | Auto-select duplicates (keep best quality) |
| `u` | Unmark all in current group |

### Executing Deletion

| Key | Action |
|-----|--------|
| `x` | Move marked photos to trash |
| `X` | Permanently delete marked (no trash) |

### Other Actions

| Key | Action |
|-----|--------|
| `Enter` | Open photo in external viewer |
| `?` | Show duplicates help |
| `Esc` / `q` | Exit duplicates view |

## Duplicate Types

### Exact Duplicates

Files with identical content (same SHA-256 hash):

- **Same file copied multiple times**
- **Imported twice from camera**
- **Backup copies**

These are 100% identical - safe to delete extras.

### Perceptual Duplicates

Visually similar images (similar perceptual hash):

- **Resized versions** (thumbnail vs original)
- **Recompressed** (different JPEG quality)
- **Minor edits** (cropped, color adjusted)
- **Format converted** (PNG to JPEG)

Review these carefully - they may have different quality.

## Quality Scoring

When using auto-select (`a`), Clepho ranks photos by quality:

| Factor | Weight | Best Value |
|--------|--------|------------|
| Resolution | High | Larger dimensions |
| File size | Medium | Larger (less compression) |
| Original name | Low | Camera naming patterns |

The highest-quality photo is kept, others are marked.

### Quality Score Display

```
photo_001.jpg        [Quality: 95]  ← Keep
photo_001_web.jpg    [Quality: 62]  ← Delete
photo_001_thumb.jpg  [Quality: 31]  ← Delete
```

## Similarity Threshold

Configure how similar photos must be to be grouped:

```toml
[scanner]
# Hamming distance threshold for perceptual hash
# Lower = stricter (fewer matches)
# Higher = looser (more matches)
similarity_threshold = 50
```

### Threshold Guide

| Value | Matches |
|-------|---------|
| 10-20 | Nearly identical only |
| 30-40 | Same photo, minor differences |
| 50-60 | Same photo, moderate edits (default) |
| 70-80 | Similar compositions |
| 90+ | May include false positives |

## Workflow Example

### Finding and Cleaning Duplicates

1. **Scan your collection** (`s`)
   ```
   Scanning complete: 10,000 photos
   ```

2. **Find duplicates** (`d`)
   ```
   Found 150 groups with 380 duplicate photos
   ```

3. **Review groups** (`J`/`K` to navigate)
   - Check each group visually
   - Verify duplicates are actually duplicates

4. **Auto-select low quality** (`a`)
   - Marks lower-quality versions
   - Keeps highest-quality in each group

5. **Review selections**
   - Use `Space` to adjust if needed
   - Some "duplicates" may be intentional

6. **Move to trash** (`x`)
   ```
   Moved 230 files to trash
   ```

7. **Verify and permanently delete**
   - Press `t` to view trash
   - Review trashed files
   - Press `c` to cleanup or wait for auto-cleanup

## Safe Deletion

### Trash System

By default, duplicates go to trash:
- Located at `~/.local/share/clepho/.trash/`
- Can be restored via trash view (`t`)
- Auto-cleaned based on age/size settings

### Permanent Deletion

Use `X` (uppercase) for immediate permanent deletion:
- **Cannot be undone**
- Use only when certain
- Bypasses trash entirely

## Tips

### Before Deleting

1. **Backup first** - Always have backups
2. **Check quality** - Larger isn't always better
3. **Check metadata** - Some versions have better EXIF
4. **Check edits** - "Duplicates" may be intentional edits

### Handling False Positives

If unrelated photos are grouped:
- Lower the similarity threshold
- Don't mark them - skip to next group
- Consider if perceptual hashing suits your collection

### Large Collections

For collections with many duplicates:
1. Process in batches by directory
2. Use auto-select for obvious duplicates
3. Manually review perceptual matches
4. Let trash accumulate, then bulk cleanup

## Troubleshooting

### No Duplicates Found

- Ensure photos are scanned first (`s`)
- Check similarity threshold isn't too low
- Verify hash values exist in database

### Too Many False Positives

- Lower the similarity threshold
- Stick to exact duplicates only
- Review perceptual matches manually

### Missing Duplicates

- Increase similarity threshold
- Ensure all locations are scanned
- Check if files were already deleted

## Technical Details

### Perceptual Hashing

Clepho uses pHash algorithm:
1. Resize image to 32x32
2. Convert to grayscale
3. Apply DCT (discrete cosine transform)
4. Extract 64-bit hash from low frequencies

Similar images have similar hashes (low Hamming distance).

### Hash Storage

```sql
-- In photos table
sha256_hash TEXT,      -- Exact matching
perceptual_hash TEXT   -- Visual similarity (hex string)
```

### Grouping Algorithm

1. Group by identical SHA-256 (exact)
2. Compare perceptual hashes pairwise
3. Group if Hamming distance ≤ threshold
4. Merge overlapping groups
