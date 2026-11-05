//! Command-line interface grammar and argument parsing for `kiyo-cli`.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// High-performance PDF engine, viewer, and document manipulation suite.
#[derive(Parser, Debug)]
#[command(
    name = "kiyo",
    version = "0.1.0",
    about = "Ultra-high-performance PDF engine, viewer, and document manipulation suite",
    long_about = "Kiyo unifies GPU-accelerated viewing (GUI) and headless manipulation (CLI) into a single cohesive Rust system."
)]
pub struct Cli {
    /// Suppress all informational output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Output structured results as JSON for automation
    #[arg(short, long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to PDF file to open (defaults to launching GPU desktop viewer)
    #[arg(value_name = "FILE")]
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

    /// Prepress N-up imposition, booklet saddle-stitching, mechanical creep, and printer marks
    Impose(ImposeArgs),

    /// Intelligent whitespace auto-cropping with running header/footer suppression
    CropMargins(CropMarginsArgs),

    /// Control active Kiyo GUI instance via UNIX domain socket IPC (or standalone kiyo-remote)
    Remote(RemoteArgs),

    /// Speculative document metadata and thumbnail prefetching daemon (systemd service)
    Prefetch(PrefetchArgs),
}

#[derive(Args, Debug, PartialEq)]
pub struct MergeArgs {
    /// Input PDF files to merge in sequential order
    #[arg(required = true, num_args = 2.., value_name = "INPUTS")]
    pub inputs: Vec<PathBuf>,

    /// Output destination file path
    #[arg(short, long, default_value = "merged.pdf", value_name = "OUTPUT")]
    pub output: PathBuf,

    /// Preserve and merge document outlines/bookmarks
    #[arg(long)]
    pub preserve_bookmarks: bool,
}

#[derive(Args, Debug, PartialEq)]
pub struct SplitArgs {
    /// Source PDF document to split
    #[arg(required = true, value_name = "INPUT")]
    pub input: PathBuf,

    /// Page range specification (e.g. '1-5,8,12-end')
    #[arg(short, long, value_name = "RANGES")]
    pub pages: Option<String>,

    /// Split every single page into an independent PDF file
    #[arg(long)]
    pub burst: bool,

    /// Split document automatically at top-level table of contents bookmarks
    #[arg(long)]
    pub by_chapters: bool,

    /// Output destination directory
    #[arg(short, long, default_value = "dist", value_name = "DIR")]
    pub output_dir: PathBuf,
}

#[derive(Args, Debug, PartialEq)]
pub struct DiffArgs {
    /// Baseline revision PDF (Revision A)
    #[arg(required = true, value_name = "FILE_A")]
    pub file_a: PathBuf,

    /// Modified revision PDF (Revision B)
    #[arg(required = true, value_name = "FILE_B")]
    pub file_b: PathBuf,

    /// Generate a visual composite PDF showing colored differences
    #[arg(short, long, value_name = "DIFF_PDF")]
    pub output_pdf: Option<PathBuf>,

    /// Visual pixel difference sensitivity threshold [0.0 - 1.0]
    #[arg(long, default_value = "0.08")]
    pub threshold: f32,
}

#[derive(Args, Debug, PartialEq)]
pub struct OcrArgs {
    /// Input scanned PDF or image document
    #[arg(required = true, value_name = "INPUT")]
    pub input: PathBuf,

    /// Output destination for the searchable PDF
    #[arg(short, long, value_name = "OUTPUT")]
    pub output: Option<PathBuf>,

    /// Inject invisible text layer over original image (ISO 32000 3 Tr)
    #[arg(long, default_value = "true")]
    pub sandwich: bool,

    /// Target language codes (e.g. 'en,de,fr')
    #[arg(short, long, value_name = "LANG")]
    pub language: Option<String>,
}

#[derive(Args, Debug, PartialEq)]
pub struct ExtractArgs {
    /// Input PDF file
    #[arg(required = true, value_name = "INPUT")]
    pub input: PathBuf,

    /// Reconstruct two-column text and tables into clean Markdown format
    #[arg(long)]
    pub markdown: bool,

    /// Extract all embedded raster images into the output folder
    #[arg(long)]
    pub images: bool,

    /// Output directory
    #[arg(short, long, default_value = "./extracted", value_name = "DIR")]
    pub output_dir: PathBuf,
}

