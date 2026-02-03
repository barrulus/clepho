# Running Clepho

Clepho has two binaries: the **TUI** (`clepho`) for interactive use, and the **daemon** (`clepho-daemon`) for background task processing. They share the same database (SQLite or PostgreSQL) and configuration file.

## The TUI (`clepho`)

The main application. Launch it in a terminal:

```bash
clepho
```

The TUI handles all interactive features: browsing, scanning, AI descriptions, face detection, duplicate management, and more. It also has a built-in scheduler that checks for due tasks every second while running.

### Command line options

```
clepho [OPTIONS]

OPTIONS:
    --config, -c PATH                 Path to config file
    --migrate-to-postgres URL         Migrate SQLite database to PostgreSQL
    --version, -V                     Show version
    --help, -h                        Show this help message
```

## The Daemon (`clepho-daemon`)

The daemon processes scheduled tasks in the background **without** the TUI open. It polls the shared database for pending tasks and runs them headlessly.

### Command line options

```
clepho-daemon [OPTIONS]

OPTIONS:
    --once, -1          Process pending tasks once and exit
    --interval, -i N    Poll interval in seconds (default: 60)
    --config, -c PATH   Path to config file
    --help, -h          Show this help message
```

### When you need the daemon

The TUI's built-in scheduler only runs while the TUI is open. If Clepho isn't running when a task is due, the task becomes overdue and you're prompted about it next launch.

The daemon solves this. Run it if you:

- Schedule overnight scans or LLM batch jobs and don't want to leave the TUI open
- Want tasks to run reliably on a headless server or NAS
- Prefer a "set and forget" workflow where tasks always execute on time

If you only use Clepho interactively and trigger tasks manually, you don't need the daemon.

### What the daemon processes

| Task type | Description |
|-----------|-------------|
| Directory scan | Discovers new photos and indexes metadata |
| LLM batch | Generates AI descriptions for unprocessed photos |
| Face detection | Detects and clusters faces in photos |

### How the TUI and daemon cooperate

Both binaries read and write the same database (SQLite or PostgreSQL, as configured) and share the same config file (default: `~/.config/clepho/config.toml`).

```
┌────────────┐         ┌──────────────┐
│  clepho    │         │clepho-daemon │
│  (TUI)     │         │ (background) │
└─────┬──────┘         └──────┬───────┘
      │                       │
      │   ┌───────────────┐   │
      └──►│  Database     │◄──┘
          │  config.toml  │
          └───────────────┘
```

The workflow:

1. **Schedule tasks** from the TUI (press `@` to open the schedule dialog)
2. **The daemon picks them up** on its next poll cycle and executes them
3. **Results appear in the TUI** next time you browse — new photos are indexed, descriptions are populated, faces are detected

You can run both simultaneously. SQLite handles concurrent access via its built-in locking. Avoid running multiple daemon instances, as they would compete for the same tasks.

## Running the Daemon

### Foreground (manual)

```bash
# Run continuously, polling every 60 seconds (default)
clepho-daemon

# Poll every 5 minutes instead
clepho-daemon --interval 300

# Process pending tasks once and exit
clepho-daemon --once
```

### systemd service (Linux)

The repository includes a service file at `clepho.service`.

#### Installation

```bash
# Copy the service file
sudo cp clepho.service /etc/systemd/system/

# Edit to run as your user instead of root
sudo systemctl edit clepho
```

Add the following override (replace `youruser` with your username):

```ini
[Service]
User=youruser
Group=youruser
```

Then enable and start:

```bash
sudo systemctl enable --now clepho
```

#### Monitoring

```bash
# Check status
sudo systemctl status clepho

# Follow logs
journalctl -u clepho -f

# View recent logs
journalctl -u clepho --since "1 hour ago"
```

#### Service configuration

The service file includes security hardening:

- `ProtectHome=read-only` — read-only access to `/home` (with `ReadWritePaths` exceptions)
- `ProtectSystem=strict` — read-only root filesystem
- `NoNewPrivileges=yes` — prevents privilege escalation
- `MemoryMax=2G` / `CPUQuota=50%` — resource limits

If the daemon needs write access to additional directories (e.g. a mounted drive), add them to the service override:

```ini
[Service]
ReadWritePaths=/mnt/photos
```

