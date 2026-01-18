# Change Detection

Clepho automatically detects new and modified files in your photo directories, alerting you when a rescan may be needed.

## Overview

Change detection helps you keep your database synchronized with your filesystem by:

1. **Detecting new files** - Files on disk but not in database
2. **Detecting modified files** - Files changed since last scan
3. **Alerting you** - Status bar indicator when changes found
4. **Easy rescan** - Quick dialog to rescan changed files

## How It Works

### Detection Process

When you enter a directory, Clepho:

1. Reads current filesystem state (files and mtimes)
2. Compares against database records
3. Identifies discrepancies
4. Updates the change indicator

### What's Detected

| Type | Condition |
|------|-----------|
| **New file** | File exists on disk but not in database |
| **Modified file** | File's mtime is newer than database record |

### What's NOT Detected

- Deleted files (file no longer on disk)
- Files in subdirectories (current directory only)
- Non-image files (filtered by extensions)

## Status Bar Indicator

When changes are detected, a red indicator appears:

```
~/Photos | 3 dirs, 45 files | [!5 changes] | s:scan c:changes ?:help q:quit
                              ^^^^^^^^^^^^
```

The number shows total changes (new + modified).

## Checking for Changes

### Automatic Check

Changes are checked automatically when:
- Entering a directory (navigation)
- Returning to a directory

### Manual Check

Press `c` to:
1. Refresh change detection
2. Open the changes dialog (if changes exist)

If no changes: `"No file changes detected"`

## Changes Dialog

Press `c` when changes are detected to open the dialog:

```
┌─────────────────────────────────────────────────────────────┐
│ File Changes                                                │
├───────────────┬─────────────────────────────────────────────┤
│ New (3)       │ Modified (2)                                │
├───────────────┴─────────────────────────────────────────────┤
│ [ ] new_photo_001.jpg                                       │
│ [x] vacation_pic.jpg                                        │
│ [ ] IMG_4521.jpg                                            │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ Tab=switch j/k=nav Space=toggle a=all Enter=rescan q=close │
└─────────────────────────────────────────────────────────────┘
```

### Dialog Tabs

| Tab | Content |
|-----|---------|
| **New** | Files not yet in database |
| **Modified** | Files changed since last scan |

### Navigation

| Key | Action |
|-----|--------|
| `Tab` | Switch between New/Modified tabs |
| `j` / `k` | Navigate file list |
| `Space` | Toggle file selection |
| `a` | Select all files (both tabs) |
| `Enter` | Rescan selected files |
| `Esc` / `q` | Close dialog |

### Selection Behavior

- **No selection**: Enter rescans ALL changed files
- **With selection**: Enter rescans only selected files

## Rescanning

### What Happens

When you press Enter in the changes dialog:

1. Selected files are rescanned
2. Metadata re-extracted
3. Hashes recalculated
4. Thumbnails regenerated (if changed)
5. Database updated
6. Changes indicator cleared

### Rescan Types

| File Type | Action |
|-----------|--------|
| New file | Full scan and insert |
| Modified file | Full scan and update |

## Configuration

Change detection uses existing scanner configuration:

```toml
[scanner]
# File extensions to detect
image_extensions = ["jpg", "jpeg", "png", "gif", "webp", ...]
```

Only files matching these extensions are considered.

## Use Cases

### Import Workflow

1. Copy new photos to a folder
2. Navigate to that folder in Clepho
3. See `[!N changes]` indicator
4. Press `c` to view and rescan

### External Editing

1. Edit a photo in external editor
2. Save changes (mtime updated)
3. Return to Clepho
4. Change detected as "Modified"
5. Rescan to update metadata/hash

### Sync with Cloud Storage

1. Photos sync from cloud
2. New files appear in directory
3. Clepho detects as "New"
4. Rescan to add to database

### Batch Import

1. Add many photos to folder
2. See `[!150 changes]`
3. Press `c`
4. Select all with `a`
5. Press Enter to scan all

## Technical Details

### Modification Time Tracking

Each photo record stores:
```sql
modified_at TEXT  -- ISO timestamp of file mtime at scan time
```

### Comparison Logic

```
For each file in directory:
  If file.extension in image_extensions:
    If file.path not in database:
      → Mark as NEW
    Else if file.mtime > database.modified_at:
      → Mark as MODIFIED
```

### Performance

Change detection is fast because:
- Only reads directory listing (not file contents)
- Only checks current directory (not recursive)
- Uses filesystem mtime (no hashing)

Typical performance: < 100ms for directories with 1000+ files

## Workflow Examples

### Daily Photo Management

```
1. Open Clepho
2. Navigate to photo import folder
3. See [!12 changes] from camera import
4. Press 'c' → Enter to scan all
5. Continue organizing
```

### After External Editing

```
1. Edit photo_001.jpg in Lightroom
2. Export updated version
3. In Clepho, navigate to folder
4. See [!1 changes] (modified)
5. Press 'c' → Enter to update
6. New hash/metadata recorded
```

### Selective Rescan

```
1. See [!50 changes]
2. Press 'c' to open dialog
3. Only want to scan JPEGs now
4. Select specific files with Space
5. Press Enter
6. Only selected files scanned
```

## Tips

### Regular Checks

- Check change indicator when entering directories
- Rescan promptly to keep database current
- Use scheduled scans for auto-import folders

### Large Imports

For many new files:
1. Check changes first (`c`)
2. Review file list
3. Use `a` to select all
4. Consider scheduling if large batch

### Dealing with False Positives

If files show as "Modified" unexpectedly:
- Filesystem may have touched mtime
- Cloud sync may update mtime
- Just rescan to update records

## Limitations

### Current Directory Only

Change detection only checks the current directory:
- Does not scan subdirectories
- Navigate into each folder to check

### No Deletion Detection

Deleted files are not detected:
- Files removed from disk remain in database
- Use duplicate detection to find orphaned records
- Manual database cleanup if needed

### Extension Filtering

Only configured extensions are checked:
- New RAW format? Add to config first
- Then change detection will find them

## Troubleshooting

### Changes Not Detected

- Verify file extension is in config
- Check file permissions (readable?)
- Ensure directory is correct

### False "Modified" Detection

- Normal after cloud sync
- Normal after filesystem operations
- Just rescan to clear

### Indicator Won't Clear

- Ensure rescan completed successfully
- Check for scan errors
- Navigate away and back
