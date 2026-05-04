# Pipeline & Tagging Alignment — Design Spec

**Date:** 2026-05-04
**Status:** design (pre-implementation)
**Scope:** unify clepho's LLM, taxonomy, tagging, description, and gallery surfaces into one mental model with one pipeline, one browse surface, and typed facets.

---

## 1. Problem Statement

The current TUI feels disjointed: LLM-generated tags live in a JSON column on `photos` and are never displayed; user tags live in a parallel `user_tags` system with no reconciliation; the gallery view is a sibling of the folder browser with duplicated state and divergent affordances; album schema exists but has no UI; manual description edits don't update LLM metadata; per-folder prompts are saved silently with no management UI; face detection/clustering is "not optimal" and not user-visible.

The user's stated goal: *point at a folder → run/resume/schedule scan/thumbnail/exif/llm/face/tag → group/view/search by people, places, dates, objects, etc → easily edit/update/alter tags and descriptions, with workflow equivalence between folder and gallery views.*

---

## 2. Architecture Overview

```
┌────────────────────────────────────────────────────────────────┐
│  CONFIG  + managed_folders + smart_albums + pipeline_events    │
└────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┴─────────────────────┐
        ▼                                           ▼
┌───────────────┐                         ┌────────────────────┐
│  TUI          │ ◄──── shared state ───► │  Daemon            │
│  (one keymap, │       (DB + bus)        │  (managed folders, │
│   one dialog  │                         │   schedules)       │
│   surface)    │                         └────────────────────┘
└───────────────┘                                   │
        │                                           │
        ▼                                           ▼
┌────────────────────────────────────────────────────────────────┐
│  PIPELINE — per-photo state machine                            │
│  scan → exif → (thumb ∥ llm ∥ faces) → index                   │
│  (each stage independently triggerable, resumable, skip-if-    │
│   done by default, force-reprocess on demand,                  │
│   provenance-aware writes only)                                │
└────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│  FACETS (typed, distinct)                                      │
│  people  places(GPS)  dates  objects  user_tags  cameras       │
│  events  + albums (manual + smart)                             │
│  Each multi-valued row carries (source, confirmed_at).         │
└────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│  BROWSE SURFACE (one component, two render modes)              │
│  list ◄── Tab ──► grid                                         │
│   • shared selection, filter bar, sort, status badges          │
│   • preview pane = read-only AI panel (always visible)         │
│   • Enter → Inspector mode (full-screen, all-facet edit)       │
│   • multi-select + Enter → bulk Inspector                      │
└────────────────────────────────────────────────────────────────┘
                              │
            ┌─────────────────┼─────────────────┐
            ▼                 ▼                 ▼
      Facets screen     Pipeline status    Map view
```

### Five load-bearing principles

1. **One pipeline, two trigger modes.** Daemon (managed folders, scheduled) and TUI (ad-hoc, per-photo) invoke the same stage executors via the same scheduler.
2. **One browse surface.** `gallery.rs` and the normal-mode browser merge into a single `BrowseView` with two render modes (list/grid). Selection, filters, status, dialogs are shared. Toggling list↔grid is free.
3. **Typed facets, not flat tags.** Seven facets plus albums. The current `photos.tags` JSON column and parallel `user_tags` reconciliation problem both go away.
4. **Provenance is a schema concern, not a UI concern.** Every facet row has `(source, confirmed_at)`. The pipeline writes only where `source = 'ai' AND confirmed_at IS NULL`. The UI just renders the icons.
5. **Skip-if-done is the default.** Each stage gates on `<stage>_done_at`. Force-reprocess is an explicit, scoped action that clears chosen stage timestamps but never overwrites confirmed/user-edited values.

---

## 3. Data Model

### 3.1 `photos` — per-photo state row

Singleton facets and pipeline state live as columns. Multi-valued facets live in junction tables.

```
photos
  id            INTEGER PK
  path          TEXT UNIQUE NOT NULL
  file_hash     TEXT
  file_size     INTEGER
  width, height INTEGER
  mime          TEXT

  -- singleton facets
  taken_at      TEXT            -- EXIF DateTimeOriginal (ISO)
  gps_lat       REAL
  gps_lon       REAL
  gps_alt       REAL
  gps_place_label TEXT          -- nullable; reserved for future reverse geocoding
  camera_make   TEXT
  camera_model  TEXT
  camera_lens   TEXT

  -- description with provenance
  description           TEXT
  description_source    TEXT  -- 'ai' | 'user' | 'ai_edited'
  description_confirmed_at TEXT  -- nullable

  -- pipeline state (skip-if-done gates on these)
  scan_done_at    TEXT
  exif_done_at    TEXT
  thumb_done_at   TEXT
  llm_done_at     TEXT
  faces_done_at   TEXT
  index_done_at   TEXT

  -- latest error per stage (cleared on successful re-run)
  scan_error    TEXT
  exif_error    TEXT
  thumb_error   TEXT
  llm_error     TEXT
  faces_error   TEXT
  index_error   TEXT

  created_at, updated_at  TEXT
```

