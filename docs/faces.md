# Face Detection

Clepho can detect faces in your photos, cluster similar faces together, and let you assign names to identify people across your collection.

## Overview

Face detection in Clepho:

1. **Detection** - Find faces in scanned photos
2. **Clustering** - Group similar faces together
3. **Identification** - Assign names to face clusters
4. **Search** - Find all photos of a specific person

## Starting Face Detection

### Manual Detection

Press `F` (uppercase) to detect faces in scanned photos.

```
Detecting faces in 500 photos...
[████████████████████████████████████████] 100%
Found 847 faces in 312 photos
```

### Scheduled Detection

Schedule face detection for later:

1. Press `@` to open schedule dialog
2. Select "Face Detection"
3. Set date/time
4. Press Enter

## Face Detection Process

### What Happens

1. **Load photo** - Each scanned photo is processed
2. **Detect faces** - Neural network finds face regions
3. **Extract embedding** - Face features converted to vector
4. **Store results** - Bounding boxes and embeddings saved
5. **Cluster** - Similar faces grouped automatically

### Detection Output

For each face found:
- **Bounding box** - Location in image (x, y, width, height)
- **Confidence** - Detection confidence score
- **Embedding** - 512-dimensional feature vector

## People Management

### Opening People View

Press `p` to open the people management dialog.

```
┌─────────────────────────────────────────────────────────────┐
│ People                                                      │
├─────────────────────────────────────────────────────────────┤
│ Named People (5):                                          │
│ > John Smith (47 photos)                                   │
│   Jane Doe (32 photos)                                     │
│   Mike Johnson (28 photos)                                 │
│   Sarah Williams (15 photos)                               │
│   Unknown Child (8 photos)                                 │
│                                                             │
│ Unassigned Faces (23):                                     │
│   [Press Tab to view]                                      │
├─────────────────────────────────────────────────────────────┤
│ Tab:switch j/k:nav n:name Enter:view d:delete Esc:close   │
└─────────────────────────────────────────────────────────────┘
```

### Navigation

| Key | Action |
|-----|--------|
| `Tab` | Switch between People/Faces views |
| `j` / `k` | Navigate list |
| `Enter` | View photos for selected person |
| `n` | Name selected face/person |
| `d` | Delete selected person |
| `Esc` / `q` | Close dialog |

## Naming Faces

### Assigning Names

1. Press `p` to open people view
2. Press `Tab` to switch to unassigned faces
3. Navigate to a face cluster
4. Press `n` to name
5. Type the person's name
6. Press Enter to confirm

```
┌─────────────────────────────────────────────────────────────┐
│ Name this person:                                          │
│ > John Smith_                                              │
│                                                             │
│ [Enter to confirm, Esc to cancel]                          │
└─────────────────────────────────────────────────────────────┘
```

### Renaming People

1. Select a named person
2. Press `n`
3. Edit the name
4. Press Enter

### Merging People

If the same person appears in multiple clusters:

1. Name both clusters with the same name
2. They will be automatically merged

## Viewing Person's Photos

1. Open people view (`p`)
2. Select a person
3. Press `Enter`
4. Navigate to first photo of that person
5. Use standard navigation to browse

## Face Clustering

### How It Works

Faces are clustered based on embedding similarity:

1. **Extract embeddings** - Each face → 512-dim vector
2. **Compare distances** - Euclidean distance between vectors
3. **Group similar** - Faces within threshold grouped
4. **Form clusters** - Connected faces become a cluster

### Cluster Quality

| Distance | Relationship |
|----------|--------------|
| 0.0 - 0.4 | Same person (high confidence) |
| 0.4 - 0.6 | Likely same person |
| 0.6 - 0.8 | Possibly same person |
| 0.8+ | Different people |

### Improving Clusters

If clustering isn't accurate:

1. **Add more photos** - More examples improve matching
2. **Manual correction** - Rename mis-clustered faces
3. **Delete bad detections** - Remove false positives

## Database Storage

### Face Data

```sql
-- faces table
id              -- Unique face ID
photo_id        -- Link to photo
bbox_x, y, w, h -- Face location
embedding       -- 512-dim vector (blob)
person_id       -- Link to person (if named)
confidence      -- Detection confidence
```

### People Data

```sql
-- people table
id          -- Unique person ID
name        -- Person's name
created_at  -- When first named
updated_at  -- Last modified
```

## Performance

### Detection Speed

| Factor | Impact |
|--------|--------|
| GPU available | 10-50x faster |
| Image resolution | Higher = slower |
| Faces per image | More = slightly slower |
| Model size | Larger = slower, more accurate |

### Typical Performance

| Setup | Speed |
|-------|-------|
| GPU (CUDA) | ~0.5 sec/photo |
| GPU (Metal) | ~0.8 sec/photo |
| CPU only | ~5-10 sec/photo |

### Resource Usage

- **VRAM**: ~2-4GB for detection model
- **RAM**: ~4GB during processing
- **Disk**: ~1KB per face (embedding storage)

## Workflow Example

### Initial Setup

1. **Scan photos** (`s`)
   ```
   Scanned 5,000 photos
   ```

2. **Detect faces** (`F`)
   ```
   Found 2,341 faces in 1,876 photos
   ```

3. **Open people view** (`p`)
   - See automatically clustered faces

4. **Name key people**
   - Tab to unassigned faces
   - Name largest clusters first
   - Work through smaller clusters

5. **Find someone's photos**
   - Select named person
   - Press Enter to navigate to their photos

### Ongoing Management

1. **New photos**: Scan → Detect faces → Auto-clusters with existing
2. **New people**: Name new face clusters as they appear
3. **Corrections**: Rename mis-identified faces

## Tips

### Getting Good Results

1. **Quality photos** - Clear faces work best
2. **Multiple angles** - Helps with matching
3. **Good lighting** - Shadows reduce accuracy
4. **Face size** - Very small faces may not detect

### Handling Large Collections

1. **Process in batches** - Don't detect all at once
2. **Name as you go** - Don't wait until the end
3. **Review periodically** - Fix clustering issues early

### Privacy Considerations

- Face data stored locally only
- Embeddings can't reconstruct faces
- Delete person removes all their data
- No cloud upload of face data

## Troubleshooting

### Faces Not Detected

- Ensure photo is scanned first
- Check face isn't too small (< 50px)
- Face may be obscured or at extreme angle
- Try photos with clearer faces

### Wrong Clustering

- Same person in multiple clusters: name both the same
- Different people in one cluster: name with different names
- Verify with multiple photos before naming

### Slow Detection

- Enable GPU acceleration if available
- Process smaller batches
- Schedule for overnight processing
- Check available memory

### Detection Errors

```
Error: Failed to load face detection model
```

**Solutions:**
- Check model files are present
- Verify sufficient memory
- Try restarting Clepho
