use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "kiyo",
    version = "0.1.0",
    about = "Ultra-high-performance PDF engine, viewer, and document manipulation suite"
)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[arg(short, long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    pub default_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Gui(GuiArgs),
    Tui(TuiArgs),
    Merge(MergeArgs),
    Split(SplitArgs),
    Diff(DiffArgs),
    Ocr(OcrArgs),
    Extract(ExtractArgs),
    Watch(WatchArgs),
}

#[derive(Args, Debug)]
pub struct MergeArgs {
    #[arg(required = true, num_args = 2..)]
    pub inputs: Vec<PathBuf>,
    #[arg(short, long, default_value = "merged.pdf")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct SplitArgs {
    #[arg(required = true)]
    pub input: PathBuf,
    #[arg(short, long)]
    pub pages: Option<String>,
    #[arg(long)]
    pub burst: bool,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    #[arg(required = true)]
    pub file_a: PathBuf,
    #[arg(required = true)]
    pub file_b: PathBuf,
}

#[derive(Args, Debug)]
pub struct OcrArgs {
    #[arg(required = true)]
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value = "true")]
    pub sandwich: bool,
}

#[derive(Args, Debug)]
pub struct ExtractArgs {
    #[arg(required = true)]
    pub input: PathBuf,
    #[arg(long)]
    pub markdown: bool,
}

#[derive(Args, Debug)]
pub struct WatchArgs {
    #[arg(required = true)]
    pub file: PathBuf,
}

#[derive(Args, Debug)]
pub struct GuiArgs {
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub dark: bool,
}

#[derive(Args, Debug)]
pub struct TuiArgs {
    pub file: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "auto")]
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
