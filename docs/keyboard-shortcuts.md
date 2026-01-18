# Keyboard Shortcuts

Complete reference for all keyboard shortcuts in Clepho.

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
| `v` | Enter visual selection mode |
| `V` | Select all files in directory |

### Scanning & Analysis

| Key | Action |
|-----|--------|
| `s` | Scan current directory |
| `d` | Find duplicate photos |
| `D` | Describe selected image with AI |
| `P` | Batch process all photos with AI |
| `F` | Detect faces in scanned photos |
| `/` | Open semantic search |

### File Operations

| Key | Action |
|-----|--------|
| `m` | Move selected files |
| `r` | Rename selected files |
| `e` | Export metadata |

### Dialogs & Views

| Key | Action |
|-----|--------|
| `p` | Open people (faces) dialog |
| `t` | Open trash dialog |
| `c` | Check for file changes |
| `@` | Open schedule dialog |
| `T` | Open task list |
| `?` | Show help overlay |

### External

| Key | Action |
|-----|--------|
| `Enter` (on image) | Open in external viewer |
| Right-click | Open in external viewer |

## Visual Mode

Entered with `v`:

| Key | Action |
|-----|--------|
| `j` / `k` | Extend selection down/up |
| `gg` | Extend to first |
| `G` | Extend to last |
| `Esc` | Confirm selection |

## Duplicates View

Entered with `d`:

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

Entered with `p`:

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

Entered with `t`:

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

Entered with `e`:

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
| `Shift` | Uppercase commands (`D`, `P`, `F`, `V`, `X`, `J`, `K`, `T`) |
| `Ctrl` | Page navigation (`Ctrl+f`, `Ctrl+b`, `Ctrl+d`, `Ctrl+u`) |
| None | Most commands |

## Key Conventions

### Vim-Style

- `h/j/k/l` for navigation
- `gg/G` for start/end
- `Esc` to exit modes

### Case Sensitivity

- Lowercase: common operations
- Uppercase: more significant/powerful operations
  - `d` = find duplicates, `D` = describe with AI
  - `x` = trash, `X` = permanent delete
  - `j` = move in group, `J` = move between groups

## Customization

Currently, keybindings are not user-configurable. Future versions may support custom key mappings via configuration.

## Quick Reference Card

```
NAVIGATION          OPERATIONS           VIEWS
j/k     up/down     s   scan            d  duplicates
h/l     left/right  D   AI describe     p  people
gg/G    top/bottom  P   batch AI        t  trash
~       home        F   face detect     T  tasks
                    /   search          c  changes
SELECTION           m   move            @  schedule
Space   toggle      r   rename          ?  help
v       visual      e   export
V       all         x   trash
                    X   delete
```