### 3.2 Multi-valued facet tables

```
objects(id, name UNIQUE, created_at)
photo_objects(photo_id, object_id, source, confirmed_at,
              PRIMARY KEY(photo_id, object_id))

user_tags(id, name UNIQUE, color, created_at)
photo_user_tags(photo_id, tag_id, source, confirmed_at,
                PRIMARY KEY(photo_id, tag_id))

events(id, name, description, cover_photo_id,
       date_start, date_end, created_at)
photo_events(photo_id, event_id, PRIMARY KEY(photo_id, event_id))
```

### 3.3 Faces & people

```
people
  id            INTEGER PK
  name          TEXT
  embedding     BLOB         -- centroid (512 floats) of confirmed faces
  cover_face_id INTEGER
  created_at, updated_at TEXT

faces
  id            INTEGER PK
  photo_id      INTEGER NOT NULL
  bbox_x, bbox_y, bbox_w, bbox_h REAL  -- normalised 0..1
  embedding     BLOB         -- 512 floats from ArcFace
  person_id     INTEGER NULL -- nullable until identified
  source        TEXT NOT NULL  -- 'ai' | 'user'
  confirmed_at  TEXT          -- nullable; non-null = sticky
  detection_confidence REAL
  match_similarity     REAL   -- cosine sim to centroid at match time
```

`person_id IS NULL` faces are clustered live (DBSCAN over embeddings) when the user opens the "Unidentified" view.

### 3.4 Suppressed suggestions

```
rejected_suggestions(photo_id, facet, value, rejected_at)
  -- prevents re-suggesting the same AI value the user explicitly rejected
  -- facet ∈ {object, person, user_tag}
```

### 3.5 Albums (manual + smart, one table)

```
albums
  id INTEGER PK
  name TEXT NOT NULL
  description TEXT
  cover_photo_id INTEGER
  kind TEXT NOT NULL          -- 'manual' | 'smart'
  filter_json TEXT            -- non-null when kind='smart'
  created_at, updated_at TEXT

album_photos(album_id, photo_id, position,
             PRIMARY KEY(album_id, photo_id))
  -- only used when kind='manual'
```

Smart-album `filter_json` is the structure produced by the BrowseView filter bar (saving a filter = creating a smart album):

```json
{ "combinator": "and",
  "clauses": [
    {"facet":"people",  "op":"any_of",     "values":["alice","bob"]},
    {"facet":"places",  "op":"within_km",  "value":{"lat":41.9,"lon":12.5,"km":50}},
    {"facet":"dates",   "op":"between",    "value":{"from":"2024-01-01","to":"2024-12-31"}},
    {"facet":"objects", "op":"any_of",     "values":["sunset"]}
  ] }
```

### 3.6 Pipeline orchestration tables

```
managed_folders
  id INTEGER PK
  path TEXT UNIQUE NOT NULL
  schedule_cron TEXT          -- nullable (manual-only if null)
  paused INTEGER DEFAULT 0
  last_run_at TEXT
  faces_disabled INTEGER DEFAULT 0  -- set if user declined model download
  created_at, updated_at TEXT

folder_prompts
  path TEXT PRIMARY KEY        -- both managed and ad-hoc folders
  custom_prompt TEXT NOT NULL
  updated_at TEXT

pipeline_events
  id INTEGER PK
  occurred_at TEXT NOT NULL
  level TEXT NOT NULL          -- 'info' | 'warn' | 'error'
  stage TEXT                   -- nullable (folder-level events)
  folder TEXT                  -- nullable
  photo_id INTEGER             -- nullable
  error_class TEXT             -- nullable, coarse bucket for grouping
  message TEXT NOT NULL
  context_json TEXT            -- nullable, structured details
  resolved_at TEXT             -- nullable
```

`directory_prompts` (current) → folded into `folder_prompts`.

### 3.7 Embeddings (kept)

Existing `embeddings` table stays for semantic-search-by-description. Face embeddings live on the `faces` table (different domain, different lifecycle).