#[derive(Args, Debug, PartialEq)]
pub struct WatchArgs {
    /// Target PDF file to monitor
    #[arg(required = true, value_name = "FILE")]
    pub file: PathBuf,

    /// Trigger shell command hook on each atomic reload
    #[arg(long, value_name = "CMD")]
    pub on_reload: Option<String>,
}

#[derive(Args, Debug, PartialEq)]
pub struct GuiArgs {
    /// Path to document file(s) to open
    #[arg(value_name = "FILES")]
    pub files: Vec<PathBuf>,

    /// Enable smart luminance-preserving dark mode
    #[arg(long)]
    pub dark: bool,
}

impl GuiArgs {
    /// Returns the primary target document path, if provided.
    pub fn file(&self) -> Option<&std::path::Path> {
        self.files.first().map(|p| p.as_path())
    }
}

#[derive(Args, Debug, PartialEq)]
pub struct ThumbnailArgs {
    /// Source document path (%i)
    #[arg(required = true, value_name = "INPUT")]
    pub input: PathBuf,

    /// Output thumbnail image destination (%o)
    #[arg(required = true, value_name = "OUTPUT")]
    pub output: PathBuf,

    /// Maximum thumbnail bounding size in pixels (%s, default 256)
    #[arg(value_name = "SIZE", default_value = "256")]
    pub size: u32,

    /// Page index to render (0-based)
    #[arg(short, long, default_value = "0")]
    pub page: u32,
}

#[derive(Args, Debug, PartialEq)]
pub struct ImposeArgs {
    /// Input PDF file to impose
    #[arg(required = true, value_name = "INPUT")]
    pub input: PathBuf,

    /// Output destination file path
    #[arg(short, long, default_value = "imposed.pdf", value_name = "OUTPUT")]
    pub output: PathBuf,

    /// N-up sheet page layout (e.g. 1, 2, 4, 8)
    #[arg(long, default_value = "1")]
    pub nup: u8,

    /// Impose as folded saddle-stitch booklet
    #[arg(long)]
    pub booklet: bool,

    /// Mechanical paper thickness/creep delta in pt (0.1mm = 0.2835pt)
    #[arg(long, default_value = "0.2835")]
    pub creep: f32,

    /// Embed prepress printer marks (trim ticks, bleed ticks, fold marks, CMYK wedges)
    #[arg(long)]
    pub marks: bool,

    /// Gutter spacing between imposed pages in points
    #[arg(long, default_value = "14.17")]
    pub gutter: f32,
}

#[derive(Args, Debug, PartialEq)]
pub struct CropMarginsArgs {
    /// Input document with wide margins
    #[arg(required = true, value_name = "INPUT")]
    pub input: PathBuf,

    /// Output destination file path
    #[arg(short, long, default_value = "cropped.pdf", value_name = "OUTPUT")]
    pub output: PathBuf,

    /// Detect and suppress running headers and footers from margin limits
    #[arg(long, default_value = "true")]
    pub preserve_headers: bool,

    /// Safety breathing padding around content text in points
    #[arg(long, default_value = "12.0")]
    pub padding: f32,

    /// Luminance threshold to consider a pixel white whitespace [0.0 - 1.0]
    #[arg(long, default_value = "0.98")]
    pub threshold: f32,
}

#[derive(Args, Debug, PartialEq)]
pub struct RemoteArgs {
    #[command(subcommand)]
    pub action: RemoteAction,

    /// Specific Kiyo PID to target (defaults to active focused instance)
    #[arg(long)]
    pub pid: Option<u32>,
}

use serde::{Deserialize, Serialize};

#[derive(Subcommand, Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload")]
pub enum RemoteAction {
    /// Advance 1 page (respects comic RTL progression)
    NextPage,
    /// Move backward 1 page
    PrevPage,
    /// Jump directly to target page number
    Goto {
        #[arg(value_name = "PAGE")]
        page: usize,
    },
    /// Modify zoom factor ('in', 'out', 'fit-width', 'fit-page', or float like '1.5')
    Zoom {
        #[arg(value_name = "LEVEL")]
        level: String,
    },
    /// Toggle Smart Oklab Dark Mode
    ToggleDark,
    /// Teleport viewport/cursor to Hop/Avy jump tag
    JumpTag {
        #[arg(value_name = "TAG")]
        tag: String,
    },
    /// Retrieve JSON document and viewport status
    GetStatus,
}

