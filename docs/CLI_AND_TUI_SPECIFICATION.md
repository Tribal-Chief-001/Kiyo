# CLI Toolkit and Terminal UI (TUI) Specification

## 1. Headless CLI Architecture (`kiyo-cli`)

`kiyo-cli` provides a high-throughput, scriptable document processing pipeline. Built on `clap 4.5` with derive macros, it delivers sub-millisecond execution times, structured outputs (JSON, YAML, plain text), and UNIX pipe integration.

```
                                      +--------------------+
                                      |      kiyo-cli      |
                                      +--------------------+
                                                |
        +-------------+------------+------------+------------+------------+-------------+
        |             |            |            |            |            |             |
        v             v            v            v            v            v             v
    [merge]        [split]       [diff]       [ocr]      [extract]     [watch]     [gui / tui]
  (Zero-Copy)   (Page Burst)   (Visual/Text) (Sandwich) (Markdown)    (Hot-Reload) (Viewers)
```

### 1.1. CLI Grammar & Command-Line Interface Definition

```rust
// crates/kiyo-cli/src/args.rs

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "kiyo",
    version = "0.1.0",
    about = "Ultra-high-performance PDF engine, viewer, and document manipulation suite",
    long_about = "Kiyo unifies GPU-accelerated viewing (GUI), terminal graphical viewing (TUI), and headless manipulation (CLI) into a single cohesive Rust system."
)]
pub struct Cli {
    #[arg(short, long, global = true, help = "Suppress all informational output")]
    pub quiet: bool,

    #[arg(short, long, global = true, help = "Output structured results as JSON for automation")]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(help = "Path to PDF file to open (defaults to launching GUI or TUI)")]
    pub default_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch the high-framerate hardware-accelerated desktop viewer
    Gui(GuiArgs),

    /// Launch the modal terminal graphical viewer
    Tui(TuiArgs),

    /// Concatenate multiple PDF documents into a single unified file
    Merge(MergeArgs),

    /// Decompose a PDF into page ranges, chapters, or individual page bursts
    Split(SplitArgs),

    /// Compare two PDF revisions with visual GPU overlays or textual diffs
    Diff(DiffArgs),

    /// Run offline neural OCR to generate searchable sandwich PDFs
    Ocr(OcrArgs),

    /// Extract structured plain text, Markdown tables, or embedded images
    Extract(ExtractArgs),

    /// Monitor a PDF file and hot-reload viewers on modification (LaTeX/Typst loops)
    Watch(WatchArgs),
}

#[derive(Args, Debug)]
pub struct MergeArgs {
    #[arg(required = true, num_args = 2.., help = "Input PDF files to merge in sequential order")]
    pub inputs: Vec<PathBuf>,

    #[arg(short, long, default_value = "merged.pdf", help = "Output destination file path")]
    pub output: PathBuf,

    #[arg(long, help = "Preserve and merge document outlines/bookmarks")]
    pub preserve_bookmarks: bool,
}

#[derive(Args, Debug)]
pub struct SplitArgs {
    #[arg(required = true, help = "Source PDF document to split")]
    pub input: PathBuf,

    #[arg(short, long, help = "Page range specification (e.g. '1-5,8,12-end')")]
    pub pages: Option<String>,

    #[arg(long, help = "Split every single page into an independent PDF file")]
    pub burst: bool,

    #[arg(long, help = "Split document automatically at top-level table of contents bookmarks")]
    pub by_chapters: bool,

    #[arg(short, long, default_value = "dist", help = "Output directory")]
    pub output_dir: PathBuf,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    #[arg(required = true, help = "Baseline revision PDF (Revision A)")]
    pub file_a: PathBuf,

    #[arg(required = true, help = "Modified revision PDF (Revision B)")]
    pub file_b: PathBuf,

    #[arg(short, long, help = "Generate a visual composite PDF showing colored differences")]
    pub output_pdf: Option<PathBuf>,

    #[arg(long, default_value = "0.08", help = "Visual pixel difference sensitivity threshold [0.0 - 1.0]")]
    pub threshold: f32,
}

#[derive(Args, Debug)]
pub struct OcrArgs {
    #[arg(required = true, help = "Input scanned PDF or image document")]
    pub input: PathBuf,

    #[arg(short, long, help = "Output destination for the searchable PDF")]
    pub output: Option<PathBuf>,

    #[arg(long, default_value = "true", help = "Inject invisible text layer over original image (ISO 32000 3 Tr)")]
    pub sandwich: bool,

    #[arg(short, long, help = "Target language codes (e.g. 'en,de,fr')")]
    pub language: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExtractArgs {
    #[arg(required = true, help = "Input PDF file")]
    pub input: PathBuf,

    #[arg(long, help = "Reconstruct two-column text and tables into clean Markdown format")]
    pub markdown: bool,

    #[arg(long, help = "Extract all embedded raster images into the output folder")]
    pub images: bool,

    #[arg(short, long, default_value = "./extracted", help = "Output directory")]
    pub output_dir: PathBuf,
}

#[derive(Args, Debug)]
pub struct WatchArgs {
    #[arg(required = true, help = "Target PDF file to monitor")]
    pub file: PathBuf,

    #[arg(long, help = "Trigger shell command hook on each atomic reload")]
    pub on_reload: Option<String>,
}

#[derive(Args, Debug)]
pub struct GuiArgs {
    pub file: Option<PathBuf>,
    #[arg(long, help = "Enable smart luminance-preserving dark mode")]
    pub dark: bool,
}

#[derive(Args, Debug)]
pub struct TuiArgs {
    pub file: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "auto", help = "Forced terminal graphics protocol")]
    pub graphics: TerminalGraphicsMode,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum TerminalGraphicsMode {
    Auto,
    Kitty,
    Sixel,
    Iterm2,
    Halfblock,
}
```

