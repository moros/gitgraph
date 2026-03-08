# gitgraph Roadmap Spec

> Command: `gg`
> A TUI that combines serie's git log graph with ftdv's tree diff viewing into a unified commit exploration tool.

---

## 1. Project Overview

**gitgraph** (command: `gg`) is a terminal-based git commit explorer that merges two complementary workflows:

- **serie** — A git log TUI that renders image-based commit graphs (iTerm2/Kitty protocols) with a navigable commit list, detail view, refs browser, and configurable user commands.
- **ftdv** — A file tree diff viewer that shows a collapsible file tree alongside inline diff content, with external diff tool integration and checkbox persistence.

**gitgraph combines these** into a single tool: browse the commit graph (serie-style), select a commit, and explore its changes through a file tree with inline diffs (ftdv-style) — all without leaving the terminal.

### Design Principles

1. **Unified flow**: One tool replaces `serie` + `ftdv` for commit exploration.
2. **Familiar UX**: Vim-style keybindings, consistent with both predecessors.
3. **Performance**: Lazy loading, caching, and incremental rendering for large repos.
4. **Configurability**: TOML config for colors, keybindings, columns, and external tools.
5. **Protocol support**: iTerm2 and Kitty image protocols for graph rendering (inherited from serie).

---

## 2. Architecture Design

### 2.1 Module Structure

```
src/
├── main.rs              # Entry point, CLI parsing, terminal setup/teardown
├── lib.rs               # Public API, module declarations, run() lifecycle
├── app.rs               # App struct, state machine, main event loop
├── event.rs             # Event types, mpsc channel, background input thread
├── keybind.rs           # Keybinding system (defaults + config overrides)
├── config.rs            # TOML config loading, validation, defaults
├── color.rs             # Color theme (UI colors + graph colors)
├── protocol.rs          # Terminal protocol detection (iTerm2/Kitty)
├── external.rs          # User commands, clipboard, external diff tools
│
├── git/
│   ├── mod.rs           # Repository struct, loading orchestration
│   ├── commit.rs        # Commit, CommitHash, Ref, Head types
│   ├── diff.rs          # Diff generation, FileChange, FileDiff types
│   └── executor.rs      # Git command execution (shelling out)
│
├── graph/
│   ├── mod.rs           # Graph struct, public API
│   ├── calc.rs          # Lane-based graph layout algorithm
│   ├── geometry.rs      # 2D math (Point, Vector, bounding box)
│   └── image.rs         # Per-commit PNG rendering, protocol encoding
│
├── view/
│   ├── mod.rs           # View enum, trait, dispatch
│   ├── list.rs          # ListView — commit log with graph
│   ├── detail.rs        # DetailView — commit info + file tree + diff
│   ├── refs.rs          # RefsView — branch/tag/stash browser
│   ├── help.rs          # HelpView — dynamic keybinding reference
│   └── user_command.rs  # UserCommandView — external command output
│
├── widget/
│   ├── mod.rs           # Widget re-exports
│   ├── commit_list.rs   # CommitList widget (graph + columns)
│   ├── commit_detail.rs # CommitDetail widget (metadata + message)
│   ├── file_tree.rs     # FileTree widget (collapsible tree with stats)
│   ├── diff_viewer.rs   # DiffViewer widget (inline diff rendering)
│   └── ref_list.rs      # RefList widget (tree of refs)
│
├── tree/
│   ├── mod.rs           # FileTreeBuilder, FileTreeItem
│   └── icons.rs         # Nerd Font file/directory icons
│
└── theme.rs             # ThemeColor serde, ColorScheme
```

### 2.2 Data Flow

```
CLI args (clap)
  → Config loading (TOML)
  → Git repository loading (git CLI)
  → Graph calculation (lane algorithm)
  → Graph image generation (PNG + protocol encoding)
  → App::run() event loop
      ↓
  ┌─────────────────────────────────────┐
  │  crossterm events (background thread) │
  │         ↓ mpsc channel               │
  │  App::handle_event()                 │
  │         ↓                             │
  │  View::handle_event() → AppEvent    │
  │         ↓                             │
  │  State transitions / data loading    │
  │         ↓                             │
  │  View::render() → Frame             │
  └─────────────────────────────────────┘
```