#[derive(Args, Debug, PartialEq)]
pub struct PrefetchArgs {
    /// Directories to monitor for incoming documents (e.g. Downloads, Documents)
    #[arg(long, num_args = 1.., value_name = "DIRS")]
    pub dirs: Vec<PathBuf>,

    /// Maximum thumbnail cache limit in megabytes
    #[arg(long, default_value = "256", value_name = "MB")]
    pub max_cache_mb: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_merge_args_parsing() {
        let args = Cli::try_parse_from([
            "kiyo",
            "merge",
            "doc1.pdf",
            "doc2.pdf",
            "--output",
            "combined.pdf",
            "--preserve-bookmarks",
        ])
        .expect("Valid merge command");

        match args.command {
            Some(Commands::Merge(merge)) => {
                assert_eq!(
                    merge.inputs,
                    vec![PathBuf::from("doc1.pdf"), PathBuf::from("doc2.pdf")]
                );
                assert_eq!(merge.output, PathBuf::from("combined.pdf"));
                assert!(merge.preserve_bookmarks);
            }
            _ => panic!("Expected Merge subcommand"),
        }
    }

    #[test]
    fn test_split_args_parsing() {
        let args = Cli::try_parse_from([
            "kiyo",
            "split",
            "book.pdf",
            "-p",
            "1-10,15",
            "--by-chapters",
            "--output-dir",
            "out_chapters",
        ])
        .expect("Valid split command");

        match args.command {
            Some(Commands::Split(split)) => {
                assert_eq!(split.input, PathBuf::from("book.pdf"));
                assert_eq!(split.pages.as_deref(), Some("1-10,15"));
                assert!(split.by_chapters);
                assert!(!split.burst);
                assert_eq!(split.output_dir, PathBuf::from("out_chapters"));
            }
            _ => panic!("Expected Split subcommand"),
        }
    }

    #[test]
    fn test_diff_args_parsing() {
        let args = Cli::try_parse_from([
            "kiyo",
            "diff",
            "rev_a.pdf",
            "rev_b.pdf",
            "--output-pdf",
            "diff_result.pdf",
            "--threshold",
            "0.15",
        ])
        .expect("Valid diff command");

        match args.command {
            Some(Commands::Diff(diff)) => {
                assert_eq!(diff.file_a, PathBuf::from("rev_a.pdf"));
                assert_eq!(diff.file_b, PathBuf::from("rev_b.pdf"));
                assert_eq!(diff.output_pdf, Some(PathBuf::from("diff_result.pdf")));
                assert!((diff.threshold - 0.15).abs() < 1e-5);
            }
            _ => panic!("Expected Diff subcommand"),
        }
    }

    #[test]
    fn test_ocr_and_extract_parsing() {
        let ocr_cli = Cli::try_parse_from([
            "kiyo",
            "ocr",
            "scan.pdf",
            "-o",
            "ocr_out.pdf",
            "--sandwich",
            "--language",
            "en,fr",
        ])
        .expect("Valid ocr command");

        match ocr_cli.command {
            Some(Commands::Ocr(ocr)) => {
                assert_eq!(ocr.input, PathBuf::from("scan.pdf"));
                assert_eq!(ocr.output, Some(PathBuf::from("ocr_out.pdf")));
                assert!(ocr.sandwich);
                assert_eq!(ocr.language.as_deref(), Some("en,fr"));
            }
            _ => panic!("Expected Ocr subcommand"),
        }

        let extract_cli = Cli::try_parse_from([
            "kiyo",
            "extract",
            "paper.pdf",
            "--markdown",
            "--images",
            "-o",
            "assets",
        ])
        .expect("Valid extract command");

        match extract_cli.command {
            Some(Commands::Extract(extract)) => {
                assert_eq!(extract.input, PathBuf::from("paper.pdf"));
                assert!(extract.markdown);
                assert!(extract.images);
                assert_eq!(extract.output_dir, PathBuf::from("assets"));
            }
            _ => panic!("Expected Extract subcommand"),
        }
    }

