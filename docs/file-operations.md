# File Operations

Clepho provides file management operations including moving and renaming photos with powerful pattern-based renaming.

## Selection

Before performing operations, select files:

### Single Selection

| Key | Action |
|-----|--------|
| `Space` | Toggle selection on current file |

### Visual Selection

| Key | Action |
|-----|--------|
| `v` | Enter visual mode |
| `j` / `k` | Extend selection |
| `Esc` | Confirm selection |

### Select All

| Key | Action |
|-----|--------|
| `V` | Select all files in directory |

### Selection Indicator

Selected files show `*` prefix:
```
  * photo_001.jpg
  * photo_002.jpg
    photo_003.jpg
  * photo_004.jpg
```

## Moving Files

### Starting Move Operation

1. Select files to move
2. Press `m`
3. Navigate to destination
4. Press `Enter` to confirm

### Move Dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Move 5 files to:                                           │
├─────────────────────────────────────────────────────────────┤
│ Current: /home/user/Photos/2024                            │
├─────────────────────────────────────────────────────────────┤
│   ../                                                       │
│   january/                                                  │
│ > february/                                                 │
│   march/                                                    │
│   [Create new folder...]                                   │
├─────────────────────────────────────────────────────────────┤
│ Input: _                                                   │
├─────────────────────────────────────────────────────────────┤
│ j/k:nav Enter:select/move Tab:input h:parent Esc:cancel    │
└─────────────────────────────────────────────────────────────┘
```

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate directories |
| `l` / `Enter` | Enter directory |
| `h` | Go to parent |
| `Tab` | Switch to path input |
| `Enter` (on destination) | Execute move |
| `Esc` | Cancel |

### Creating Directories

1. Press `Tab` to enter input mode
2. Type new directory name
3. Press `Enter` to create and select

### Move Behavior

- Files moved to selected destination
- Database records updated automatically
- Thumbnails preserved (hash-based)

## Renaming Files

### Starting Rename Operation

1. Select files to rename
2. Press `r`
3. Enter rename pattern
4. Review preview
5. Press `Enter` to confirm

### Rename Dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Rename 5 files                                             │
├─────────────────────────────────────────────────────────────┤
│ Pattern: vacation_{N:3}.{ext}                              │
├─────────────────────────────────────────────────────────────┤
│ Preview:                                                   │
│   IMG_4521.jpg  →  vacation_001.jpg                        │
│   IMG_4522.jpg  →  vacation_002.jpg                        │
│   IMG_4523.jpg  →  vacation_003.jpg                        │
│   IMG_4524.jpg  →  vacation_004.jpg                        │
│   IMG_4525.jpg  →  vacation_005.jpg                        │
├─────────────────────────────────────────────────────────────┤
│ Enter:rename Esc:cancel                                    │
└─────────────────────────────────────────────────────────────┘
```

### Pattern Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `{name}` | Original filename (no extension) | `IMG_4521` |
| `{ext}` | Original extension | `jpg` |
| `{N}` | Sequential number | `1`, `2`, `3`... |
| `{N:3}` | Padded number (3 digits) | `001`, `002`, `003`... |
| `{N:4}` | Padded number (4 digits) | `0001`, `0002`... |
| `{date}` | Date from EXIF | `2024-01-15` |
| `{time}` | Time from EXIF | `14-32-00` |
| `{camera}` | Camera model | `Canon_EOS_R5` |

### Pattern Examples

| Pattern | Result |
|---------|--------|
| `{name}.{ext}` | Keep original names |
| `photo_{N:3}.{ext}` | `photo_001.jpg`, `photo_002.jpg` |
| `{date}_{N:2}.{ext}` | `2024-01-15_01.jpg` |
| `vacation_{N}.{ext}` | `vacation_1.jpg`, `vacation_2.jpg` |
| `{camera}_{N:4}.{ext}` | `Canon_EOS_R5_0001.jpg` |

### Counter Start

The counter `{N}` starts at 1 by default. For custom start:
- Not yet configurable in UI
- Future: counter start option

### Rename Behavior

- Files renamed in selected order
- Database records updated
- Conflicts prevented (won't overwrite)

## Database Synchronization

### Automatic Updates

File operations automatically update the database:

| Operation | Database Action |
|-----------|-----------------|
| Move | Update `path` and `directory` |
| Rename | Update `path` and `filename` |
| Delete (via trash) | Update `path`, set `original_path` |

### Maintaining Integrity

- Thumbnails use content hash (survive moves)
- AI descriptions preserved
- Face data preserved
- All metadata retained

## Error Handling

### Move Errors

| Error | Cause | Solution |
|-------|-------|----------|
| Permission denied | No write access | Check destination permissions |
| File exists | Name conflict | Choose different destination |
| Disk full | No space | Free up space |
| Invalid path | Bad destination | Check path exists |

### Rename Errors

| Error | Cause | Solution |
|-------|-------|----------|
| Name conflict | File would overwrite | Use different pattern |
| Invalid characters | OS restrictions | Remove special characters |
| Path too long | Exceeds OS limit | Shorten names |

### Recovery

If operation fails mid-way:
- Successfully processed files are updated
- Failed files show error
- Retry for failed files

## Workflow Examples

### Organizing by Date

1. Navigate to mixed photos folder
2. Select photos from same date (`v` + navigate)
3. Press `m`
4. Navigate to/create date folder
5. Press `Enter`

### Batch Rename Import

1. Select imported camera files (`V` for all)
2. Press `r`
3. Enter pattern: `vacation_{date}_{N:3}.{ext}`
4. Review preview
5. Press `Enter`

### Reorganizing Collection

1. Find duplicates (`d`)
2. Move duplicates to trash (`x`)
3. Select remaining files
4. Move to organized folders
5. Rename with consistent pattern

## Tips

### Efficient Selection

- Use `V` to select all, then `Space` to deselect unwanted
- Use `v` for contiguous ranges
- Combine with navigation (`gg`, `G`)

### Safe Renaming

1. Always check preview before confirming
2. Use padded numbers (`{N:3}`) for sorting
3. Keep extensions: always end with `.{ext}`
4. Test pattern on few files first

### Organizing Strategy

1. Organize by date (YYYY/MM structure)
2. Or by event/project
3. Use consistent naming conventions
4. Let Clepho track metadata, not filenames

## Limitations

### Move Limitations

- Cannot move across filesystems (copy not supported)
- Cannot merge directories
- Single destination per operation

### Rename Limitations

- Counter resets per rename operation
- Limited EXIF variables currently
- Cannot undo (use caution)

### General Limitations

- Operations are synchronous (blocks UI)
- Large batches may take time
- No undo for completed operations

## Future Enhancements

Planned improvements:
- Copy operation (cross-filesystem)
- More EXIF variables in patterns
- Custom counter start
- Operation undo
- Background processing for large batches
