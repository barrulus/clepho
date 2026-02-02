# Navigation

Clepho uses a three-pane file browser inspired by ranger and yazi, providing efficient keyboard-driven navigation through your photo collection.

## Layout Overview

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
├─────────────┬─────────────────────┬─────────────────────────┤
│   PARENT    │      CURRENT        │        PREVIEW          │
│   (20%)     │      (40%)          │        (40%)            │
│             │                     │                         │
│   ../       │   > photo1.jpg     │   ┌─────────────────┐   │
│   backup/   │     photo2.jpg     │   │                 │   │
│   2023/     │     photo3.png     │   │  [Image]        │   │
│ > 2024/     │     vacation/      │   │                 │   │
│             │                     │   └─────────────────┘   │
│             │                     │   Name: photo1.jpg      │
│             │                     │   Size: 2.4 MB          │
│             │                     │   Date: 2024-01-15      │
├─────────────┴─────────────────────┴─────────────────────────┤
│ ~/Photos/2024 | 1 dir, 3 files | s:scan ?:help q:quit       │
└─────────────────────────────────────────────────────────────┘
```

### Panes

| Pane | Purpose | Content |
|------|---------|---------|
| **Parent** | Shows parent directory | Directories only, highlights current |
| **Current** | Active directory listing | Files and directories |
| **Preview** | Shows selected item | Image preview or directory contents |

## Keyboard Navigation

### Basic Movement

| Key | Action |
|-----|--------|
| `j` / `↓` | Move cursor down |
| `k` / `↑` | Move cursor up |
| `h` / `←` / `Backspace` | Go to parent directory |
| `l` / `→` / `Enter` | Enter directory or select file |

### Fast Movement

| Key | Action |
|-----|--------|
| `gg` | Jump to first item |
| `G` | Jump to last item |
| `Ctrl+f` | Page down |
| `Ctrl+b` | Page up |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |

### Special Navigation

| Key | Action |
|-----|--------|
| `~` | Go to home directory |
| `-` | Go to previous directory |

## Mouse Navigation

Clepho supports full mouse interaction:

| Action | Location | Effect |
|--------|----------|--------|
| **Left click** | Parent pane | Navigate to clicked directory |
| **Left click** | Current pane (directory) | Enter directory |
| **Left click** | Current pane (file) | Select file |
| **Right click** | Any file | Open with external viewer |
| **Scroll up/down** | Parent/Current pane | Navigate list |
| **Scroll up/down** | Preview pane | Scroll preview text |

## Preview Pane

The preview pane displays different content based on selection:

### Image Files

When an image is selected:

```
┌─────────────────────────┐
│                         │
│    [Rendered Image]     │
│                         │
└─────────────────────────┘
Name: vacation_001.jpg
Size: 3.2 MB
Dimensions: 4032 x 3024
Camera: Canon EOS R5
Date: 2024-06-15 14:32:00

[AI Description if available]
A sunny beach scene with...
```

### Directories

When a directory is selected:

```
Contents of vacation/:
  ├── day1/
  ├── day2/
  ├── highlights/
  ├── photo001.jpg
  ├── photo002.jpg
  └── ... (15 more items)

3 directories, 12 files
```

### Preview Scrolling

For long descriptions or metadata:

| Key | Action |
|-----|--------|
| `{` | Scroll preview up |
| `}` | Scroll preview down |

## Visual Selection Mode

Select multiple files for batch operations:

| Key | Action |
|-----|--------|
| `Space` | Toggle selection on current file |
| `v` / `V` | Enter visual mode (select range) |
| `Ctrl+a` | Select all (in gallery view) |
| `Esc` | Clear selection / exit visual mode |

### Visual Mode

In visual mode, movement keys extend the selection:

1. Press `v` to start visual mode
2. Move with `j`/`k` to extend selection
3. Press `Esc` to confirm selection

Selected files are highlighted and can be used with:
- `m` - Move selected files
- `r` - Rename selected files
- `e` - Export metadata
- `@` - Schedule processing

## View Filtering

By default, Clepho hides dotfiles/directories and shows only supported image files. Toggle filters to see everything:

| Key | Action |
|-----|--------|
| `.` | Toggle hidden files (dotfiles) |
| `H` | Toggle show all files vs images only |

When filters are active, the status bar shows indicators like `[.*]` (hidden shown) or `[all]` (all files shown).

## Status Bar

The bottom status bar shows:

```
~/Photos/2024 | 3 dirs, 45 files | [.*,all] | [S:75%] [!2 changes] | s:scan ?:help q:quit
```

| Element | Meaning |
|---------|---------|
| Path | Current directory |
| Counts | Directory and file counts |
| `[.*]` | Hidden files visible |
| `[all]` | All files visible (not just images) |
| `[S:75%]` | Running scan at 75% |
| `[!2 changes]` | 2 file changes detected |
| Hints | Available keyboard shortcuts |

## File Display

### File Indicators

| Indicator | Meaning |
|-----------|---------|
| `/` suffix | Directory |
| `>` prefix | Currently selected |
| `*` prefix | Selected for operation |
| Cyan color | Scanned (in database) |
| White color | Not scanned |

### Sorting

Files are displayed in the following order:
1. Directories first (alphabetical)
2. Files (alphabetical)

## Terminal Compatibility

Image preview quality depends on terminal support:

| Terminal | Protocol | Preview Quality |
|----------|----------|-----------------|
| Kitty | Kitty | Full color, sharp |
| iTerm2 | iTerm2 | Full color, sharp |
| WezTerm | Kitty/Sixel | Full color, sharp |
| Konsole | Sixel | Good color |
| foot | Sixel | Good color |
| xterm | Sixel | Good (if compiled with sixel) |
| Others | Halfblocks | Basic, blocky |

### Configuring Preview Protocol

In `config.toml`:

```toml
[preview]
# Auto-detect best protocol
protocol = "auto"

# Or force a specific protocol
# protocol = "kitty"
# protocol = "sixel"
# protocol = "halfblocks"
# protocol = "none"  # Disable previews
```

## Tips

### Efficient Navigation

1. Use `gg` and `G` to quickly jump to start/end
2. Use `/` to search by description instead of scrolling
3. Use `~` to quickly return to home directory

### Working with Large Directories

1. Scan the directory first (`s`) to enable metadata display
2. Use semantic search (`/`) to find specific photos
3. Use duplicate detection (`u`) to find similar photos

### Multi-Monitor Workflow

1. Run Clepho in one terminal
2. Right-click to open photos in external viewer on another monitor
3. Use the preview pane for quick verification