---

## 4. Pipeline & State Machine

### 4.1 Six stages

| # | Stage | Inputs | Writes |
|---|---|---|---|
| 1 | scan  | filesystem entry | `photos` row (path, file_hash, size, mime, dimensions) |
| 2 | exif  | image bytes | `photos.taken_at, gps_*, camera_*` |
| 3 | thumb | image bytes | thumbnail file on disk |
| 4 | llm   | image bytes + prompt | `photos.description` (`description_source='ai'`), `photo_objects` (`source='ai'`), text embedding in `embeddings` |
| 5 | faces | image bytes | `faces` rows (with `person_id` if cosine-sim ≥ match_threshold) |
| 6 | index | row + facet rows | recompute smart-album membership for this photo; rebuild FTS document; recompute affected `people.embedding` centroids; emit completion `pipeline_events` |

### 4.2 DAG

```
scan ──► exif ──► (thumb ∥ llm ∥ faces) ──► index
```

`scan` and `exif` are sequential. `thumb`, `llm`, `faces` run in parallel after `exif`. `index` runs after all three model stages complete for a photo.

### 4.3 Skip-if-done

For each stage, the executor selects only photos where `<stage>_done_at IS NULL`. `index_done_at` is the canonical "fully processed" indicator used by status badges.

### 4.4 Force-reprocess

- **Per photo, per stage**: from Inspector, "Re-run LLM" / "Re-run faces" buttons clear that stage's `<stage>_done_at` and downstream `index_done_at`.
- **Folder-level bulk**: `Shift+R` opens a dialog: `"Reprocess folder /photos. Stages: [✓] LLM [✗] Faces [✗] EXIF"`. Submit clears chosen stage timestamps for *all* photos in the folder.
- **Provenance still wins**: even on forced reprocess, the stage executor only writes facet rows where `source='ai' AND confirmed_at IS NULL`.
- **Pre-flight count**: dialog shows `"Will reprocess 412 photos. 38 user-edited and 71 confirmed-AI values will be preserved."`

### 4.5 Worker pools

```toml
[pipeline]
scan_workers   = 2
exif_workers   = 4
thumb_workers  = 4
llm_workers    = 2
faces_workers  = 2
index_workers  = 4
circuit_breaker_threshold = 3   # consecutive same-class failures before stage pauses
```

A single in-process scheduler owns the pools. Scheduler pulls work via SQL ("photos where exif_done_at IS NULL AND scan_done_at IS NOT NULL", etc.) and dispatches to the right pool.

### 4.6 Trigger model

- **Daemon**: cron-driven enqueues for managed folders; filesystem watcher for new files.
- **TUI**: `R` runs ad-hoc on a folder; `i` per-photo; `Shift+R` force-reprocess; `M` toggles managed status (writes `managed_folders` row).

The scheduler doesn't care which path enqueued the work; the queue is the contract.

### 4.7 Failure & retry

- A stage-worker exception → write `<stage>_error`, append `pipeline_events` at level=`error`, leave `<stage>_done_at` NULL so the photo is naturally a candidate for retry.
- Three (configurable) consecutive errors of the same class in one stage → stage pool pauses; other stages keep running.
- Retry from Pipeline Status: `r` clears `<stage>_error` and requeues.

### 4.8 Crash safety

- Stage completes only when both (a) outputs written and (b) `<stage>_done_at` set, **in the same transaction**. Partial writes impossible.
- On restart, scheduler re-queries pending work; no in-memory queue to recover.
- Stages are idempotent (writing the same description twice is a no-op via provenance rule; writing the same `photo_objects` row uses `INSERT OR IGNORE`).

---

## 5. TUI Surfaces

### 5.1 BrowseView — list mode

