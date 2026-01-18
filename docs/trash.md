# Trash System

Clepho includes a safe trash system that allows you to delete files with the ability to restore them before permanent deletion.

## Overview

The trash system provides:

1. **Safe deletion** - Files moved to trash, not immediately deleted
2. **Restore capability** - Recover accidentally deleted files
3. **Auto-cleanup** - Old/excess files automatically removed
4. **Size management** - Configurable maximum trash size

## Trash Location

Default: `~/.local/share/clepho/.trash/`

Files in trash are renamed to prevent conflicts:
```
~/.local/share/clepho/.trash/
├── a1b2c3d4_photo_001.jpg
├── e5f6g7h8_vacation.jpg
└── ...
```

## Configuration

```toml
[trash]
# Trash directory location
path = "~/.local/share/clepho/.trash"

# Auto-delete files older than this (days)
max_age_days = 30

# Maximum trash size (bytes)
# When exceeded, oldest files deleted first
max_size_bytes = 1073741824  # 1GB
```

### Size Examples

| Setting | Size |
|---------|------|
| 536870912 | 512 MB |
| 1073741824 | 1 GB |
| 5368709120 | 5 GB |
| 10737418240 | 10 GB |

## Using the Trash

### Opening Trash View

Press `t` to open the trash dialog.

```
┌─────────────────────────────────────────────────────────────┐
│ Trash: 15 files | 234.5 MB / 1.0 GB (23%)                  │
├─────────────────────────────────────────────────────────────┤
│ > photo_001.jpg | 3.2 MB | 2024-01-15                      │
│   vacation_pic.jpg | 2.8 MB | 2024-01-14                   │
│   IMG_4521.jpg | 4.1 MB | 2024-01-13                       │
│   duplicate.jpg | 3.2 MB | 2024-01-12                      │
│   ...                                                       │
├─────────────────────────────────────────────────────────────┤
│ Original: /home/user/Photos/2024/photo_001.jpg             │
├─────────────────────────────────────────────────────────────┤
│ j/k:nav Enter/r:restore d:delete c:cleanup q:close         │
└─────────────────────────────────────────────────────────────┘
```

### Trash View Information

| Element | Description |
|---------|-------------|
| File count | Number of files in trash |
| Size used | Current trash size |
| Percentage | Trash fullness |
| File list | Trashed files with size and date |
| Original path | Where file came from |

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` / `↓` / `↑` | Navigate list |
| `Enter` / `r` | Restore selected file |
| `d` | Permanently delete selected |
| `c` | Run cleanup (apply age/size rules) |
| `Esc` / `q` | Close trash view |

## Moving Files to Trash

### From Duplicates View

1. Mark duplicates with `Space`
2. Press `x` to move to trash

### From File Browser

Currently, files are primarily trashed through:
- Duplicate detection (`d` → mark → `x`)
- Future: Direct delete command

## Restoring Files

### Single File Restore

1. Open trash (`t`)
2. Navigate to file
3. Press `Enter` or `r`

```
Restored: photo_001.jpg → /home/user/Photos/2024/photo_001.jpg
```

### Restore Behavior

- File restored to **original location**
- If original location doesn't exist, directory is created
- If file exists at destination, restore fails (won't overwrite)

## Permanent Deletion

### Single File

1. Open trash (`t`)
2. Navigate to file
3. Press `d`
4. File is permanently deleted

### Bypass Trash (Duplicates)

In duplicates view, press `X` (uppercase) instead of `x`:
- Files immediately and permanently deleted
- **Cannot be recovered**
- Use with caution

## Auto-Cleanup

### Age-Based Cleanup

Files older than `max_age_days` are automatically deleted:

```toml
[trash]
max_age_days = 30  # Delete files after 30 days
```

### Size-Based Cleanup

When trash exceeds `max_size_bytes`, oldest files are deleted:

```toml
[trash]
max_size_bytes = 1073741824  # 1GB limit
```

### Manual Cleanup

Press `c` in trash view to run cleanup immediately:

```
Cleanup: Removed 5 files (125.3 MB) - over age limit
Cleanup: Removed 3 files (89.2 MB) - over size limit
```

### Cleanup Trigger

Auto-cleanup runs when:
- Opening trash view
- Moving files to trash
- On Clepho startup

## Database Tracking

### Trashed File Record

When a file is trashed:
- Original path stored in database
- Trash path recorded
- Timestamp saved
- File size preserved

```sql
-- In photos table
path = '/home/user/.local/share/clepho/.trash/abc123_photo.jpg'
original_path = '/home/user/Photos/photo.jpg'
trashed_at = '2024-01-15T10:30:00'
```

### After Restore

- `path` reset to original
- `original_path` cleared
- `trashed_at` cleared

### After Permanent Delete

- Record removed from database
- File deleted from filesystem
- No recovery possible

## Workflow Examples

### Cleaning Up Duplicates

1. Find duplicates (`d`)
2. Mark lower-quality versions (`Space` or `a`)
3. Move to trash (`x`)
4. Continue with confidence (can restore)
5. After verification, let auto-cleanup handle deletion

### Recovering Mistake

1. Realize file was needed
2. Open trash (`t`)
3. Find the file
4. Press `Enter` to restore
5. File back in original location

### Aggressive Cleanup

If you're confident in deletions:

1. In duplicates view, use `X` instead of `x`
2. Or set short `max_age_days`:
   ```toml
   max_age_days = 1  # Delete after 1 day
   ```

### Conservative Cleanup

If you want extra safety:

```toml
[trash]
max_age_days = 90      # Keep 90 days
max_size_bytes = 10737418240  # 10GB limit
```

## Space Management

### Checking Trash Size

Open trash view (`t`) - size shown in header:
```
Trash: 15 files | 234.5 MB / 1.0 GB (23%)
```

### Reclaiming Space

1. Open trash (`t`)
2. Press `c` for cleanup
3. Or manually delete large files with `d`

### Monitoring Growth

If trash grows too fast:
1. Review what's being deleted
2. Adjust `max_size_bytes` if needed
3. Consider smaller `max_age_days`

## Tips

### Before Bulk Deletion

1. Do a test run with a few files
2. Verify they appear in trash
3. Test restore works
4. Then proceed with larger batches

### Trash Location

Consider using same filesystem as photos:
- Faster moves (rename vs copy)
- No cross-filesystem issues

```toml
[trash]
path = "/media/photos/.clepho-trash"
```

### Backup Strategy

Trash is **not a backup**:
- Still on same filesystem
- Subject to auto-cleanup
- Maintain separate backups

## Troubleshooting

### Restore Failed

```
Error: Cannot restore - file exists at destination
```

**Solutions:**
- Rename existing file
- Delete existing file
- Restore to different location (not yet supported)

### Trash Full

```
Warning: Trash at 95% capacity
```

**Solutions:**
- Run cleanup (`c`)
- Permanently delete unneeded files
- Increase `max_size_bytes`

### Can't Delete

```
Error: Permission denied
```

**Solutions:**
- Check file permissions
- Check trash directory permissions
- Verify disk isn't full

### Files Not in Trash

If deleted files aren't appearing:
- Check trash path in config
- Verify trash directory exists
- Check disk space
