# Cross-Platform Compatibility Assessment for Clepho

## Executive Summary

**Current State**: The application is **well-positioned for cross-platform support** with approximately **70-80% of the codebase already cross-platform compatible**.

**Effort Estimate**: Medium - primarily involves creating alternative service management and proper installers.

---

## What's Already Cross-Platform (No Work Needed)

| Component | Implementation |
|-----------|---------------|
| **Core TUI** | `crossterm` + `ratatui` - works on all platforms |
| **Database** | `rusqlite` with bundled SQLite - fully portable |
| **File Operations** | Standard Rust `std::fs` APIs |
| **Directory Handling** | `dirs` crate maps to platform-appropriate paths |
| **Image Processing** | `image` crate - pure Rust |
| **EXIF Extraction** | `kamadak-exif` - pure Rust |
| **Configuration** | TOML format, platform-agnostic |
| **External Viewer** | Already has `#[cfg]` blocks for Linux/macOS/Windows |
| **Async/Threading** | `tokio` + `rayon` - cross-platform |

---

## Work Required by Priority

### HIGH Priority

#### 1. Logging Backend Alternatives
**Current**: Linux uses `tracing-journald`, others fall back to file logging.

**Status**: Already works! Falls back gracefully.

**Optional Enhancement**: Add native logging for:
- macOS: OSLog (via `tracing-oslog` crate)
- Windows: Event Log (via `tracing-etw` crate)

**Files**: `src/logging.rs`, `src/bin/daemon.rs`, `Cargo.toml`

---

#### 2. Service/Daemon Management
**Current**: `clepho.service` systemd file (Linux-only)

**Work Needed**:
| Platform | Solution | Files to Create |
|----------|----------|-----------------|
| macOS | LaunchAgent plist | `com.clepho.daemon.plist` |
| Windows | Windows Service or scheduled task | PowerShell install script |

**Alternative**: Document running daemon manually or via `supervisor`/`pm2`

**Files**: `clepho.service` (existing), new platform-specific files

---

#### 3. Installation/Distribution
**Current**: No installer, manual Rust build

**Work Needed**:
| Platform | Solution |
|----------|----------|
| Linux | AppImage, .deb, .rpm, or Flatpak |
| macOS | Homebrew formula or DMG |
| Windows | MSI installer or WinGet manifest |

**Alternative**: Provide pre-built binaries via GitHub Releases

---

### MEDIUM Priority

#### 4. ONNX Runtime Binary Verification
**Current**: Uses `download-binaries` feature to auto-fetch ONNX Runtime

**Concern**: May fail on:
- Older/exotic ARM platforms
- Air-gapped systems
- Corporate firewalls

**Work**: Test builds on Windows and macOS CI, document manual ONNX setup

**Files**: `Cargo.toml`, potentially `build.rs`

---

#### 5. Terminal Image Rendering
**Current**: Multiple protocols supported (Sixel, Kitty, iTerm2, Halfblocks)

**Concern**: Windows Terminal support is newer and may have quirks

**Work**: Test on Windows Terminal, add documentation for terminal requirements

---

### LOW Priority (Nice to Have)

#### 6. System Trash Integration
**Current**: Uses custom `~/.local/share/clepho/.trash`

**Enhancement**: Use `trash` crate to integrate with:
- Linux: FreeDesktop Trash spec
- macOS: ~/.Trash
- Windows: Recycle Bin

**Files**: `src/trash/mod.rs`, `Cargo.toml`

---

## Summary of Work Effort

| Task | Effort | Priority |
|------|--------|----------|
| Test & document current cross-platform support | 1-2 days | HIGH |
| Create macOS LaunchAgent plist | 1 day | HIGH |
| Create Windows service installer | 2-3 days | HIGH |
| Set up GitHub Actions for multi-platform builds | 1-2 days | HIGH |
| Create platform installers | 3-5 days | MEDIUM |
| Add optional native logging per platform | 1-2 days | LOW |
| Integrate system trash | 1-2 days | LOW |

**Total Estimate**: 1-2 weeks for full production-ready cross-platform support

---

## Files That Would Need Changes

```
# Core changes (minimal)
src/logging.rs          # Optional: Add macOS/Windows native logging
src/bin/daemon.rs       # Optional: Same logging changes
Cargo.toml              # Add platform-specific deps, features

# New files needed
clepho.plist            # macOS LaunchAgent
install-windows.ps1     # Windows installer script
.github/workflows/      # CI for multi-platform builds

# Optional
src/trash/mod.rs        # System trash integration
```

---

## Platform-Specific Code Locations

### Conditional Compilation (`#[cfg]`)

**File Opener** - `src/app.rs` (lines 801-823):
```rust
#[cfg(target_os = "linux")]
{ "xdg-open" }
#[cfg(target_os = "macos")]
{ "open" }
#[cfg(target_os = "windows")]
{ "start" }
```

**Logging** - `src/logging.rs`:
```rust
#[cfg(target_os = "linux")]
{
    if let Ok(journald_layer) = tracing_journald::layer() {
        // Uses systemd journald
    }
}
// Fallback to file-based logging on all platforms
```

**Platform Dependencies** - `Cargo.toml`:
```toml
[target.'cfg(target_os = "linux")'.dependencies]
tracing-journald = "0.3"
```

---

## Recommendation

The codebase is already well-architected for cross-platform support. The main work is:

1. **Testing** - Verify builds work on Windows/macOS
2. **Documentation** - Document platform-specific setup
3. **Service files** - Create alternatives to systemd
4. **Distribution** - Pre-built binaries or installers

No fundamental architectural changes are required.