```
┌─ /home/photos/2024/italy ────────────────────────[●managed]─────┐
│ Filter: [👤 alice] [📍 ≤50km of Rome] [📅 2024-06]          [+] │
│ Sort: date↓                                Selected: 3        ⚙ │
├──────────┬─────────────────────────────────┬────────────────────┤
│ ../      │   IMG_2841.jpg          ✓✓✓✓✓✓│ IMG_2841.jpg       │
│ 2023/    │   IMG_2842.jpg          ✓✓✓✓✓·│  ┌──────────────┐  │
│ 2024/    │ ▶ IMG_2843.jpg          ✓✓·✓✓·│  │   [thumb]    │  │
│  italy/●│ ✓ IMG_2844.jpg          ✓✓✓✓✓✓│  └──────────────┘  │
│  france/ │ ✓ IMG_2845.jpg          ✓✓✓!✓·│ 📝 Sunset on the    │
│          │   IMG_2846.jpg          ····· │   Tiber...      ✨   │
│          │   IMG_2847.jpg          ✓✓✓✓✓✓│ 👤 Alice ✓, Bob ✨  │
│          │                                │ 📍 41.89° 12.49°    │
│          │                                │ 📅 2024-06-12       │
│          │                                │ 🏷 sunset, river ✨ │
│          │                                │ 🔖 keep ✏️          │
│          │                                │ 📷 Sony A7iv 35mm   │
│          │                                │ 🎉 —                │
│          │                                │ Pipeline:           │
│          │                                │  ✓✓✓✓✓✓ all done    │
├──────────┴─────────────────────────────────┴────────────────────┤
│ 412/1200 indexed · LLM running(3) · 2 errors  [T]tasks [?]help  │
└─────────────────────────────────────────────────────────────────┘
```

**Status badge strip** = `scan/exif/thumb/llm/faces/index`. `✓` done · `·` pending · `!` error.

**Provenance icons**: `✨` AI, `✓` confirmed, `✏️` user-edited.

### 5.2 BrowseView — grid mode (`Tab` toggles)

Identical filter bar, identical preview pane, identical keymap. **Selection survives the toggle.**

### 5.3 Inspector — single photo (`Enter`)

Full-screen edit view with sections: Description · People · Places · Date · Objects · Tags · Camera · Events · Pipeline status. `Tab/Shift+Tab` cycles sections. Inside a section: standard `j/k/a/d/Enter/x` per the unified keymap.

Quick-edits jump straight here: `b` opens Inspector with Tags section in add-mode; `e` opens Description in edit mode; `p` opens People; `i` opens Description and runs regenerate. Old `tag_dialog.rs` and `edit_dialog.rs` are retired.

### 5.4 Bulk Inspector (multi-select + `Enter`)

Same layout, shows "common to all / partial N/M / varies" per row. Singleton facets (Description, Places, Date, Camera) can only be set wholesale with a warning.

### 5.5 Facets screen (`F`)

Eight facets: People, Places, Dates, Objects, Tags, Cameras, Events, Albums. Bulk operations live here (rename a person, merge two people, delete a noisy object tag).

### 5.6 Map view (Places facet → `Enter`, or `m` from Inspector)

ASCII unicode dot-scatter for v1. Lat/lon normalised to terminal canvas; clusters within ~1 char merged with count. Real tile rendering deferred.

### 5.7 Pipeline Status screen (`T`)

Three sections: Managed Folders (with cron, status, photo counts) · Workers (live per-stage activity) · Failures Inbox (grouped by `error_class`, then folder).

### 5.8 Filter bar (`f` from BrowseView)

Add clauses by facet; AND across facets, OR within a facet (where applicable). Save as smart album = create `albums` row with `kind='smart'`.

### 5.9 Canonical keymap

| Key | Action |
|---|---|
| `j/k/h/l` | move (l/h grid only) |
| `g/G` | top/bottom |
| `Tab` | list↔grid in BrowseView; section cycle in Inspector |
| `Space` | toggle select |
| `v / V` | visual range select |
| `Ctrl+a` | select all |
| `Esc` | clear selection / close |
| `Enter` | open Inspector (single/bulk) |
| `f / F` | filter bar / Facets screen |
| `T` | Pipeline Status |
| `R / Shift+R` | run / force-reprocess |
| `M` | toggle managed folder |
| `i / e / b / p` | Inspector pre-positioned |
| `a / d / x / c` | add / remove / reject / confirm (within Inspector section) |
| `m` | map view (within Places section) |
| `?` `$` `q` | help / settings / quit |

### 5.10 Mouse interactions

Mouse is a complement, not a parallel. Every gesture has a keyboard equivalent.

**Universal:**
- Left click → move cursor.
- Click on focused item → activate (= `Enter`).
- Double-click → open Inspector.
- Wheel → scroll (= `j/k`).
- Shift-click → range select.
- Ctrl-click → toggle multi-select.
- Click outside modal → close.

**Surface-specific:**
- Status badge strip → click a single badge opens Inspector with Pipeline section focused on that stage.
- Filter chip → click to remove.
- AI suggestion chip → click accept; shift-click or middle-click reject.
- Inspector section header → click to focus that section.
- Map → click-drag pan; click cluster filter; wheel zoom.

**Multi-select rubber-band:**
Grid mode click-drag on empty area = rubber-band selection. Hold `Ctrl` to add to existing selection.

