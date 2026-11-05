# Headless CLI Toolkit & Linux Desktop Integration Specification

## 1. Headless CLI Architecture (`kiyo-cli`)

`kiyo-cli` provides an ultra-high-throughput, scriptable document processing pipeline and desktop launcher for Kiyo. Built on `clap 4.5` with derive macros, it delivers sub-millisecond execution times, structured outputs (JSON, YAML, plain text), and native UNIX pipe integration.

```
                                      +--------------------+
                                      |      kiyo-cli      |
                                      +--------------------+
                                                |
        +-------------+------------+------------+------------+------------+-------------+-------------+
        |             |            |            |            |            |             |             |
        v             v            v            v            v            v             v             v
     [merge]       [split]       [diff]       [ocr]      [extract]     [watch]     [thumbnail]      [gui]
   (Zero-Copy)  (Page Burst)  (Visual/Text) (Sandwich)  (Markdown)   (Hot-Reload) (Nemo Thumbs) (GPU Viewer)
```

### 1.1. CLI Grammar & Command-Line Interface Definition

```rust
// crates/kiyo-cli/src/args.rs

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "kiyo",
    version = "0.1.0",
    about = "Ultra-high-performance PDF engine, viewer, and document manipulation suite",
    long_about = "Kiyo unifies GPU-accelerated viewing (GUI) and headless manipulation (CLI) into a single cohesive Rust system."
)]
pub struct Cli {
    #[arg(short, long, global = true, help = "Suppress all informational output")]
    pub quiet: bool,

    #[arg(short, long, global = true, help = "Output structured results as JSON for automation")]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(help = "Path to PDF file to open (defaults to launching GPU desktop viewer)")]
    pub default_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// Launch the high-framerate hardware-accelerated desktop viewer
    Gui(GuiArgs),

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

    /// Generate a raster thumbnail image for file managers (Nemo, Nautilus, Dolphin)
    Thumbnail(ThumbnailArgs),
}

#[derive(Args, Debug, PartialEq)]
pub struct MergeArgs {
    #[arg(required = true, num_args = 2.., help = "Input PDF files to merge in sequential order")]
    pub inputs: Vec<PathBuf>,

    #[arg(short, long, default_value = "merged.pdf", help = "Output destination file path")]
    pub output: PathBuf,

    #[arg(long, help = "Preserve and merge document outlines/bookmarks")]
    pub preserve_bookmarks: bool,
}

#[derive(Args, Debug, PartialEq)]
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

#[derive(Args, Debug, PartialEq)]
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

#[derive(Args, Debug, PartialEq)]
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

#[derive(Args, Debug, PartialEq)]
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

#[derive(Args, Debug, PartialEq)]
pub struct WatchArgs {
    #[arg(required = true, help = "Target PDF file to monitor")]
    pub file: PathBuf,

    #[arg(long, help = "Trigger shell command hook on each atomic reload")]
    pub on_reload: Option<String>,
}

#[derive(Args, Debug, PartialEq)]
pub struct GuiArgs {
    #[arg(help = "Path to PDF file to open")]
    pub file: Option<PathBuf>,

    #[arg(long, help = "Enable smart luminance-preserving dark mode")]
    pub dark: bool,
}

#[derive(Args, Debug, PartialEq)]
pub struct ThumbnailArgs {
    #[arg(required = true, help = "Source document path (%i)")]
    pub input: PathBuf,

    #[arg(required = true, help = "Output thumbnail image destination (%o)")]
    pub output: PathBuf,

    #[arg(default_value = "256", help = "Maximum thumbnail bounding size in pixels (%s)")]
    pub size: u32,

    #[arg(short, long, default_value = "0", help = "Page index to render (0-based)")]
    pub page: u32,
}
```

---

## 2. Linux Mint & Desktop Environment Integration

`kiyo-cli` serves as the primary integration engine for Linux Mint (Cinnamon / Nemo), GNOME (Nautilus), and KDE (Dolphin).

### 2.1. Linux Mint Nemo Actions (`kiyo.nemo_action`)

Located at `/usr/share/nemo/actions/kiyo.nemo_action` or `~/.local/share/nemo/actions/kiyo.nemo_action`:

```ini
[Nemo Action]
Active=true
Name=Kiyo PDF Actions
Comment=High-performance PDF viewing, OCR, diffing, and merging
Exec=kiyo gui %F
Icon-Name=application-pdf
Selection=any
Extensions=pdf;PDF;epub;EPUB;cbz;CBZ;cbr;CBR;cb7;CB7;cbt;CBT;djvu;DJVU;djv;DJV;oxps;OXPS;xps;XPS;mobi;MOBI;azw3;AZW3;fb2;FB2;
Quote=double

[Action Root]
Name=Kiyo Document Operations
Icon-Name=application-pdf

[Action item_open]
Name=Open in Kiyo (GPU Accelerated)
Exec=kiyo gui %F

[Action item_ocr]
Name=OCR: Convert to Searchable PDF
Exec=kiyo ocr --sandwich %F
Conditions=mimetype=application/pdf;

[Action item_diff]
Name=Visual Diff Revisions with Kiyo
Exec=kiyo diff %F
Conditions=selection_count=2;mimetype=application/pdf;

[Action item_merge]
Name=Merge Selected PDFs into Single Document
Exec=kiyo merge %F --output "merged_output.pdf"
Conditions=selection_count=>1;mimetype=application/pdf;
```

### 2.2. High-Speed File Manager Thumbnailer (`kiyo.thumbnailer`)

Located at `/usr/share/thumbnailers/kiyo.thumbnailer`:

```ini
[Thumbnailer Entry]
TryExec=kiyo
Exec=kiyo thumbnail %i %o %s
MimeType=application/pdf;application/x-pdf;application/epub+zip;application/x-cbz;application/x-cbr;application/x-cb7;application/x-cbt;image/vnd.djvu;application/oxps;application/vnd.ms-xpsdocument;application/x-mobipocket-ebook;application/vnd.amazon.ebook;application/x-fictionbook+xml;
```

When Nemo or Nautilus navigates into a folder with large multi-gigabyte PDF archives or comic book CBR/CBZ files, `kiyo thumbnail` renders crisp anti-aliased preview thumbnails directly into the system thumbnail cache (`~/.cache/thumbnails/normal/`) in under 5 milliseconds per document using memory-mapped zero-copy parsing.

### 2.3. Desktop Application Entry (`org.kiyo.Kiyo.desktop`)

Located at `/usr/share/applications/org.kiyo.Kiyo.desktop`:

```ini
[Desktop Entry]
Name=Kiyo
GenericName=PDF Viewer & Manipulator
Comment=High-framerate GPU-accelerated PDF viewer and document engine
Exec=kiyo gui %U
Terminal=false
Type=Application
Icon=org.kiyo.Kiyo
Categories=Office;Viewer;Graphics;Publishing;
MimeType=application/pdf;application/x-pdf;application/x-bzpdf;application/x-gzpdf;application/epub+zip;application/x-cbz;application/x-cbr;application/x-cb7;application/x-cbt;image/vnd.djvu;application/oxps;application/vnd.ms-xpsdocument;application/x-mobipocket-ebook;application/vnd.amazon.ebook;application/x-fictionbook+xml;text/markdown;
Keywords=PDF;Viewer;Document;OCR;Diff;GPU;Vim;EPUB;Comic;Manga;CBR;CBZ;DjVu;E-Reader;
Actions=DiffMode;

[Desktop Action DiffMode]
Name=Compare Two PDFs
Exec=kiyo diff %U
```

---

## 3. Headless Batch & Automation Workflows

### 3.1. High-Throughput PDF Merging
```bash
# Merge dozens of invoices into a unified ledger while preserving bookmarks
kiyo merge invoice_01.pdf invoice_02.pdf invoice_03.pdf --output monthly_ledger.pdf --preserve-bookmarks
```

### 3.2. Automated Chapter & Page Decomposition
```bash
# Decompose manual by top-level chapters into 'chapters/' folder
kiyo split manual.pdf --by-chapters -o ./chapters

# Burst entire document into single-page PDFs
kiyo split scan_bundle.pdf --burst -o ./burst_pages
```

### 3.3. Offline Neural OCR Pipeline
```bash
# Convert scanned archival document into an ISO 32000 searchable sandwich PDF
kiyo ocr archive_scan.pdf -o archive_searchable.pdf --sandwich --language en,de
```

### 3.4. Hardware-Accelerated Visual Diffing
```bash
# Compare contract revisions and output a difference heatmap PDF
kiyo diff contract_v1.pdf contract_v2.pdf --output-pdf contract_diff.pdf --threshold 0.05
```

### 3.5. Structured Document Extraction
```bash
# Extract multi-column text and tables into clean Markdown and dump embedded images
kiyo extract technical_paper.pdf --markdown --images -o ./extracted_paper
```

### 3.6. LaTeX/Typst Live Hot-Reload Loop
```bash
# Monitor compiled document and trigger browser/notification hook on update
kiyo watch paper.pdf --on-reload "notify-send 'Kiyo' 'Document updated!'"
```
