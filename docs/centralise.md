# Photo Centralisation

Centralise organizes photos into a managed library with a structured Year/Month hierarchy and descriptive filenames based on metadata.

## Overview

Press `L` to centralise selected photos (or current photo if none selected).

### Result Structure

```
~/Photos/Library/
├── 2024/
│   ├── 01/
│   │   └── 20240115-1430_birthday_emma-tom_cake-cutting_001.jpg
│   └── 03/
│       └── 20240320-0900_vacation_beach-sunset_001.jpg
└── unknown/
    └── {NO_CAT}_old-photo-scan_001.jpg
```

## Filename Generation

Filenames are built from photo metadata:

| Component | Source | Example |
|-----------|--------|---------|
| Date | EXIF taken_at | `20240115` |
| Time | EXIF taken_at | `1430` |
| Event | Tags/description keywords | `birthday`, `vacation` |
| People | Face recognition | `emma-tom` |
| Description | AI description (first words) | `cake-cutting` |
| Count | Auto-increment for uniqueness | `001` |

### Example Filename

```
20240115-1430_birthday_emma-tom_cake-cutting_001.jpg
└─ date ─┴time┴─ event ┴─people─┴─description──┴count
```

### Missing Metadata

- **No date**: Goes to `unknown/` folder
- **No metadata**: Uses `{NO_CAT}_originalname_001.jpg`

## Using Centralise

### Prerequisites

1. **Scan photos** (`s`) - required for any metadata
2. **AI describe** (`i`) - recommended for better descriptions
3. **Face detection** (`F`) - optional, adds people names

### Workflow

1. Navigate to photos you want to organize
2. Select files with `Space` or visual mode (`v`)
3. Press `L` to open centralise dialog
4. Review the preview showing source → destination
5. Toggle Copy/Move with `c`
6. Press `Enter` to execute

### Dialog Controls

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate preview list |
| `c` | Toggle Copy/Move mode |
| `Enter` | Execute operation |
| `Esc` | Cancel |

## Operations

### Move (default)

- Relocates files to library
- Updates database paths automatically
- Original location becomes empty
- Faster for same-filesystem moves

### Copy

- Duplicates files to library
- Original files remain in place
- Database tracks both locations
- Use for backup workflows

## Folder Organization

### Year/Month Structure

Photos with EXIF dates are organized by when they were taken:

```
2024/
├── 01/    # January 2024
├── 02/    # February 2024
└── 12/    # December 2024
```

### Unknown Folder

Photos without dates go to `unknown/`:

```
unknown/
├── {NO_CAT}_scan-001_001.jpg
└── {NO_CAT}_old-photo_001.jpg
```

## Configuration

In `config.toml`:

```toml
[library]
# Target directory for centralised files
path = "~/Photos/Library"

# Maximum filename length (default: 200)
max_filename_length = 200
```

## Event Detection

Events are extracted from tags and descriptions:

| Keyword | Detected As |
|---------|-------------|
| birthday | `birthday` |
| wedding | `wedding` |
| vacation, trip, travel | `vacation` |
| holiday, christmas, easter | respective event |
| graduation | `graduation` |
| party, concert | respective event |
| family | `family` |

## Skipped Files

Files are skipped if:

- Not in database (scan first)
- Already in library location
- File doesn't exist

The preview shows skipped files with reasons.

## Tips

### Better Organization

1. Scan all photos first
2. Run AI describe for context
3. Use face detection for people
4. Tag photos with event names
5. Then centralise

### Handling Duplicates

Centralise auto-increments the count (`001`, `002`) for files landing in the same folder with the same base name.

### Cross-Filesystem Moves

If source and destination are on different filesystems:
1. File is copied to destination
2. Original is deleted
3. Database is updated

### Undo

There's no automatic undo. To reverse:
- For Copy: Delete the copied files
- For Move: Move files back manually

## Example Session

```bash
# 1. Navigate to photos
cd ~/Downloads/camera-import/

# 2. Start clepho
clepho

# 3. Scan directory
s

# 4. Describe with AI (optional but recommended)
i

# 5. Select all photos
v, then G to select all

# 6. Centralise
L

# 7. Review preview, press Enter to execute
```

## Limitations

- Requires photos to be scanned first
- Event detection is keyword-based
- No custom folder patterns (Year/Month only)
- No batch undo