**Terminal-copy escape:**
Holding `Shift` during any mouse gesture bypasses the TUI mouse handler, falling through to terminal-native text selection.

**Out of scope for v1**: right-click context menus, drag-and-drop between folders, drag to reorder.

---

## 6. Editing & Provenance

### 6.1 The contract (one sentence)

**The pipeline writes only where `source = 'ai' AND confirmed_at IS NULL`. Users write anywhere; their writes set `source = 'user'`.**

### 6.2 Per-operation semantics

| Operation | Result |
|---|---|
| Pipeline writes new AI value | INSERT/UPDATE: `source='ai'`, `confirmed_at=NULL` |
| Pipeline rerun, value matches existing | No-op (idempotent) |
| Pipeline rerun, would differ from confirmed/user row | Skipped — contract excludes it |
| User accepts AI suggestion (`Enter`/`c`) | UPDATE: `source='ai'`, `confirmed_at=now()` |
| User adds value manually (`a`) | INSERT: `source='user'`, `confirmed_at=now()` |
| User edits a value | UPDATE: description→`description_source='ai_edited'` if was ai, else `'user'`; `confirmed_at=now()` |
| User removes a value (`d`) | DELETE; pipeline may re-suggest next run |
| User rejects AI suggestion (`x`) | DELETE + INSERT into `rejected_suggestions`; never re-suggested |

### 6.3 Description provenance

`description_source` ∈ `{ai, user, ai_edited}`:
- `ai`: pipeline wrote it; rerun *can* overwrite.
- `ai_edited`: user edited an AI description; rerun **cannot** overwrite.
- `user`: user wrote from scratch; rerun **cannot** overwrite.

`c` (confirm) on an `ai` description without editing → state stays `ai` but `description_confirmed_at` is set, locking against rerun.

### 6.4 Rejected-suggestions

Pipeline writes consult `rejected_suggestions` before INSERT — if `(photo, facet, value)` is in the table, skip. Rejection is per-photo per-value. Global rejection (delete the `objects` row entirely) is a Facets-screen action.

### 6.5 Bulk edit semantics

- `[a] add` → for each photo, single-photo add.
- `[d] remove` → for each photo with the value, single-photo remove.
- `[Enter] accept partial` → for each photo where the suggestion exists, single-photo accept; for photos without, no-op.
- `[a] apply (partial→all)` → adds with `source='user'` to photos that don't have it. (Distinct from accept — "apply" is a user assertion.)
- Singleton fields: bulk write replaces all; warning shown.

### 6.6 Index trigger on edits

User edits clear `index_done_at` for the affected photo(s). Next scheduler tick recomputes smart-album membership, FTS, person centroids. **Smart albums are eventually consistent**, not transactional.

### 6.7 No undo in v1

Edit operations are not reversible from the TUI. `Ctrl+R` revert in the description editor remains, scoped to a single edit session. Revisit if pain emerges.

### 6.8 Bulk-edit failure semantics

Each photo's edit is its own transaction (no all-or-nothing across photos). Failures append to `pipeline_events` at level=`error`. Bulk dialog shows summary on close: `"97 succeeded, 3 failed — see Pipeline Status"`.

---

## 7. Face Engine

### 7.1 Trait

```rust
trait FaceEngine: Send + Sync {
    fn detect(&self, image: &DynamicImage) -> Result<Vec<Detection>>;
    fn embed(&self, image: &DynamicImage, det: &Detection) -> Result<Embedding512>;
    fn name(&self) -> &str;
    fn dims(&self) -> usize;
}

struct Detection { bbox: BBox, landmarks: [Point2; 5], confidence: f32 }
type Embedding512 = [f32; 512];
```

Same shape as `LlmClient`. Config-driven factory selects the implementation. Future Python sidecar / external API engines plug in here.

### 7.2 v1 default — Rust + ONNX (`ort` crate)

| Stage | Model | Size | Purpose |
|---|---|---|---|
| Detect | SCRFD-500M (`scrfd_500m.onnx`) | ~2.5 MB | Bounding boxes + 5-point landmarks |
| Embed  | ArcFace BUFFALO_L (`w600k_r50.onnx`) | ~166 MB | 512-dim L2-normalised embedding |

Crates: `ort`, `image`, `imageproc` (affine warp for canonical 112×112 alignment), `ndarray`.

### 7.3 Per-photo flow