---

## 2. Modal Terminal Viewer (`kiyo-tui`)

`kiyo-tui` provides a terminal-native PDF reading interface. Running inside modern terminal emulators (Kitty, Foot, WezTerm, Alacritty, iTerm2), it combines full modal Vim ergonomics with inline high-resolution graphics rendering.

```
+-----------------------------------------------------------------------------------------+
| [Normal Mode]  Page 12 / 144  (Zoom: 125%)                    [kiyo-tui v0.1.0]        |
+-----------------------------------------------------------------------------------------+
|                                                           | [Academic Portal: Eq. 4.2]  |
|   1. INTRODUCTION                                         |                             |
|                                                           |   $$\nabla \times \mathbf{E}|
|   In this paper we present a novel approach to            |   = -\frac{\partial \mathbf{|
|   high-performance document rendering... [1]              |   {B}}{\partial t}$$        |
|                                                           |                             |
|   As demonstrated in Equation (4.2), the field            |                             |
|   equations exhibit continuous symmetry...                |                             |
|                                                           |                             |
|                                                           |                             |
+-----------------------------------------------------------------------------------------+
| :goto 42                                                  [Frustum: 120Hz | Kitty PROTO]|
+-----------------------------------------------------------------------------------------+
```

### 2.1. Comprehensive Vim Keybinding Matrix

