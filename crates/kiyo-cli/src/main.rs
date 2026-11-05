use clap::Parser;
use kiyo_cli::args::{Cli, Commands};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Gui(gui)) => {
            if !cli.quiet {
                match gui.files.len() {
                    0 => println!("Launching Kiyo GPU desktop viewer for: blank"),
                    1 => println!(
                        "Launching Kiyo GPU desktop viewer for: {:?}",
                        gui.files[0]
                    ),
                    n => println!(
                        "Launching Kiyo GPU desktop viewer for {} documents: {:?}",
                        n, gui.files
                    ),
                }
            }
            let app = kiyo_gui::GuiApp::new()?;
            app.run()?;
        }
        Some(Commands::Merge(merge)) => {
            if !cli.quiet {
                println!(
                    "Merging {} PDF files into {:?}",
                    merge.inputs.len(),
                    merge.output
                );
            }
        }
        Some(Commands::Split(split)) => {
            if !cli.quiet {
                println!(
                    "Splitting document {:?} (burst: {}, by_chapters: {}) into {:?}",
                    split.input, split.burst, split.by_chapters, split.output_dir
                );
            }
        }
        Some(Commands::Diff(diff)) => {
            if !cli.quiet {
                println!(
                    "Diffing revisions {:?} vs {:?} (threshold: {})",
                    diff.file_a, diff.file_b, diff.threshold
                );
            }
        }
        Some(Commands::Ocr(ocr)) => {
            if !cli.quiet {
                println!(
                    "Running neural OCR on {:?} (sandwich: {}, lang: {:?})",
                    ocr.input, ocr.sandwich, ocr.language
                );
            }
        }
        Some(Commands::Extract(extract)) => {
            if !cli.quiet {
                println!(
                    "Extracting content from {:?} (markdown: {}, images: {}) to {:?}",
                    extract.input, extract.markdown, extract.images, extract.output_dir
                );
            }
        }
        Some(Commands::Watch(watch)) => {
            if !cli.quiet {
                println!(
                    "Watching {:?} with reload hook {:?}",
                    watch.file, watch.on_reload
                );
            }
        }
        Some(Commands::Thumbnail(thumb)) => {
            if !cli.quiet {
                println!(
                    "Generating {}x{} thumbnail for {:?} into {:?} (page {})",
                    thumb.size, thumb.size, thumb.input, thumb.output, thumb.page
                );
            }
        }
        Some(Commands::Impose(impose)) => {
            if !cli.quiet {
                println!(
                    "Imposing document {:?} into {:?} (nup: {}, booklet: {}, creep: {}, marks: {})",
                    impose.input,
                    impose.output,
                    impose.nup,
                    impose.booklet,
                    impose.creep,
                    impose.marks
                );
            }
        }
        Some(Commands::CropMargins(crop)) => {
            if !cli.quiet {
                println!(
                    "Auto-cropping margins for {:?} into {:?} (preserve_headers: {}, padding: {}, threshold: {})",
                    crop.input,
                    crop.output,
                    crop.preserve_headers,
                    crop.padding,
                    crop.threshold
                );
            }
        }
        Some(Commands::Remote(remote)) => {
            if !cli.quiet {
                println!(
                    "Dispatching remote action {:?} to instance (pid: {:?})",
                    remote.action, remote.pid
                );
            }
        }
        Some(Commands::Prefetch(prefetch)) => {
            if !cli.quiet {
                println!(
                    "Starting prefetch daemon for {:?} (max cache: {} MB)",
                    prefetch.dirs, prefetch.max_cache_mb
                );
            }
        }
        None => {
            if let Some(file) = &cli.default_file {
                if !cli.quiet {
                    println!("Opening {:?} with default Kiyo viewer", file);
                }
                let app = kiyo_gui::GuiApp::new()?;
                app.run()?;
            } else if !cli.quiet {
                println!("Kiyo PDF Engine v0.1.0 — Run 'kiyo --help' for commands");
            }
        }
    }

    Ok(())
}