```
load_image
  → SCRFD detect
  → for each detection above detect_min_confidence:
      align (affine warp via 5 landmarks → 112×112 canonical)
      → ArcFace embed (L2-normalised)
      → similarity match against people.embedding centroids
        if max_sim ≥ match_threshold:
          person_id = argmax_person, source='ai', confirmed_at=NULL,
          match_similarity = max_sim
        else if max_sim ≥ match_show_threshold:
          person_id = NULL  (clusterable, suggestable in UI)
        else:
          person_id = NULL  (no UI suggestion)
      → INSERT INTO faces
  → mark faces_done_at
```

### 7.4 Person centroid maintenance

`people.embedding` = mean of all confirmed faces' embeddings, L2-normalised. Recomputed when any face's `confirmed_at` transitions NULL→non-NULL, `person_id` changes, or face is deleted from a person. Recompute happens in the `index` stage.

### 7.5 Config

```toml
[faces]
engine                = "rust_onnx"
detect_min_confidence = 60      # percentage; SCRFD detection score floor
match_threshold       = 55      # percentage; cosine sim to auto-assign person_id
match_show_threshold  = 50      # percentage; below = no UI suggestion at all
batch_size            = 4
model_dir             = "~/.cache/clepho/models/"
execution_provider    = "cpu"   # future: "cuda" | "metal" | "coreml"
```

Stored as percentages (0..100) in config for readability; converted to 0..1 floats internally.

### 7.6 Auto-download on first use

On first `faces` stage invocation:

1. Engine constructor checks `model_dir` for required ONNX files.
2. If missing → `Err(ModelMissing { name, url, size_mb })`.
3. TUI shows one-time consent dialog:
   ```
   Download face models? SCRFD (2.5MB) + ArcFace (166MB)
     → ~/.cache/clepho/models/
     [Y] download    [N] disable face stage
   ```
4. On consent: HTTP GET, atomic rename, retry stage.
5. On decline: `managed_folders.faces_disabled = 1` for that folder; global setting also exposed.

Model URLs and SHA256 checksums baked into the binary; checksum verified after download. Nix users can pre-place models into `model_dir`.

### 7.7 Unidentified clustering

`person_id IS NULL` faces clustered live (DBSCAN, cosine distance, `eps=0.40`, `min_samples=3`) when user opens "Unidentified" view. Naming a cluster: select N face thumbnails, press `n`, type name. All selected faces get `person_id = new_person_id, source='user', confirmed_at=now()`.

### 7.8 Failure modes

| Failure | Stage outcome |
|---|---|
| Model file missing | stage skipped (decline path) or auto-download (consent path) |
| Model load fails | `faces_error = "model load failed: <details>"`, photo not done |
| No faces detected (clean negative) | success, `faces_done_at` set, no rows |
| OOM during inference | retry once with `batch_size=1`; if still OOM → error |
| Image decode error | `faces_error = "image decode: <details>"`, photo not done |

### 7.9 Performance baseline

- ArcFace inference per face: ~30-80 ms CPU.
- SCRFD per image: ~50-150 ms CPU.
- Photo with 3 faces: ~250-400 ms total face-stage time. LLM stage remains the bottleneck.

---

## 8. Error Handling & Observability

### 8.1 Three-layer surface

| Layer | Audience | Persistence |
|---|---|---|
| Status bar toast | active user | transient (~3s) |
| Pipeline Status screen | active user | DB (`pipeline_events`, ~10k cap) |
| Structured log file (JSONL) | debugging, postmortem | disk, rotated |

### 8.2 `pipeline_events` triggers

| Trigger | Level |
|---|---|
| Stage starts on folder | info |
| Stage completes on folder | info |
| Per-photo failure | error |
| Stage circuit-broken | error |
| Managed folder add/remove | info |
| Force-reprocess triggered | info |
| User clears errors | info |
| Schedule fired | info |

`error_class` is a coarse bucket (e.g. `llm_unreachable`, `model_missing`, `exif_invalid_gps`, `image_decode`, `oom`) set by the stage worker.

### 8.3 Per-photo error columns

`<stage>_error TEXT` holds the **latest** error for that stage. Cleared on successful re-run. `pipeline_events` is the time-series; `<stage>_error` is the current state.

### 8.4 Circuit breaker

Threshold: **3** consecutive same-class failures, configurable. Tripped → stage pool pauses; toast surfaces; Pipeline Status row written. Other stages keep running. User clears via Pipeline Status `r`.

### 8.5 Toast model

Status bar already shows progress (`[B:45%]`). Toasts are distinct: a one-line message above progress, ~3s auto-dismiss. Stack of pending toasts bounded (last 3 visible). Glyphs: `✓ ℹ ⚠ ✖`. Color-coded.

