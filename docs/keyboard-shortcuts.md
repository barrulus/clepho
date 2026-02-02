# Keyboard Shortcuts

Complete reference for all keyboard shortcuts in Clepho.

Keybindings are designed to be compatible with [Yazi](https://yazi-rs.github.io/) file manager where possible.

## Global Keys

These work in most modes:

| Key | Action |
|-----|--------|
| `q` | Quit / Close dialog |
| `Esc` | Cancel / Exit mode |
| `?` | Show help |

## Normal Mode (File Browser)

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `h` / `←` / `Backspace` | Go to parent directory |
| `l` / `→` / `Enter` | Enter directory / Open file |
| `gg` | Go to first item |
| `G` | Go to last item |
| `Ctrl+f` | Page down |
| `Ctrl+b` | Page up |
| `Ctrl+d` | Half page down |
| `Ctrl+u` | Half page up |
| `~` | Go to home directory |
| `-` | Go to previous directory |
| `{` | Scroll preview up |
| `}` | Scroll preview down |

### Selection

| Key | Action |
|-----|--------|
| `Space` | Toggle file selection |
| `v` / `V` | Enter visual selection mode |

### File Operations (Yazi-aligned)

| Key | Action |
|-----|--------|
| `y` / `x` | Yank (cut) selected files |
| `p` | Paste yanked files |
| `d` / `Delete` | Move to trash |
| `r` | Rename selected files |
| `m` | Move selected files (with dialog) |
| `]` | Rotate photo clockwise |
| `[` | Rotate photo counter-clockwise |
| `L` | Centralise files to library |

### View Filters

| Key | Action |
|-----|--------|
| `.` | Toggle hidden files/directories |
| `H` | Toggle show all files (vs images only) |

### Scanning & Analysis

| Key | Action |
|-----|--------|
| `s` | Scan current directory |
| `u` | Find duplicate photos |
| `i` | Describe selected image with AI |
| `I` | Batch process all photos with AI |
| `F` | Detect faces in scanned photos |
| `C` | Cluster similar faces |
| `E` | Generate CLIP embeddings |
| `/` | Open semantic search |

### Dialogs & Views

| Key | Action |
|-----|--------|
| `P` | Open people (faces) dialog |
| `X` | Open trash dialog |
| `c` | Check for file changes |
| `@` | Open schedule dialog |
| `T` | Open task list |
| `A` | Open gallery view |
| `S` | Open slideshow |
| `b` | Open tags dialog |
| `e` | Edit photo description |
| `O` | Export metadata |
| `?` | Show help overlay |

### External

| Key | Action |
|-----|--------|
| `o` | Open in system viewer |
| Right-click | Open in external viewer |

## Visual Mode

Entered with `v` or `V`:

| Key | Action |
|-----|--------|
| `j` / `k` | Extend selection down/up |
| `gg` | Extend to first |
| `G` | Extend to last |
| `d` / `x` / `Delete` | Trash selected |
| `y` | Yank selected |
| `Esc` | Exit visual mode |

## Gallery View

Entered with `A`:

### Navigation

| Key | Action |
|-----|--------|
| `h` / `j` / `k` / `l` | Navigate grid |
| `Arrow keys` | Navigate grid |
| `g` | Go to first image |
| `G` | Go to last image |
| `PageUp` / `Ctrl+b` | Page up |
| `PageDown` / `Ctrl+f` | Page down |

### Selection

| Key | Action |
|-----|--------|
| `Space` | Toggle selection |
| `v` / `V` | Enter visual mode |
| `Ctrl+a` | Select all |
| `Esc` | Clear selection / Exit gallery |

### Operations

| Key | Action |
|-----|--------|
| `y` / `x` | Cut selected to clipboard |
| `p` | Paste from clipboard |
| `d` / `Delete` | Move to trash |
| `]` | Rotate clockwise |
| `[` | Rotate counter-clockwise |
| `s` | Cycle sort options |
| `+` / `=` | Increase thumbnail size |
| `-` | Decrease thumbnail size |
| `S` | Open slideshow |
| `Enter` | Open in external viewer |
| `?` | Show help |
| `q` | Exit gallery |

## Duplicates View

Entered with `u`:

| Key | Action |
|-----|--------|
| `j` / `k` / `h` / `l` | Navigate photos in group |
| `J` / `K` | Navigate between groups |
| `Space` | Toggle mark for deletion |
| `a` | Auto-select (keep best quality) |
| `u` | Unmark all in group |
| `x` | Move marked to trash |
| `X` | Permanently delete marked |
| `Enter` | Open photo in viewer |
| `?` | Show duplicates help |
| `Esc` | Exit duplicates view |

## Move Dialog

Entered with `m`:

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate directories |
| `l` / `Enter` | Enter directory |
| `h` | Go to parent |
| `Tab` | Switch to path input |
| `Enter` (on destination) | Execute move |
| `Esc` | Cancel |

## Rename Dialog

Entered with `r`:

| Key | Action |
|-----|--------|
| Type | Enter rename pattern |
| `←` / `→` | Move cursor in pattern |
| `Backspace` | Delete character |
| `Enter` | Execute rename |
| `Esc` | Cancel |

## Search Dialog

Entered with `/`:

| Key | Action |
|-----|--------|
| Type | Enter search query |
| `Enter` | Execute search |
| `j` / `k` | Navigate results |
| `Enter` (on result) | Go to photo |
| `Esc` | Close search |

## People Dialog

Entered with `P`:

| Key | Action |
|-----|--------|
| `Tab` | Switch People/Faces tabs |
| `j` / `k` | Navigate list |
| `n` | Name selected face/person |
| `Enter` | View person's photos |
| `d` | Delete person |
| `Esc` | Close dialog |

### Naming Mode (in People Dialog)

| Key | Action |
|-----|--------|
| Type | Enter name |
| `←` / `→` | Move cursor |
| `Backspace` | Delete character |
| `Enter` | Confirm name |
| `Esc` | Cancel naming |

## Trash Dialog

Entered with `X`:

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate trash items |
| `Enter` / `r` | Restore selected file |
| `d` | Permanently delete selected |
| `c` | Run cleanup |
| `Esc` | Close dialog |

## Changes Dialog

Entered with `c`:

| Key | Action |
|-----|--------|
| `Tab` | Switch New/Modified tabs |
| `j` / `k` | Navigate file list |
| `Space` | Toggle file selection |
| `a` | Select all files |
| `Enter` | Rescan selected |
| `Esc` / `q` | Close dialog |

## Schedule Dialog

Entered with `@`:

| Key | Action |
|-----|--------|
| `Tab` / `j` / `↓` | Next field |
| `Shift+Tab` / `k` / `↑` | Previous field |
| `+` / `=` / `→` | Increment value |
| `-` / `←` | Decrement value |
| `Enter` | Create schedule |
| `n` | Run now |
| `Esc` / `q` | Cancel |

## Overdue Dialog

Shown on startup if overdue tasks exist:

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate tasks |
| `Space` | Toggle task selection |
| `a` | Select all tasks |
| `Enter` | Run selected tasks |
| `c` | Cancel all tasks |
| `Esc` / `q` | Dismiss |

## Task List

Entered with `T`:

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate tasks |
| `c` | Cancel selected task |
| `Esc` | Close task list |

## Export Dialog

Entered with `O`:

| Key | Action |
|-----|--------|
| `j` / `k` | Select format |
| `Tab` | Edit output path |
| `Enter` | Start export |
| `Esc` | Cancel |

## Help Overlay

Entered with `?`:

| Key | Action |
|-----|--------|
| `?` | Close help |
| `Esc` | Close help |
| `q` | Close help |

## Mouse Actions

| Action | Location | Effect |
|--------|----------|--------|
| Left click | Parent pane | Navigate to directory |
| Left click | Current pane (dir) | Enter directory |
| Left click | Current pane (file) | Select file |
| Right click | Any file | Open external viewer |
| Scroll | File panes | Navigate list |
| Scroll | Preview pane | Scroll preview |

## Modifier Keys

| Modifier | Usage |
|----------|-------|
| `Shift` | Uppercase commands (`P`, `F`, `S`, `X`, `J`, `K`, `T`, `I`, `O`) |
| `Ctrl` | Page navigation (`Ctrl+f`, `Ctrl+b`, `Ctrl+d`, `Ctrl+u`) |
| None | Most commands |

## Yazi Compatibility

Clepho's keybindings are designed to match [Yazi](https://yazi-rs.github.io/) where possible:

| Operation | Yazi | Clepho | Notes |
|-----------|------|--------|-------|
| Cut | `y` / `x` | `y` / `x` | Aligned |
| Paste | `p` | `p` | Aligned |
| Trash | `d` | `d` | Aligned |
| Rename | `r` | `r` | Aligned |
| Visual mode | `v` | `v` / `V` | Aligned |
| Toggle hidden | `.` | `.` | Aligned |
| Open external | `o` | `o` | Aligned |
| Navigation | `h/j/k/l` | `h/j/k/l` | Aligned |

## Customization

Keybindings are configurable in `config.toml`. See [configuration.md](configuration.md) for details.

## Quick Reference Card

```
NAVIGATION          FILE OPS (Yazi)      VIEWS
j/k     up/down     y/x  cut             u  duplicates
h/l     left/right  p    paste           P  people
gg/G    top/bottom  d    trash           X  trash
~       home        r    rename          T  tasks
                    m    move dialog     c  changes
SELECTION           ]/[  rotate          @  schedule
Space   toggle                           A  gallery
v/V     visual      SCANNING             S  slideshow
                    s   scan             b  tags
FILTERS             i   AI describe      ?  help
.       hidden      I   batch AI
H       all files   F   face detect      EXTERNAL
                    C   cluster faces    o  open file
                    E   CLIP embed
                    /   search
```