| Mode | Keybinding | Action Description | Technical Behavior |
| :--- | :--- | :--- | :--- |
| **Normal** | `j` / `Down` | Scroll viewport down | Shifts viewport by $40\text{ px}$ |
| **Normal** | `k` / `Up` | Scroll viewport up | Shifts viewport by $-40\text{ px}$ |
| **Normal** | `d` / `Ctrl+d` | Scroll half-page down | Advances viewport by $0.5 \times \text{FrustumHeight}$ |
| **Normal** | `u` / `Ctrl+u` | Scroll half-page up | Rewinds viewport by $0.5 \times \text{FrustumHeight}$ |
| **Normal** | `Ctrl+f` / `Space` | Scroll full page down | Advances viewport by $1.0 \times \text{FrustumHeight}$ |
| **Normal** | `Ctrl+b` | Scroll full page up | Rewinds viewport by $1.0 \times \text{FrustumHeight}$ |
| **Normal** | `gg` | Jump to beginning | Moves viewport to Page 1, Offset 0 |
| **Normal** | `G` | Jump to end | Moves viewport to Page N, bottom |
| **Normal** | `[count]G` | Jump to specific page | Translates viewport directly to page `[count]` |
| **Normal** | `+` / `=` | Zoom in | Multiplies zoom factor by $1.15\times$; dispatches re-rasterization |
| **Normal** | `-` | Zoom out | Multiplies zoom factor by $0.85\times$; dispatches re-rasterization |
| **Normal** | `z0` | Reset zoom | Snaps zoom factor to $1.0\times$ (100% scale) |
| **Normal** | `zf` | Fit to width | Snaps page width to fill active terminal columns |
| **Normal** | `p` | Open Academic Portal | Opens a floating secondary viewport at current cursor/citation |
| **Normal** | `P` | Pin Portal Viewport | Locks portal viewport state, preventing auto-dismiss |
| **Normal** | `Tab` | Toggle Focus | Switches keyboard focus between primary document and portal |
| **Normal** | `r` | Rotate Page | Rotates active page clockwise by $90^\circ$ |
| **Normal** | `i` | Invert Colors (Dark) | Toggles luminance-preserving smart dark mode |
| **Normal** | `m[a-z]` | Set Mark | Saves current page and viewport offset to mark register `[a-z]` |
| **Normal** | `'[a-z]` | Jump to Mark | Restores viewport offset from mark register `[a-z]` |
| **Normal** | `/` | Forward Regex Search | Opens search prompt at bottom bar; highlights matches |
| **Normal** | `?` | Backward Regex Search | Opens backward search prompt |
| **Normal** | `n` | Next Match | Jumps viewport to next search match coordinate |
| **Normal** | `N` | Previous Match | Jumps viewport to previous search match coordinate |
| **Normal** | `v` | Visual Character Mode | Initiates freeform text glyph selection |
| **Normal** | `V` | Visual Line Mode | Initiates rectangular bounding line selection |
| **Visual** | `y` | Yank Selection | Copies selected text directly to system clipboard and `PRIMARY` |
| **Visual** | `Esc` | Cancel Selection | Clears visual selection highlights |
| **Normal** | `:` | Command Mode | Enters command mode prompt at bottom status line |

### 2.2. Command Mode Syntax (`:`)
- `:goto <page>`: Jump directly to specified page number.
- `:write [path]`: Export modified document (with annotations or rotations) to disk.
- `:export md [path]`: Reconstruct and export document text into a clean Markdown file.
- `:diff <path_to_revision>`: Open visual revision difference split against another file.
- `:ocr`: Execute offline neural OCR pass on current page.
- `:info`: Display technical document metadata (PDF version, encryption, xref status, font tables).
- `:quit` / `:q`: Terminate viewer.

---

## 3. Terminal Graphics Protocols via `ratatui` and `ratatui-image`

Displaying vector PDF documents inside a text-based terminal requires translating rasterized RGBA8 tile buffers into terminal-native graphic escape codes:

```
                                  +-----------------------------+
                                  |    Rendered RGBA8 Buffer    |
                                  +-----------------------------+
                                                 |
                                     (Capability Detection)
                                                 v
        +-----------------------+-----------------------+-----------------------+
        |                       |                       |                       |
        v                       v                       v                       v
 [Kitty Protocol]       [Sixel Protocol]        [iTerm2 Protocol]       [ANSI Half-Blocks]
 - Direct shared memory - 6-pixel banded bitmap - Base64 inline stream  - ▀/▄ Truecolor 24-bit
 - Zero escape bloat    - Universal legacy      - WezTerm / macOS       - Universal fallback
 - 120Hz video capable  - Foot / WezTerm / XTerm                        - Any terminal
```

### 3.1. Automatic Capability Detection Heuristic
On startup, `kiyo-tui` executes a deterministic capability detection sequence:
1. **Direct Terminal Querying**: Sends primary and secondary device attribute request queries:
   ```
   \x1b[c          (Primary Device Attributes)
   \x1b[>c         (Secondary Device Attributes)
   \x1b_Gi=1,a=q\x1b\ (Kitty Graphics Query)
   ```
2. **Environment Variable Inspection**:
   - `KITTY_WINDOW_ID` $\to$ **Kitty Graphics Protocol**
   - `TERM_PROGRAM=WezTerm` $\to$ **Kitty / iTerm2 Protocol**
   - `TERM=foot*` $\to$ **Sixel Graphics Protocol**
3. **Graceful Fallback**: If no graphic protocol acknowledges within a 35ms timeout, `kiyo-tui` selects **ANSI Half-Block (`▀`/`▄`) Truecolor rendering**, ensuring an unbroken visual experience even over remote SSH sessions or legacy serial terminals.