### User systemd service (without root)

If you prefer not to use a system-wide service:

```bash
mkdir -p ~/.config/systemd/user/

cat > ~/.config/systemd/user/clepho-daemon.service << 'EOF'
[Unit]
Description=Clepho Background Task Processor

[Service]
Type=simple
ExecStart=%h/.cargo/bin/clepho-daemon
# Or if installed via Nix:
# ExecStart=%h/.nix-profile/bin/clepho-daemon
Restart=on-failure
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

systemctl --user enable --now clepho-daemon
systemctl --user status clepho-daemon
journalctl --user -u clepho-daemon -f
```

To ensure user services run without an active login session:

```bash
sudo loginctl enable-linger youruser
```

## NixOS

### Packages

The flake provides both binaries in a single package. Install with any of these methods:

```bash
# Run directly (no install)
nix run github:barrulus/clepho

# Install to user profile
nix profile install github:barrulus/clepho
```

Or add to your NixOS configuration or Home Manager — see [installation.md](installation.md) for details.

### NixOS systemd service module

NixOS manages systemd services declaratively. Add a service for the daemon in your NixOS configuration:

```nix
{ inputs, pkgs, ... }:

let
  clephoPkg = inputs.clepho.packages.${pkgs.system}.default;
in
{
  # Install the TUI
  environment.systemPackages = [ clephoPkg ];

  # Run the daemon as a systemd service
  systemd.services.clepho-daemon = {
    description = "Clepho Background Task Processor";
    after = [ "network.target" ];
    wantedBy = [ "multi-user.target" ];

    serviceConfig = {
      Type = "simple";
      ExecStart = "${clephoPkg}/bin/clepho-daemon";
      Restart = "on-failure";
      RestartSec = 10;

      # Run as your user
      User = "youruser";
      Group = "users";

      # Resource limits
      MemoryMax = "2G";
      CPUQuota = "50%";

      # Security hardening
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = "read-only";
      ReadWritePaths = [
        "/home/youruser/.local/share/clepho"
        "/home/youruser/.config/clepho"
        "/home/youruser/.cache/clepho"
      ];
    };

    environment = {
      RUST_LOG = "info";
    };
  };
}
```

After adding this to your configuration, rebuild:

```bash
sudo nixos-rebuild switch
```

### Home Manager user service

If you manage your user environment with Home Manager, define the daemon as a user service:

```nix
{ inputs, pkgs, ... }:

let
  clephoPkg = inputs.clepho.packages.${pkgs.system}.default;
in
{
  home.packages = [ clephoPkg ];

  systemd.user.services.clepho-daemon = {
    Unit = {
      Description = "Clepho Background Task Processor";
    };

    Service = {
      Type = "simple";
      ExecStart = "${clephoPkg}/bin/clepho-daemon";
      Restart = "on-failure";
      RestartSec = 10;
      Environment = [ "RUST_LOG=info" ];
    };

    Install = {
      WantedBy = [ "default.target" ];
    };
  };
}
```

## Environment Variables

Both binaries support:

| Variable | Description |
|----------|-------------|
| `CLEPHO_CONFIG` | Path to config file (overrides default location) |
| `RUST_LOG` | Log level: `trace`, `debug`, `info`, `warn`, `error` |

## Logging

| Binary | Default log destination |
|--------|------------------------|
| `clepho` | `~/.config/clepho/logs/` |
| `clepho-daemon` | journald (Linux), stderr (fallback) |

To increase log verbosity:

```bash
RUST_LOG=debug clepho-daemon
```

## Typical Setups

### Interactive only (no daemon)

Run `clepho` when you want to browse and manage photos. Trigger scans, LLM descriptions, and face detection manually from the TUI. Scheduled tasks run while the TUI is open; missed tasks prompt on next launch.

### TUI + daemon service

Install the daemon as a systemd service (system or user). Schedule heavy tasks (overnight scans, batch LLM processing) from the TUI. The daemon executes them on schedule regardless of whether the TUI is open. Use the TUI for browsing and interactive work.

### Headless (daemon only)

On a NAS or server with no display, run only the daemon. Schedule tasks by inserting them into the database directly, or use the TUI over SSH to schedule them, then let the daemon handle execution.
