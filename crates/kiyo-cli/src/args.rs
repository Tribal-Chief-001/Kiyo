//! Command-line interface grammar and argument parsing for `kiyo-cli`.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// High-performance PDF engine, viewer, and document manipulation suite.
#[derive(Parser, Debug)]
#[command(
    name = "kiyo",
    version = "0.1.0",
    about = "Ultra-high-performance PDF engine, viewer, and document manipulation suite",
    long_about = "Kiyo unifies GPU-accelerated viewing (GUI), terminal graphical viewing (TUI), and headless manipulation (CLI) into a single cohesive Rust system."
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

    /// Path to PDF file to open (defaults to launching GUI or TUI)
    #[arg(value_name = "FILE")]
    pub default_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug, PartialEq)]
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

    /// Generate a raster thumbnail image for file managers (Nemo, Nautilus, Dolphin)
    Thumbnail(ThumbnailArgs),
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
    /// Path to PDF file to open
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Enable smart luminance-preserving dark mode
    #[arg(long)]
    pub dark: bool,
}

#[derive(Args, Debug, PartialEq)]
pub struct TuiArgs {
    /// Path to PDF file to open
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Forced terminal graphics protocol
    #[arg(long, value_enum, default_value = "auto")]
    pub graphics: TerminalGraphicsMode,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalGraphicsMode {
    Auto,
    Kitty,
    Sixel,
    Iterm2,
    Halfblock,
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
}
