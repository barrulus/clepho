# Scheduled Tasks

Clepho allows you to schedule tasks like scans, LLM batch processing, and face detection to run at specific times.

## Overview

The scheduling system provides:

1. **Deferred execution** - Run tasks at a future time
2. **Hours of operation** - Restrict when tasks can run
3. **Overdue handling** - Prompt for missed tasks on startup
4. **Background polling** - Automatic task triggering

## Supported Task Types

| Task Type | Description |
|-----------|-------------|
| **Directory Scan** | Scan current directory for photos |
| **LLM Batch Process** | Generate AI descriptions for photos |
| **Face Detection** | Detect and cluster faces |

## Creating a Scheduled Task

### Opening the Schedule Dialog

Press `@` to open the schedule dialog:

```
┌─────────────────────────────────────────────────────────────┐
│ Schedule Task                                               │
├─────────────────────────────────────────────────────────────┤
│ Schedule Directory Scan for: current directory              │
├─────────────────────────────────────────────────────────────┤
│ > Task Type: Directory Scan                                 │
│   Date: 2024-01-20                                         │
│   Time: 02:00                                              │
│   Hours of Operation: No                                   │
├─────────────────────────────────────────────────────────────┤
│ Tab/j/k=nav +/-=change Enter=schedule n=run now q=cancel   │
└─────────────────────────────────────────────────────────────┘
```

### Dialog Fields

| Field | Description | Controls |
|-------|-------------|----------|
| **Task Type** | What to run | `+`/`-` to cycle |
| **Date** | When to run | `+`/`-` to change day |
| **Time** | Hour to run (00-23) | `+`/`-` to change hour |
| **Hours of Operation** | Restrict execution window | `+`/`-` to toggle |
| **Start Hour** | Begin window (if enabled) | `+`/`-` to change |
| **End Hour** | End window (if enabled) | `+`/`-` to change |

### Navigation

| Key | Action |
|-----|--------|
| `Tab` / `j` / `↓` | Next field |
| `Shift+Tab` / `k` / `↑` | Previous field |
| `+` / `=` / `→` | Increment value |
| `-` / `←` | Decrement value |
| `Enter` | Create scheduled task |
| `n` | Run task immediately (skip scheduling) |
| `Esc` / `q` | Cancel |

## Hours of Operation

Restrict when scheduled tasks can execute:

### Example: Office Hours Only

```
Hours of Operation: Yes
  Start Hour: 09:00
  End Hour: 17:00
```

Task will only run between 9 AM and 5 PM.

### Example: Overnight Processing

```
Hours of Operation: Yes
  Start Hour: 22:00
  End Hour: 06:00
```

Task will only run between 10 PM and 6 AM (wraps around midnight).

### Behavior

- If scheduled time is outside hours: waits until window opens
- If Clepho isn't running during window: becomes overdue

## Task Execution

### Automatic Polling

While Clepho is running:
- Checks for due tasks every second
- Triggers tasks when:
  - `scheduled_at` time has passed
  - Current time within hours of operation (if set)

### Execution Process

1. Task marked as "Running"
2. Appropriate action started (scan, LLM, faces)
3. Progress shown in status bar
4. Task marked as "Completed" when done

### Status Indicators

| Indicator | Meaning |
|-----------|---------|
| `[S:45%]` | Scheduled scan running |
| `[B:30%]` | Scheduled batch LLM running |
| `[F:60%]` | Scheduled face detection running |

## Overdue Tasks

Tasks that weren't run at their scheduled time.

### Causes

- Clepho wasn't running at scheduled time
- Computer was off/asleep
- Task was outside hours of operation window

### Startup Check

On startup, Clepho checks for overdue tasks:

```
┌─────────────────────────────────────────────────────────────┐
│ Overdue Tasks: 3 scheduled tasks were missed                │
├─────────────────────────────────────────────────────────────┤
│ [ ] Directory Scan | 2024-01-19 02:00 | /home/user/Photos  │
│ [x] LLM Batch | 2024-01-18 03:00 | /home/user/Photos       │
│ [ ] Face Detection | 2024-01-17 04:00 | /home/user/Photos  │
├─────────────────────────────────────────────────────────────┤
│ j/k=nav Space=toggle a=all Enter=run c=cancel all q=dismiss│
└─────────────────────────────────────────────────────────────┘
```