### 8.6 Failures Inbox

Grouped by `error_class` first, folder second. `Enter` expands. Per-group: `r` retry, `c` clear (sets `resolved_at`), `i` open photo in Inspector, `l` open log file at relevant timestamp.

### 8.7 Structured log file (JSONL)

Path: `${XDG_STATE_HOME:-~/.local/state}/clepho/logs/clepho-YYYYMMDD.jsonl`.

```json
{
  "ts": "2026-05-04T19:23:45.123Z",
  "level": "error",
  "component": "pipeline.faces",
  "folder": "/home/photos/2024/italy",
  "photo_id": 2841,
  "photo_path": "/home/.../IMG_2841.jpg",
  "stage": "faces",
  "error_class": "model_missing",
  "message": "ArcFace model not found at /home/.../w600k_r50.onnx",
  "context": {"engine": "rust_onnx", "model_dir": "..."},
  "duration_ms": null
}
```

Daily rotation; gzip rotated files; **30-day default retention**, configurable via `[logging] retention_days`. Verbosity levels: `trace, debug, info, warn, error`. Default `info`. `trace` is opt-in for short investigations only.

### 8.8 Crash safety

`pipeline_events` writes commit per-event (no batching); restarts visible via daemon-startup events. Stage state survives via in-DB stage-flags; on restart scheduler re-queries pending work.

### 8.9 Out of scope

- Image bytes / embeddings / full LLM responses logged only at `trace`.
- TUI edit audit trail (`edit_log` table) → v1.5.

---

## 9. Testing Strategy

### 9.1 Load-bearing rules with dedicated test modules

1. Provenance contract (Section 6.2 matrix), table-driven.
2. Stage idempotency.
3. Skip-if-done.
4. Force-reprocess preserves confirmed/user values.
5. Smart-album filter evaluator.
6. Pipeline DAG ordering.

### 9.2 Test layers

**Unit tests** (inline `#[cfg(test)]`):
- Provenance contract — `tests/provenance_contract.rs`, all (operation, starting_state, expected) pairs.
- Filter evaluator — all operators × all combinators.
- Cosine similarity / face matching decisions.
- Rejected-suggestions blocking pipeline candidates.

**Integration tests** (`tests/`, real SQLite in tmpdir):
- End-to-end pipeline on fixture photos → DB rows match expected.
- Resume after crash: drop stage future before commit, restart, assert completion without duplicates.
- Skip-if-done: pre-populate timestamps, run, assert zero work (mock executor invocation count).
- Force-reprocess: confirmed values pre-populated, run, assert AI rows refresh and confirmed untouched.
- Circuit breaker: inject 3 same-class failures → pool pauses; fix → resumes.
- Bulk edit: 3-photo selection × common/partial/varies operations → per-photo writes match Section 6.5.

**TUI snapshot tests** (`insta` + ratatui `TestBackend`):
~10-15 canonical snapshots: BrowseView list, BrowseView grid, Inspector single, Inspector bulk (mixed facets), Facets screen, Map view, Pipeline Status, Filter bar, plus provenance-icon variants and status badge permutations. Committed; reviewed on diff.

**Face engine tests** (`#[cfg(feature = "face-models")]`):
Real fixture photos under `tests/fixtures/faces/` with `manifest.json` of expected detections and same-person pairings. Run nightly / on-demand only.

### 9.3 Mocked vs real

| Component | Default test mode |
|---|---|
| Database | Real SQLite in tmpdir (in-memory for unit) |
| Filesystem | `tempfile::TempDir` |
| LLM | Mock returning canned responses |
| Face engine | Mock returning canned detections (real only behind feature flag) |
| EXIF, image decode | Real on tiny test JPEGs |
| Time | Injected `Clock` trait — non-negotiable for deterministic `confirmed_at` |
| Cron scheduler | Manual tick advance |

### 9.4 Property tests (where they earn it)

`proptest` for:
- Filter evaluator: random `(filter, facets)` → match naive reference impl.
- Smart-album recompute invariant: random edit sequence → membership equals filter applied to current facets.

### 9.5 Test fixtures

`tests/fixtures/` (≤5 MB total):
- `photos/` — 10-20 small JPEGs with curated EXIF.
- `faces/` — 5-10 photos + `manifest.json`.
- `llm_responses/` — canned bodies (valid JSON, missing fields, code-fenced, legacy TAGS:).
- `filters/` — example smart-album filter JSONs.