### 2.3 Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Git interaction | Shell out (no git2/libgit2) | Both serie and ftdv use this approach. Simpler, no native deps, leverages user's git config. |
| Graph rendering | Image-based (iTerm2/Kitty) | Inherited from serie. Produces high-quality curved branch lines impossible with text. |
| Config format | TOML | Inherited from serie. More expressive than YAML for nested config. |
| TUI framework | ratatui + crossterm | Both predecessors use this. Mature, well-documented, active ecosystem. |
| State management | Enum-based state machine | Clean transitions, exhaustive matching, inherited from serie. |

---

## 3. Dependency Selection

### 3.1 Core Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `ratatui` | 0.30 | TUI framework (serie's version, newer than ftdv's 0.29) |
| `crossterm` | 0.29 | Terminal backend |
| `clap` | 4.x (derive) | CLI argument parsing |
| `image` | 0.25 | Graph image generation (RGBA buffers, PNG encoding) |
| `rayon` | 1.11 | Parallel graph image preloading |
| `chrono` | 0.4 | Date formatting |
| `serde` | 1.x | Config deserialization |
| `toml` | 0.8 | TOML config parsing |
| `anyhow` | 1.x | Error handling |

### 3.2 UI Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `tui-tree-widget` | 0.24 | Tree rendering for refs view (from serie) |
| `tui-input` | 0.15 | Text input for search (from serie) |
| `ansi-to-tui` | 8.0 | ANSI escape → ratatui Text (for external tools) |
| `strip-ansi-escapes` | 0.2 | ANSI stripping for width calculation |

### 3.3 Utility Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `fuzzy-matcher` | 0.3 | Fuzzy search (SkimMatcherV2, from serie) |
| `laurier` | 0.3 | Search highlight rendering (from serie) |
| `arboard` | 3.6 | Clipboard (from serie) |
| `umbra` | 0.4 | TOML partial parsing with defaults (from serie) |
| `garde` | 0.22 | Config validation (from serie) |
| `dirs` | 6.0 | XDG directory resolution |

### 3.4 Dropped from ftdv

| Crate | Reason |
|-------|--------|
| `serde_yaml` | Replaced by TOML config |
| `serde_json` | Persistence will use a simpler format or be deferred |
| `tempfile` | Not needed for core functionality |

---

## 4. View System Design

gitgraph has 5 views, extending serie's view system with ftdv's diff capabilities integrated into the detail view.

### 4.1 View Enum

```rust
enum View {
    List(Box<ListView>),
    Detail(Box<DetailView>),    // Enhanced: now includes file tree + diff
    Refs(Box<RefsView>),
    Help(Box<HelpView>),
    UserCommand(Box<UserCommandView>),
}
```

### 4.2 ListView (serie-style git log with graph)

The primary view. Displays a scrollable commit list with configurable columns.

**Layout:**
```
┌──────────────────────────────────────────────────┐
│ Graph │ Marker │ Subject (refs) │ Name │ Hash │ Date │
│  ██   │   ●    │ fix: resolve…  │ dev  │ a1b2c │ 2d  │
│  ██   │        │ feat: add new… │ dev  │ 3d4e5 │ 3d  │
│  ██   │        │ Merge branch…  │ dev  │ 6f7g8 │ 5d  │
│  ...  │        │ ...            │      │       │     │
├──────────────────────────────────────────────────┤
│ [Search: /pattern]              │ [Status line]  │
└──────────────────────────────────────────────────┘
```

**Columns** (configurable order):
- **Graph**: Rendered PNG image via iTerm2/Kitty protocol
- **Marker**: HEAD indicator
- **Subject**: Commit message with inline ref labels (branches, tags)
- **Name**: Author name (truncated)
- **Hash**: Short SHA (7 chars)
- **Date**: Relative or formatted date

**Features** (from serie):
- Vim-style navigation (j/k, g/G, H/M/L, Ctrl-D/U)
- Numeric prefix for repeat (e.g., `5j` = move down 5)
- Search with `/` (exact + fuzzy, case toggle, highlight)
- Jump to parent commit (Alt-j)
- Navigate by ref (n/N for next/prev match)
- Copy commit hash (c for short, C for full)

### 4.3 DetailView (enhanced with file tree + diff)