    #[test]
    fn test_default_file_and_flags() {
        let cli = Cli::try_parse_from(["kiyo", "--quiet", "--json", "document.pdf"])
            .expect("Valid default file parsing");
        assert!(cli.quiet);
        assert!(cli.json);
        assert_eq!(cli.default_file, Some(PathBuf::from("document.pdf")));
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_thumbnail_args_parsing() {
        let thumb_cli = Cli::try_parse_from([
            "kiyo",
            "thumbnail",
            "book.pdf",
            "thumb.png",
            "512",
            "--page",
            "2",
        ])
        .expect("Valid thumbnail command");

        match thumb_cli.command {
            Some(Commands::Thumbnail(thumb)) => {
                assert_eq!(thumb.input, PathBuf::from("book.pdf"));
                assert_eq!(thumb.output, PathBuf::from("thumb.png"));
                assert_eq!(thumb.size, 512);
                assert_eq!(thumb.page, 2);
            }
            _ => panic!("Expected Thumbnail subcommand"),
        }
    }

    #[test]
    fn test_gui_args_parsing() {
        // Blank GUI launch
        let blank_gui = Cli::try_parse_from(["kiyo", "gui"]).expect("Valid blank gui launch");
        match blank_gui.command {
            Some(Commands::Gui(gui)) => {
                assert!(gui.files.is_empty());
                assert_eq!(gui.file(), None);
                assert!(!gui.dark);
            }
            _ => panic!("Expected Gui subcommand"),
        }

        // Single file GUI launch
        let single_gui = Cli::try_parse_from(["kiyo", "gui", "single.pdf", "--dark"])
            .expect("Valid single file gui launch");
        match single_gui.command {
            Some(Commands::Gui(gui)) => {
                assert_eq!(gui.files, vec![PathBuf::from("single.pdf")]);
                assert_eq!(gui.file(), Some(std::path::Path::new("single.pdf")));
                assert!(gui.dark);
            }
            _ => panic!("Expected Gui subcommand"),
        }

        // Multi-file GUI launch (%U / %F desktop / Nemo compatibility)
        let multi_gui =
            Cli::try_parse_from(["kiyo", "gui", "doc1.pdf", "doc2.pdf", "doc3.pdf", "--dark"])
                .expect("Valid multi-file gui launch");
        match multi_gui.command {
            Some(Commands::Gui(gui)) => {
                assert_eq!(
                    gui.files,
                    vec![
                        PathBuf::from("doc1.pdf"),
                        PathBuf::from("doc2.pdf"),
                        PathBuf::from("doc3.pdf")
                    ]
                );
                assert_eq!(gui.file(), Some(std::path::Path::new("doc1.pdf")));
                assert!(gui.dark);
            }
            _ => panic!("Expected Gui subcommand"),
        }
    }

    #[test]
    fn test_watch_args_parsing() {
        let watch_cli = Cli::try_parse_from([
            "kiyo",
            "watch",
            "paper.pdf",
            "--on-reload",
            "notify-send 'Updated'",
        ])
        .expect("Valid watch command");

        match watch_cli.command {
            Some(Commands::Watch(watch)) => {
                assert_eq!(watch.file, PathBuf::from("paper.pdf"));
                assert_eq!(watch.on_reload.as_deref(), Some("notify-send 'Updated'"));
            }
            _ => panic!("Expected Watch subcommand"),
        }
    }

    #[test]
    fn test_tui_retired_and_rejected() {
        // Invoking "tui" with an argument should be rejected as invalid positional arg
        // because "tui" is treated as the default file, making "doc.pdf" an unexpected argument.
        let result = Cli::try_parse_from(["kiyo", "tui", "doc.pdf"]);
        assert!(result.is_err(), "TUI subcommand must be rejected");
    }

    #[test]
    fn test_impose_args_parsing() {
        let impose_cli = Cli::try_parse_from([
            "kiyo",
            "impose",
            "source.pdf",
            "--output",
            "catalog_booklet.pdf",
            "--nup",
            "2",
            "--booklet",
            "--creep",
            "0.2835",
            "--marks",
            "--gutter",
            "18.0",
        ])
        .expect("Valid impose command");

        match impose_cli.command {
            Some(Commands::Impose(impose)) => {
                assert_eq!(impose.input, PathBuf::from("source.pdf"));
                assert_eq!(impose.output, PathBuf::from("catalog_booklet.pdf"));
                assert_eq!(impose.nup, 2);
                assert!(impose.booklet);
                assert!((impose.creep - 0.2835).abs() < 1e-5);
                assert!(impose.marks);
                assert!((impose.gutter - 18.0).abs() < 1e-5);
            }
            _ => panic!("Expected Impose subcommand"),
        }
    }

    #[test]
    fn test_crop_margins_args_parsing() {
        let crop_cli = Cli::try_parse_from([
            "kiyo",
            "crop-margins",
            "scan.pdf",
            "-o",
            "cropped.pdf",
            "--preserve-headers",
            "--padding",
            "14.5",
            "--threshold",
            "0.95",
        ])
        .expect("Valid crop-margins command");

        match crop_cli.command {
            Some(Commands::CropMargins(crop)) => {
                assert_eq!(crop.input, PathBuf::from("scan.pdf"));
                assert_eq!(crop.output, PathBuf::from("cropped.pdf"));
                assert!(crop.preserve_headers);
                assert!((crop.padding - 14.5).abs() < 1e-5);
                assert!((crop.threshold - 0.95).abs() < 1e-5);
            }
            _ => panic!("Expected CropMargins subcommand"),
        }
    }

    #[test]
    fn test_remote_args_parsing() {
        // next-page
        let next_cli =
            Cli::try_parse_from(["kiyo", "remote", "next-page"]).expect("Valid remote next-page");
        match next_cli.command {
            Some(Commands::Remote(remote)) => {
                assert_eq!(remote.action, RemoteAction::NextPage);
                assert_eq!(remote.pid, None);
            }
            _ => panic!("Expected Remote subcommand"),
        }

        // goto with PID
        let goto_cli = Cli::try_parse_from(["kiyo", "remote", "--pid", "12345", "goto", "42"])
            .expect("Valid remote goto");
        match goto_cli.command {
            Some(Commands::Remote(remote)) => {
                assert_eq!(remote.action, RemoteAction::Goto { page: 42 });
                assert_eq!(remote.pid, Some(12345));
            }
            _ => panic!("Expected Remote subcommand"),
        }

        // zoom level
        let zoom_cli = Cli::try_parse_from(["kiyo", "remote", "zoom", "fit-width"])
            .expect("Valid remote zoom");
        match zoom_cli.command {
            Some(Commands::Remote(remote)) => {
                assert_eq!(
                    remote.action,
                    RemoteAction::Zoom {
                        level: "fit-width".into()
                    }
                );
            }
            _ => panic!("Expected Remote subcommand"),
        }

        // toggle-dark, jump-tag, get-status
        let dark_cli = Cli::try_parse_from(["kiyo", "remote", "toggle-dark"])
            .expect("Valid remote toggle-dark");
        assert_eq!(
            dark_cli.command,
            Some(Commands::Remote(RemoteArgs {
                action: RemoteAction::ToggleDark,
                pid: None
            }))
        );

        let jump_cli = Cli::try_parse_from(["kiyo", "remote", "jump-tag", "ab"])
            .expect("Valid remote jump-tag");
        assert_eq!(
            jump_cli.command,
            Some(Commands::Remote(RemoteArgs {
                action: RemoteAction::JumpTag { tag: "ab".into() },
                pid: None
            }))
        );

        let status_cli =
            Cli::try_parse_from(["kiyo", "remote", "get-status"]).expect("Valid remote get-status");
        assert_eq!(
            status_cli.command,
            Some(Commands::Remote(RemoteArgs {
                action: RemoteAction::GetStatus,
                pid: None
            }))
        );
    }

    #[test]
    fn test_prefetch_args_parsing() {
        let prefetch_cli = Cli::try_parse_from([
            "kiyo",
            "prefetch",
            "--dirs",
            "/home/user/Downloads",
            "/home/user/Documents",
            "--max-cache-mb",
            "512",
        ])
        .expect("Valid prefetch command");

        match prefetch_cli.command {
            Some(Commands::Prefetch(prefetch)) => {
                assert_eq!(
                    prefetch.dirs,
                    vec![
                        PathBuf::from("/home/user/Downloads"),
                        PathBuf::from("/home/user/Documents")
                    ]
                );
                assert_eq!(prefetch.max_cache_mb, 512);
            }
            _ => panic!("Expected Prefetch subcommand"),
        }
    }
}
