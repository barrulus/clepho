# Clepho Documentation

Welcome to the Clepho documentation. Clepho is a terminal user interface (TUI) application for managing photo collections.

## Documentation Index

### Getting Started
- [Installation](installation.md) - Prerequisites, building, and first run
- [Configuration](configuration.md) - Complete configuration reference

### Core Features
- [Navigation](navigation.md) - Three-pane file browser and navigation
- [Scanning](scanning.md) - Photo scanning and indexing
- [Duplicates](duplicates.md) - Duplicate detection and management
- [Trash](trash.md) - Safe deletion with trash system

### AI Features
- [AI Descriptions](ai-features.md) - LLM-powered photo descriptions
- [Face Detection](faces.md) - Face detection and people management

### Automation
- [Change Detection](change-detection.md) - Detecting new and modified files
- [Scheduling](scheduling.md) - Scheduled task execution

### File Management
- [File Operations](file-operations.md) - Moving and renaming files
- [Export](export.md) - Exporting metadata to CSV/JSON

### Reference
- [Keyboard Shortcuts](keyboard-shortcuts.md) - Complete keybindings reference
- [Database](database.md) - Database schema and internals

## Quick Start

1. Build and install Clepho:
   ```bash
   cargo build --release
   ```

2. Run Clepho:
   ```bash
   ./target/release/clepho
   ```

3. Navigate to a photo directory using `h/j/k/l` keys

4. Press `s` to scan the current directory

5. Press `?` for help at any time

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     Clepho TUI                              │
├─────────────┬─────────────────────┬─────────────────────────┤
│   Parent    │      Current        │        Preview          │
│   Directory │      Directory      │        Pane             │
│             │                     │                         │
│   ../       │   > photo1.jpg     │   [Image Preview]       │
│   other/    │     photo2.jpg     │   or                    │
│             │     subdir/        │   [Metadata Display]    │
│             │                     │                         │
├─────────────┴─────────────────────┴─────────────────────────┤
│ ~/Photos | 2 dirs, 45 files | [S:50%] | s:scan ?:help q:quit│
└─────────────────────────────────────────────────────────────┘
```

## Data Flow

```
Filesystem → Scanner → Database → UI
                ↓
         Thumbnails Cache
                ↓
         Image Previews
```

## Support

For issues and feature requests, visit the project repository.