### Overdue Dialog

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate tasks |
| `Space` | Toggle task selection |
| `a` | Select all tasks |
| `Enter` | Run selected tasks |
| `c` | Cancel all overdue tasks |
| `Esc` / `q` | Dismiss (leave pending) |

### Configuration

```toml
[schedule]
# Show overdue dialog on startup
check_overdue_on_startup = true
```

Set to `false` to skip the overdue check.

## Configuration

### Schedule Settings

```toml
[schedule]
# Prompt for overdue tasks on startup
check_overdue_on_startup = true

# Default hours of operation (optional)
# Applied to new scheduled tasks
default_hours_start = 22  # 10 PM
default_hours_end = 6     # 6 AM
```

### Default Hours

If `default_hours_start` and `default_hours_end` are set:
- New scheduled tasks use these as defaults
- Can be overridden in the schedule dialog
- Useful for always scheduling overnight

## Database Storage

### Task Record

```sql
CREATE TABLE scheduled_tasks (
    id INTEGER PRIMARY KEY,
    task_type TEXT,           -- 'Scan', 'LlmBatch', 'FaceDetection'
    target_path TEXT,         -- Directory to process
    photo_ids TEXT,           -- JSON array (for batch operations)
    scheduled_at TEXT,        -- ISO timestamp
    hours_start INTEGER,      -- 0-23 or NULL
    hours_end INTEGER,        -- 0-23 or NULL
    status TEXT,              -- pending/running/completed/cancelled/failed
    created_at TEXT,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT
);
```

### Task Statuses

| Status | Meaning |
|--------|---------|
| `pending` | Waiting for scheduled time |
| `running` | Currently executing |
| `completed` | Finished successfully |
| `cancelled` | User cancelled |
| `failed` | Error during execution |

## Use Cases

### Nightly Scans

Schedule daily scan of auto-import folder:

1. Navigate to import folder
2. Press `@`
3. Set Task Type: Directory Scan
4. Set Time: 02:00 (2 AM)
5. Enable Hours of Operation: 01:00 - 05:00
6. Press Enter

### Weekend Processing

Schedule intensive LLM processing for weekend:

1. Select photos to process
2. Press `@`
3. Set Task Type: LLM Batch Process
4. Set Date: Saturday
5. Set Time: 00:00 (midnight)
6. Press Enter

### Maintenance Window

Schedule all heavy tasks for overnight:

```toml
[schedule]
default_hours_start = 1   # 1 AM
default_hours_end = 5     # 5 AM
```

Then schedule tasks - they'll default to this window.

## Tips

### Reliable Scheduling

1. **Keep Clepho running** - Tasks only execute while running
2. **Use hours of operation** - Prevent accidental daytime execution
3. **Check overdue on startup** - Don't miss important tasks

### Planning Tasks

1. **Scan first** - Always scan before LLM/faces
2. **Schedule in order** - Scan → LLM → Faces
3. **Leave time gaps** - Allow tasks to complete

### Testing Schedules

1. Use `n` to "Run Now" and verify task works
2. Then schedule for actual time
3. Check task list (`T`) for progress

## Troubleshooting

### Task Not Running

- Check Clepho is running at scheduled time
- Verify current time is within hours of operation
- Check task status in database

### Task Failed

- Check error message in task record
- Verify target path exists
- Check disk space and permissions

### Overdue Not Showing

- Verify `check_overdue_on_startup = true`
- Check task exists and is `pending`
- Verify scheduled time is in the past

### Tasks Piling Up

If tasks accumulate:
- Cancel unneeded tasks via overdue dialog
- Check why tasks aren't completing
- Reduce scheduling frequency

## Limitations

### No Recurring Tasks

Currently, each task runs once:
- Must manually recreate for recurring tasks
- Future: recurring schedule support

### Single Instance

- Can't schedule same type twice simultaneously
- Wait for completion before scheduling again

### Clepho Must Run

- Tasks don't run if Clepho isn't open
- Consider running Clepho in background/tmux
- Overdue handling catches missed tasks