**This is the key innovation of gitgraph.** When you press Enter on a commit in ListView, the DetailView shows commit metadata AND a file tree with inline diffs — combining serie's detail panel with ftdv's core functionality.

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│ [Commit List - compressed, top portion]              │
├──────────────────────────────────────────────────────┤
│ Author:    John Doe <john@example.com>               │
│ Date:      2024-01-15 14:30:00                       │
│ SHA:       a1b2c3d4e5f6 (HEAD -> main, origin/main)  │
│ Parents:   9f8e7d6                                   │
│                                                      │
│ feat: add new authentication module                  │
│                                                      │
│ Implements OAuth2 flow with PKCE support.            │
├────────────────────┬─────────────────────────────────┤
│ File Tree (20%)    │ Diff Content (80%)              │
│                    │                                  │
│ ├── src/           │ @@ -10,6 +10,8 @@               │
│ │   ├── auth/      │  use crate::config;              │
│ │   │   ├── mod.rs │ +use crate::oauth;               │
│ │   │   └── oauth… │ +use crate::token;               │
│ │   └── main.rs    │                                  │
│ ├── tests/         │  fn main() {                     │
│ │   └── auth_tes…  │ +    let client = OAuth::new();  │
│ └── Cargo.toml     │      // ...                     │
│    +142 -23        │                                  │
└────────────────────┴─────────────────────────────────┘
```

**Commit metadata section** (from serie's CommitDetail):
- Author name, email, date
- Committer (if different from author)
- SHA with ref labels
- Parent hashes
- Commit message (subject + body)

**File tree panel** (from ftdv's tree system):
- Hierarchical file tree with expand/collapse
- Tree connectors (├, ╰, │) for visual hierarchy
- Nerd Font icons per file type
- Per-file diff stats (+N -M) colored green/red
- Directory aggregate stats when collapsed
- Navigation: j/k to move, Enter/l to toggle directory, h to collapse

**Diff content panel** (from ftdv's diff rendering):
- Shows diff for currently selected file
- Syntax-aware coloring (added/removed/context lines)
- Horizontal and vertical scrolling
- Support for external diff tools (delta, difftastic)
- ANSI escape parsing for external tool output
- Width-responsive re-rendering

**Key behaviors:**
- File tree loads lazily: `git diff --name-status` on enter, full diff on file select
- Navigate commits while in detail view (J/K or Alt-j/Alt-k from serie)
- Press `q` or `Esc` to return to list view
- Preserve scroll position and selected file when navigating between commits

### 4.4 RefsView (from serie)

Side panel showing branches, remotes, tags, stashes as a tree. Selecting a ref jumps to that commit in the list.

### 4.5 HelpView (from serie)

Full-screen overlay showing all keybindings, dynamically generated from the keybinding configuration. Sections: Common, List, Detail, Refs, UserCommand.

### 4.6 UserCommandView (from serie)

Split panel showing output of user-defined commands (e.g., `git diff`, custom scripts). Supports inline (captured stdout) and silent (background execution with optional refresh) modes.

---

## 5. File Tree Widget

The file tree widget is gitgraph's adaptation of ftdv's tree system, redesigned as a standalone ratatui widget.

### 5.1 Data Structures

```rust
struct FileTreeItem {
    name: String,
    full_path: String,
    is_directory: bool,
    depth: usize,
    file_diff: Option<FileDiff>,
    is_last_child: bool,
    parent_is_last: Vec<bool>,    // For drawing tree connectors
    is_expanded: bool,
    dir_file_count: usize,       // Aggregate stats for collapsed dirs
    dir_added_lines: usize,
    dir_removed_lines: usize,
}

struct FileTreeBuilder {
    // Builds Vec<FileTreeItem> from Vec<FileChange>
}

struct FileTreeState {
    selected_index: usize,
    collapsed_directories: HashSet<String>,
    offset: usize,              // Scroll offset
}
```

### 5.2 Build Pipeline

1. **Input**: `Vec<FileChange>` from `git diff --name-status`
2. **Build tree structure**: Split paths on `/`, create intermediate directory nodes
3. **Sort**: Directories first, then case-insensitive alphabetical (ftdv's diffnav-like ordering)
4. **Calculate stats**: Bottom-up recursive aggregation of file count, added lines, removed lines per directory
5. **Flatten**: DFS traversal, skip children of collapsed directories → `Vec<FileTreeItem>`

### 5.3 Rendering

Each row renders:
```
[tree prefix][icon] [filename]              [+N -M]
```

- **Tree prefix**: Unicode box-drawing chars (│, ├──, ╰──) based on `depth`, `is_last_child`, `parent_is_last`
- **Icon**: Nerd Font icon from `icons.rs` (extension-based lookup, directory open/closed)
- **Filename**: Truncated with `...` if exceeding available width
- **Stats**: Right-aligned `+N -M` in green/red, or directory aggregate stats when collapsed

### 5.4 Interactions

| Key | Action |
|-----|--------|
| `j`/`↓` | Move selection down |
| `k`/`↑` | Move selection up |
| `Enter`/`l` | Toggle directory expand/collapse, or select file (loads diff) |
| `h` | Collapse current directory (or move to parent) |
| `g` | Jump to first file |
| `G` | Jump to last file |

---

## 6. Inline Diff Viewer

The diff viewer renders file diffs within the detail view's right panel.

### 6.1 Data Structures

```rust
struct FileDiff {
    filename: String,
    old_path: Option<String>,
    new_path: Option<String>,
    content: String,            // Raw diff text
    added_lines: usize,
    removed_lines: usize,
}

struct DiffViewerState {
    vertical_scroll: usize,
    horizontal_scroll: usize,
    content_height: usize,      // For scroll clamping
    content_width: usize,
}
```

### 6.2 Diff Generation

Two modes for obtaining diff content:

1. **Internal** (default): `git diff <parent>..<commit> -- <file_path>` with `--color=never`. Parsed and styled by gitgraph.
2. **External tool**: `git -c diff.external=<cmd> diff --ext-diff` or piping through a pager (delta, bat). ANSI output parsed via `ansi-to-tui`.

External tool integration follows ftdv's approach:
- **Pager mode**: Pipe diff content to stdin of tool (delta, bat, ydiff)
- **External diff mode**: Set via git's `diff.external` config
- Template variables: `{{width}}`, `{{columnWidth}}` for width-responsive tools
- Width-responsive re-rendering on terminal resize (threshold: >5 char change)

### 6.3 Rendering

- Context lines: default foreground
- Added lines (`+`): green foreground
- Removed lines (`-`): red foreground
- Hunk headers (`@@`): cyan/styled
- File headers (`diff --git`, `---`, `+++`): bold
- Line wrapping: `Wrap { trim: false }` (consistent with ftdv)
- Scroll: vertical (e/y for single line, d/u for page) and horizontal (h/l or arrows)

### 6.4 Scroll Clamping

Content dimensions are calculated by stripping ANSI codes and measuring actual line count and max line width. Scroll positions are clamped to prevent over-scroll (adopted from ftdv's `clamp_scroll()`).

---

## 7. Keybinding Map

Keybindings are configurable via TOML config (serie's system). Defaults merge with user overrides.

### 7.1 Global

| Key | Action | Context |
|-----|--------|---------|
| `Ctrl-C` | Force quit | All |
| `q` | Quit / close view | All |
| `?` | Toggle help | All |
| `Esc` | Cancel / close | All |
| `R` | Refresh (reload git data) | All |

### 7.2 List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Navigate down |
| `k` / `↑` | Navigate up |
| `g` | Go to top |
| `G` | Go to bottom |
| `H` / `M` / `L` | Select top / middle / bottom of screen |
| `Ctrl-D` / `Ctrl-U` | Half page down / up |
| `Ctrl-F` / `PageDown` / `Space` | Page down |
| `Ctrl-B` / `PageUp` | Page up |
| `Ctrl-E` / `Ctrl-Y` | Scroll down / up (without moving selection) |
| `Shift-J` / `Shift-K` | Select down / up (move selection with scroll) |
| `Enter` | Open detail view for selected commit |
| `Tab` | Open refs view |
| `/` | Start search |
| `n` / `N` | Next / previous search match |
| `Ctrl-G` | Toggle case-insensitive search |
| `Ctrl-X` | Toggle fuzzy search |
| `Alt-j` | Jump to parent commit |
| `c` | Copy short hash |
| `C` | Copy full hash |
| `d` | User command 1 (default: `git diff`) |
| `1`-`9` | Numeric prefix for repeat |

### 7.3 Detail View

| Key | Action |
|-----|--------|
| `j` / `↓` | Navigate file tree down |
| `k` / `↑` | Navigate file tree up |
| `Enter` / `l` | Toggle directory / select file |
| `h` | Collapse directory / move to parent dir |
| `g` / `G` | First / last file |
| `e` / `y` | Scroll diff down / up (1 line) |
| `d` / `u` | Scroll diff down / up (half page) |
| `f` / `b` | Scroll diff down / up (full page) |
| `←` / `→` | Horizontal scroll diff (5 cols) |
| `Shift-H` / `Shift-L` | Fast horizontal scroll (20 cols) |
| `J` / `K` | Navigate to next / previous commit (stay in detail) |
| `q` / `Esc` | Return to list view |

### 7.4 Refs View

| Key | Action |
|-----|--------|
| `j` / `↓` | Navigate down |
| `k` / `↑` | Navigate up |
| `Enter` | Jump to selected ref in commit list |
| `c` | Copy ref name |
| `Tab` / `q` | Close refs view |

### 7.5 Keybinding Config Format

```toml
[keybind]
navigate_down = ["j", "down"]
navigate_up = ["k", "up"]
confirm = ["enter"]
quit = ["q"]
help_toggle = ["?"]
# ... full list in assets/default-keybind.toml
```

Supports modifiers: `ctrl-`, `alt-`, `shift-`. Chains: `ctrl-shift-l`. Special keys: `esc`, `enter`, `f1`-`f12`, `tab`, `space`, `backspace`.

---

## 8. State Machine

### 8.1 App States

```
                    ┌─────────┐
              ┌────→│  Help   │←────┐
              │     └────┬────┘     │
              │  ?       │ ?/q      │  ?
              │          ↓          │
         ┌────┴────┐          ┌────┴─────────┐
    ────→│  List   │──Enter──→│   Detail     │
         │         │←──q/Esc──│ (tree + diff)│
         │         │          └──────────────┘
         │         │                │
         │         │──Tab──→┌──────┴────┐
         │         │←─Tab/q─│   Refs    │
         │         │        └───────────┘
         │         │
         │         │──d/1-9─→┌─────────────┐
         │         │←──q/Esc──│ UserCommand │
         └────┬────┘         └─────────────┘
              │
         ForceQuit / Quit
```

### 8.2 State Transitions

| From | Trigger | To | Data Loaded |
|------|---------|-----|-------------|
| List | `Enter` | Detail | `git diff --name-status` for selected commit, build file tree |
| Detail | `q`/`Esc` | List | Restore list selection and scroll |
| Detail | `J`/`K` | Detail (next/prev commit) | Reload file tree + diff for new commit |
| Detail | file selected | Detail (diff loaded) | `git diff <parent>..<commit> -- <file>` |
| List | `Tab` | Refs | Load all refs into tree |
| Refs | `Enter` | List | Jump list to selected ref's commit |
| Refs | `Tab`/`q` | List | Restore list state |
| Any | `?` | Help | Store previous view for restore |
| Help | `?`/`q` | Previous | Restore stored view |
| List/Detail | `d`/`1-9` | UserCommand | Execute external command with commit context |
| UserCommand | `q` | List | — |
| Any | `R` | Same (refreshed) | Reload git repo, recalculate graph, preserve context |

### 8.3 CommitListState Ownership

Following serie's `Option::take` pattern: `CommitListState` is wrapped in `Option` and transferred between views via `take()`. Each view that needs the commit list takes ownership; on transition back, it returns the state.

### 8.4 RefreshViewContext

On refresh (`R`), the app preserves:
- Selected commit hash
- Scroll offset
- Current view and view-specific state (e.g., selected file in detail, search query in list)

The refresh cycle: event loop returns `Ret::Refresh` → `lib.rs` reloads repository → recalculates graph → re-enters app with preserved context.

---

## 9. Rendering Pipeline

### 9.1 Frame Composition

Each frame follows this pipeline:

```
App::draw(frame)
  → match current_view {
      List → render_list_view(frame)
      Detail → render_detail_view(frame)
      Refs → render_refs_view(frame)
      Help → render_help_view(frame)
      UserCommand → render_user_command_view(frame)
    }
```

### 9.2 List View Rendering

```
frame area
├── commit_list area (full width, height - status_bar_height)
│   ├── Column: Graph (encoded image, cell_width * max_pos_x)
│   ├── Column: Marker (1 char)
│   ├── Column: Subject (min_width constraint, flexible)
│   │   └── Inline ref labels [branch] [tag] before subject text
│   ├── Column: Name (configurable width, truncated)
│   ├── Column: Hash (7 chars)
│   └── Column: Date (configurable width + format)
└── status_bar area (full width, 1 row)
    └── search input (when active) or status messages
```

**Graph column**: Each row is a pre-rendered PNG image encoded via iTerm2 inline image protocol or Kitty graphics protocol. Images are lazily generated and cached in `GraphImageManager`. Rayon preloads visible range.

**Column widths**: Configurable via config. Subject uses `Min` constraint to fill remaining space. Column order is configurable.

### 9.3 Detail View Rendering

```
frame area
├── commit_list area (top portion, compressed)
├── commit_metadata area (configurable height)
│   ├── Two-column layout: 12-char labels + values
│   └── Author, Committer, SHA, Parents, Refs, Message, File Changes summary
└── diff_explorer area (remaining height)
    ├── file_tree panel (20% width)
    │   └── FileTree widget (scrollable, tree connectors, icons, stats)
    └── diff_content panel (80% width)
        ├── status_line (file path, icon, diff stats, scroll position)
        └── diff_viewer (scrollable paragraph with syntax coloring)
```

### 9.4 Graph Image Pipeline (from serie)

```
Per commit:
  1. Create RGBA buffer (cell_width * lanes, 36px height)
  2. Fill background color
  3. Draw edges (anti-aliased SDF lines)
     - 10 edge types: Vertical, Horizontal, Up, Down, Left, Right,
       RightTop, RightBottom, LeftTop, LeftBottom
     - Colors cycle through palette by x-position
     - GraphStyle: Rounded (Bezier arcs) or Angular (sharp corners)
  4. Draw commit dot (filled circle at commit position)
  5. Encode as PNG
  6. Encode via terminal protocol (iTerm2 base64 or Kitty chunked)
  7. Cache result in GraphImageManager
```

**Cell sizing**: Double (24px/cell) or Single (12px/cell), height always 36px. Auto-detection based on terminal width.

---

## 10. Git Data Layer

### 10.1 Repository Loading

Following serie's approach, all git interaction shells out via `std::process::Command`. No libgit2.

**Load sequence** (at startup and on refresh):

1. `git rev-parse --git-dir` — verify git repository
2. `git show-ref --head --dereference` — load all refs (branches, tags, remotes). Handle annotated tag dereferencing.
3. `git stash list` — load stashes
4. `git log -z --format="%H%x1f%an%x1f%ae%x1f%ad%x1f%cn%x1f%ce%x1f%cd%x1f%s%x1f%b%x1f%P" --branches --remotes --tags HEAD` — load all commits. NUL-delimited, fields separated by Unit Separator. Supports `--date-order` or `--topo-order`.
5. Merge stashes into commit list (inserted before parent)
6. Build bidirectional parent/child maps

### 10.2 Diff Generation

**On-demand** (when entering detail view or selecting a file):

- **File list**: `git diff --name-status <parent>..<commit>` → `Vec<FileChange>` with status (Add, Modify, Delete, Move)
- **File diff**: `git diff <parent>..<commit> -- <file_path>` → raw diff text
- **Initial commits** (no parent): `git ls-tree -r --name-only <commit>` for file list, `git show <commit> -- <file>` for content
- **External tools**: `git -c diff.external=<cmd> diff --ext-diff <parent>..<commit> -- <file>` or pipe through pager

### 10.3 Data Structures

```rust
struct CommitHash(String);  // newtype, as_short_hash() = 7 chars

struct Commit {
    hash: CommitHash,
    author: Author,           // { name, email, date }
    committer: Author,
    subject: String,
    body: String,
    parent_hashes: Vec<CommitHash>,
    commit_type: CommitType,  // Commit | Stash
}

enum Ref {
    Tag { name, target },
    Branch { name, target },
    RemoteBranch { name, target },
    Stash { name, target, message },
}

enum Head {
    Branch { name },
    Detached { target },
    None,
}

struct Repository {
    commits: CommitMap,           // IndexMap<CommitHash, Commit>
    parents_map: HashMap<CommitHash, Vec<CommitHash>>,
    children_map: HashMap<CommitHash, Vec<CommitHash>>,
    refs: RefMap,                 // HashMap<CommitHash, Vec<Ref>>
    commit_hashes: Vec<CommitHash>,  // Ordered
    head: Head,
}

enum FileChange {
    Add(String),
    Modify(String),
    Delete(String),
    Move(String, String),  // from, to
}

struct FileDiff {
    filename: String,
    old_path: Option<String>,
    new_path: Option<String>,
    content: String,
    added_lines: usize,
    removed_lines: usize,
}
```

### 10.4 Caching Strategy

| Data | Cache Lifetime | Invalidation |
|------|---------------|--------------|
| Commit list + graph | App session | Refresh (`R`) reloads |
| Graph images | App session | Regenerated on refresh |
| File change list per commit | Detail view session | Cleared on view close or commit change |
| File diff content | Per file selection | Replaced when different file selected |
| Ref list | App session | Refresh reloads |

Future optimization: LRU cache for file diffs of recently viewed commits to speed up J/K navigation in detail view.

---

## 11. Config System

### 11.1 Config File Location

Resolution order (serie's approach):
1. `GITGRAPH_CONFIG_FILE` environment variable
2. `$XDG_CONFIG_HOME/gitgraph/config.toml`
3. `~/.config/gitgraph/config.toml`

Uses `umbra::optional` macro for partial TOML parsing with defaults. `garde` crate for validation.

### 11.2 Config Schema

```toml
[core]
# Git log ordering
order = "chrono"          # "chrono" | "topo"

# Graph rendering
graph_width = "auto"      # "auto" | "double" | "single"
graph_style = "rounded"   # "rounded" | "angular"

# Initial selection
initial_selection = "latest"  # "latest" | "head"

# Terminal protocol
protocol = "auto"         # "auto" | "iterm" | "kitty"

[core.search]
ignore_case = false
fuzzy = false

[core.external]
clipboard = "auto"        # "auto" | { commands = ["wl-copy"] }

[core.diff]
# External diff tool integration (from ftdv)
pager = ""                # e.g., "delta --side-by-side"
external_command = ""     # e.g., "difft --color=always"
color_arg = "always"

[core.user_command]
# Numbered user commands (from serie)
commands_1 = { name = "Diff", type = "inline", commands = ["git", "--no-pager", "diff", "--color=always", "first_parent_hash", "target_hash"] }
# commands_2, commands_3, etc.
tab_width = 4

[ui.list]
columns = ["Graph", "Marker", "Subject", "Name", "Hash", "Date"]
subject_min_width = 20
date_format = "%Y-%m-%d"
date_width = 10
date_local = true
name_width = 20

[ui.detail]
metadata_height = 8       # Height of commit metadata section
date_format = "%Y-%m-%d %H:%M:%S"
date_local = true
tree_width_percent = 20   # File tree panel width percentage

[ui.refs]
width = 26

[ui.common]
cursor_type = "native"    # "native" | { virtual = ">" }

[graph.color]
branches = ["#E06C75", "#E5C07B", "#98C379", "#56B6C2", "#61AFEF", "#C678DD"]
edge = "transparent"
background = "transparent"

[color]
# 48+ color fields for all UI elements (from serie)
# Plus new fields for file tree and diff viewer
list_selected_bg = "#3E4452"
tree_directory = "blue"
tree_file = "white"
tree_selected_bg = "#323248"
diff_added = "green"
diff_removed = "red"
diff_hunk_header = "cyan"
diff_context = "white"
# ... (full list in defaults)

[keybind]
# Override any default keybinding
# navigate_down = ["j", "down"]
# confirm = ["enter"]
```

---

## 12. Implementation Phases

### Phase 1: Core Scaffold

**Objective**: Buildable project with CLI, config loading, and terminal setup/teardown.

**Deliverables**:
- `Cargo.toml` with all dependencies
- `main.rs` — CLI parsing (clap derive), config loading, terminal init/restore
- `lib.rs` — module declarations, `run()` entry point
- `config.rs` — TOML config with `umbra`, defaults, validation
- `color.rs` / `theme.rs` — color scheme definitions
- `keybind.rs` — keybinding system with defaults file
- Empty view/widget/git module stubs

**Acceptance Criteria**:
- `cargo build` succeeds
- `gg --help` shows usage
- `gg --version` shows version
- Config file is loaded if present, defaults used otherwise
- Terminal enters raw mode and exits cleanly

### Phase 2: Git Data Layer + Graph

**Objective**: Load repository data and calculate graph layout.

**Deliverables**:
- `git/` module — Repository loading, commit/ref/stash parsing
- `graph/` module — Lane-based layout algorithm, edge calculation
- `event.rs` — Event types, mpsc channel, background input thread
- `app.rs` — Basic App struct with event loop skeleton

**Acceptance Criteria**:
- Running in a git repo loads all commits, refs, stashes
- Graph positions calculated for all commits
- Event loop starts and handles Ctrl-C quit
- Supports `--date-order` and `--topo-order`

### Phase 3: List View

**Objective**: Functional commit list with graph rendering.

**Deliverables**:
- `graph/image.rs` — Per-commit PNG rendering, protocol encoding
- `protocol.rs` — iTerm2/Kitty detection and encoding
- `widget/commit_list.rs` — CommitList widget with configurable columns
- `view/list.rs` — ListView with navigation, search, numeric prefix

**Acceptance Criteria**:
- Commit list renders with graph images (iTerm2 or Kitty)
- All navigation keybindings work (j/k, g/G, H/M/L, Ctrl-D/U, etc.)
- Search with `/` works (exact + fuzzy, case toggle)
- Numeric prefix repeats work (e.g., `5j`)
- Copy hash to clipboard works (c/C)
- Column order and widths are configurable

### Phase 4: Detail View with File Tree + Diff

**Objective**: The key feature — commit detail with file tree and inline diff viewer.

**Deliverables**:
- `git/diff.rs` — On-demand diff generation, file change detection
- `tree/` module — FileTreeBuilder, FileTreeItem, icons
- `widget/file_tree.rs` — FileTree widget with expand/collapse
- `widget/diff_viewer.rs` — DiffViewer widget with scroll and syntax coloring
- `widget/commit_detail.rs` — CommitDetail metadata widget
- `view/detail.rs` — DetailView composing all three panels
- `external.rs` — External diff tool integration (pager + ext-diff modes)

**Acceptance Criteria**:
- Pressing Enter on a commit shows detail view with metadata, file tree, and diff
- File tree shows hierarchical structure with icons and diff stats
- Directories collapse/expand with aggregate stats
- Selecting a file shows its diff in the right panel
- Diff scrolls vertically and horizontally
- External diff tools (delta, difftastic) work if configured
- J/K navigates between commits while staying in detail view
- q/Esc returns to list view with preserved selection

### Phase 5: Refs, Help, User Commands

**Objective**: Complete the remaining views.

**Deliverables**:
- `widget/ref_list.rs` — RefList tree widget
- `view/refs.rs` — RefsView with ref tree and list jump
- `view/help.rs` — HelpView with dynamic keybinding display
- `view/user_command.rs` — UserCommandView with inline/silent modes
- `external.rs` — User command execution with template markers

**Acceptance Criteria**:
- Tab opens refs panel with branches/remotes/tags/stashes tree
- Selecting a ref jumps to that commit
- `?` shows help with all configured keybindings
- User commands execute with commit context template expansion
- Inline commands show output, silent commands run in background

### Phase 6: Polish and Optimization

**Objective**: Performance, edge cases, and UX refinement.

**Deliverables**:
- Rayon-based parallel graph image preloading
- LRU cache for file diffs (detail view J/K navigation)
- Resize handling (re-render, width-responsive external tools)
- Error display (status line notifications from serie)
- Refresh cycle (R key) with context preservation
- Fallback for terminals without image protocol support (text-based graph)
- Integration tests

**Acceptance Criteria**:
- Smooth scrolling in repos with 10,000+ commits
- Graph images preloaded for visible range
- Terminal resize handled gracefully
- Refresh preserves user context (selection, scroll, view)
- Errors shown in status line, consumed on next keypress
- All keybindings configurable and validated
- `cargo test` passes
- `cargo clippy` clean

---

## Appendix A: Comparison with Predecessors

| Feature | serie | ftdv | gitgraph |
|---------|-------|------|---------|
| Commit log with graph | Yes (image-based) | No | Yes |
| File tree diff view | No | Yes | Yes (in detail view) |
| Inline diff viewer | No | Yes | Yes (in detail view) |
| External diff tools | Via user commands | Pager + ext-diff | Both approaches |
| Refs browser | Yes | No | Yes |
| Search | Exact + fuzzy | Substring | Exact + fuzzy |
| Config format | TOML | YAML | TOML |
| Clipboard | Yes | No | Yes |
| Persistence | No | Checkbox state | Deferred |
| Graph style | Rounded/Angular | N/A | Rounded/Angular |
| Numeric prefix | Yes (vim-style) | No | Yes |
| User commands | Yes (numbered) | No | Yes |
| File icons | No | Nerd Font | Nerd Font |

## Appendix B: Crate Versions Reference

Based on serie v0.6.1 and ftdv v0.1.2 dependency analysis. When versions differ, prefer serie's (newer) versions.

```toml
[dependencies]
ratatui = "0.30"
crossterm = "0.29"
clap = { version = "4", features = ["derive"] }
image = "0.25"
rayon = "1.11"
chrono = "0.4"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
anyhow = "1"
tui-tree-widget = "0.24"
tui-input = "0.15"
ansi-to-tui = "8.0"
strip-ansi-escapes = "0.2"
fuzzy-matcher = "0.3"
laurier = "0.3"
arboard = "3.6"
umbra = { version = "0.4", features = ["optional"] }
garde = { version = "0.22", features = ["derive"] }
dirs = "6.0"

[dev-dependencies]
dircpy = "0.3"
tempfile = "3"
text-to-png = "0.2"
```