### 9.6 What we don't test

- Visual appearance of unicode glyphs.
- Mouse hit-test coordinates in detail (the central hit-test map gets unit-tested for `bounds → action`).
- External services reachable.
- Performance benchmarks in CI gate.

### 9.7 CI

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                          # default
cargo test --features face-models   # nightly / on-demand
```

Snapshot review mandatory on rendering PRs.

---

## 10. Scope, Migration & Build Sequence

### 10.1 Schema reset

Fresh schema, no data migration. On first start of new build: detect pre-existing schema → one-time "Reset?" dialog with clear warning. No silent destruction.

### 10.2 Survives from current code

| Subsystem | Status |
|---|---|
| `LlmClient` trait + provider implementations | keep |
| `embeddings` table | keep |
| Image preview protocols (sixel/kitty) | keep |
| Settings dialog frame | keep, extend |
| Cron / schedule code | keep, extend |
| EXIF reader | keep |
| Thumbnail generator | keep |
| Confirm dialog (with editable prompt) | keep, generalise |

### 10.3 Deleted

| Subsystem | Reason |
|---|---|
| `tag_dialog.rs` | replaced by Inspector → Tags |
| `edit_dialog.rs` | replaced by Inspector → Description |
| Old face detection/clustering tables and code | replaced by FaceEngine + faces/people |
| `photos.tags` JSON column | replaced by `photo_objects` + `photo_user_tags` |
| `directory_prompts` table | folded into `folder_prompts` |
| Existing gallery view as separate component | merged into BrowseView |

### 10.4 Five-phase build sequence

**Phase 1 — Foundation** (size: M; risk: low)
- Fresh schema
- Provenance contract enforcement at DB layer (single function with tests)
- Stage state machine + scheduler skeleton
- `pipeline_events` + JSONL log
- `Clock` trait injection
- Reset-DB dialog

**Phase 2 — Pipeline (5 stages, no faces)** (size: L; risk: medium)
- 5 stages in DAG order: scan, exif, thumb, llm, index
- Skip-if-done + force-reprocess + circuit breaker
- LLM stage refactored to write new facet rows
- `managed_folders` + register/unregister
- Pipeline Status screen (minimal)

**Phase 3 — Face engine** (size: L; risk: medium-high)
- `FaceEngine` trait + Rust+ONNX impl
- Auto-download consent dialog
- `faces` + `people` tables
- Centroid maintenance + match flow
- DBSCAN clustering query function (no UI yet)

**Phase 4 — TUI rebuild (cutover)** (size: XL; risk: high)
- BrowseView (list + grid + filter bar + status badges)
- Preview pane = unified AI panel
- Inspector (single + bulk) replacing old dialogs
- Mouse hit-test centralisation
- Canonical keymap enforced

Lands as one PR/release — partial state would be confusing.

**Phase 5 — Discovery surfaces** (size: L; risk: low-medium)
- Facets screen (8 facets including albums)
- Filter bar save → smart album
- Manual albums
- Map view (unicode dot-scatter)
- People merge/rename/triage UI
- Unidentified-faces clustering view

### 10.5 Out of scope for v1 (deferred to v1.5+)

- Reverse geocoding for `gps_place_label` (column reserved, layer not built)
- Right-click context menus
- Drag-and-drop between folders
- Edit-log audit trail
- GPU execution provider tuning for face models
- Undo beyond `Ctrl+R` revert
- Python sidecar face engine
- Real tile-based map rendering
- Bulk-edit description
- Confidence display on objects (faces only in v1)

### 10.6 Risks

1. **BrowseView refactor blast radius.** Mitigation: spike Phase 4 with throwaway prototype before committing to cutover.
2. **Smart album recompute cost.** Mitigation: incremental recompute (only check albums whose filter touches changed facets).
3. **Face match threshold tuning.** 60/55/50 are starting points. Easily adjustable; revisit after first large run.
4. **DBSCAN performance on >10k unidentified embeddings.** Profile early; pre-computed clusters as fallback.
5. **Daemon/TUI scheduler race.** Two options: TUI-with-daemon defers to daemon's scheduler via existing IPC, or DB advisory lock ensures only one scheduler is active. Decide in implementation plan.

---

## 11. References

- Current LLM design: `docs/LLM_PROCESS_DESIGN.md`
- Current keyboard shortcuts: `docs/keyboard-shortcuts.md`
- Architecture: `docs/ARCHITECTURE.md`
- Existing face docs: `docs/faces.md`

---

*End of design.*
