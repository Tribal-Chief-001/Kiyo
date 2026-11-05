#![allow(clippy::collapsible_if)]

use anyhow::Result;
use clap::Parser;
use kiyo_cli::args::{Cli, Commands};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

fn main() -> Result<()> {
    // Sanitize Linux locale environment to guarantee valid UTF-8 for libxkbcommon & X11 Compose
    #[cfg(target_os = "linux")]
    {
        let needs_utf8 = |var: &str| {
            std::env::var(var)
                .map(|v| !v.is_empty() && !v.to_lowercase().contains("utf"))
                .unwrap_or(false)
        };
        // SAFETY: Called synchronously at the very entry of main() before any threads are spawned.
        unsafe {
            if needs_utf8("LANG") {
                if let Ok(lang) = std::env::var("LANG") {
                    std::env::set_var("LANG", format!("{}.UTF-8", lang));
                }
            }
            if needs_utf8("LC_CTYPE") || needs_utf8("LC_ALL") || needs_utf8("LANG") {
                std::env::set_var("LC_CTYPE", "C.UTF-8");
            }
        }
    }

    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Gui(gui)) => {
            if !cli.quiet {
                match gui.files.len() {
                    0 => println!("Launching Kiyo GPU desktop viewer for: blank"),
                    1 => println!("Launching Kiyo GPU desktop viewer for: {:?}", gui.files[0]),
                    n => println!(
                        "Launching Kiyo GPU desktop viewer for {} documents: {:?}",
                        n, gui.files
                    ),
                }
            }
            let app = if let Some(file) = gui.file() {
                kiyo_gui::GuiApp::with_file(file)?
            } else {
                kiyo_gui::GuiApp::new()?
            };
            if gui.dark {
                let mut state = app.state.lock().unwrap();
                state.dark_mode = true;
            }
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
            kiyo_core::manipulation::merge_documents(
                &merge.inputs,
                &merge.output,
                merge.preserve_bookmarks,
            )?;
            if !cli.quiet {
                println!("Successfully created merged document: {:?}", merge.output);
            }
        }
        Some(Commands::Split(split)) => {
            if !cli.quiet {
                println!(
                    "Splitting document {:?} (burst: {}, by_chapters: {}) into {:?}",
                    split.input, split.burst, split.by_chapters, split.output_dir
                );
            }
            let generated = kiyo_core::manipulation::split_document(
                &split.input,
                split.pages.as_deref(),
                split.by_chapters,
                split.burst,
                &split.output_dir,
            )?;
            if !cli.quiet {
                println!(
                    "Split generated {} files in {:?}",
                    generated.len(),
                    split.output_dir
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
            use kiyo_render::RenderBackend;
            let doc_a: Arc<dyn kiyo_core::universal::UniversalDocument> =
                Arc::from(kiyo_render::open_universal_document(&diff.file_a)?);
            let doc_b: Arc<dyn kiyo_core::universal::UniversalDocument> =
                Arc::from(kiyo_render::open_universal_document(&diff.file_b)?);

            let backend_a = kiyo_render::UniversalRenderBackend::new(doc_a);
            let tile_a = backend_a.render_tile(
                kiyo_render::TileKey {
                    page_index: 0,
                    level_of_detail: 1,
                    tile_col: 0,
                    tile_row: 0,
                },
                1.0,
            )?;

            let backend_b = kiyo_render::UniversalRenderBackend::new(doc_b);
            let tile_b = backend_b.render_tile(
                kiyo_render::TileKey {
                    page_index: 0,
                    level_of_detail: 1,
                    tile_col: 0,
                    tile_row: 0,
                },
                1.0,
            )?;

            let diff_pixels = kiyo_gpu::render_visual_diff(
                &tile_a.pixels,
                &tile_b.pixels,
                tile_a.width,
                tile_a.height,
                diff.threshold,
            )?;

            if let Some(out_pdf) = &diff.output_pdf {
                kiyo_gpu::generate_diff_pdf(&diff_pixels, tile_a.width, tile_a.height, out_pdf)?;
                if !cli.quiet {
                    println!("Visual difference PDF written to {:?}", out_pdf);
                }
            } else if !cli.quiet {
                println!(
                    "Visual difference calculated across {} pixels.",
                    tile_a.width * tile_a.height
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
            let pipeline = kiyo_ocr::OcrPipeline::new()?;
            let out_path = ocr.output.clone().unwrap_or_else(|| {
                let stem = ocr.input.file_stem().unwrap_or_default().to_string_lossy();
                ocr.input.with_file_name(format!("{stem}_ocr.pdf"))
            });
            let word_count =
                pipeline.process_document(&ocr.input, &out_path, ocr.language.as_deref())?;
            if !cli.quiet {
                println!(
                    "OCR finished: {word_count} words recognized, output written to {:?}",
                    out_path
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
            use kiyo_render::RenderBackend;
            std::fs::create_dir_all(&extract.output_dir)?;
            let doc: Arc<dyn kiyo_core::universal::UniversalDocument> =
                Arc::from(kiyo_render::open_universal_document(&extract.input)?);
            let backend = kiyo_render::UniversalRenderBackend::new(doc);
            let pages = backend.page_count();

            if extract.markdown || (!extract.markdown && !extract.images) {
                let mut full_text = String::new();
                for p in 0..pages {
                    if let Ok(runs) = backend.extract_text_runs(p) {
                        full_text.push_str(&format!("\n## Page {}\n\n", p + 1));
                        for r in runs {
                            full_text.push_str(&r.text);
                            full_text.push('\n');
                        }
                    }
                }
                let md_path = extract.output_dir.join("document_extracted.md");
                std::fs::write(&md_path, full_text)?;
                if !cli.quiet {
                    println!("Extracted text written to {:?}", md_path);
                }
            }

            if extract.images {
                for p in 0..pages {
                    if let Ok(tile) = backend.render_tile(
                        kiyo_render::TileKey {
                            page_index: p,
                            level_of_detail: 1,
                            tile_col: 0,
                            tile_row: 0,
                        },
                        1.0,
                    ) {
                        if let Some(img) =
                            image::RgbaImage::from_raw(tile.width, tile.height, tile.pixels)
                        {
                            let img_path = extract.output_dir.join(format!("page_{}.png", p + 1));
                            let _ = img.save(&img_path);
                        }
                    }
                }
                if !cli.quiet {
                    println!("Rendered page images saved into {:?}", extract.output_dir);
                }
            }
        }
        Some(Commands::Watch(watch)) => {
            if !cli.quiet {
                println!(
                    "Watching {:?} with reload hook {:?}",
                    watch.file, watch.on_reload
                );
            }
            use notify::{RecursiveMode, Watcher};
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx)?;
            if watch.file.exists() {
                watcher.watch(&watch.file, RecursiveMode::NonRecursive)?;
                if !cli.quiet {
                    println!("Watcher started on {:?}. Press Ctrl+C to stop.", watch.file);
                }
                for res in rx {
                    match res {
                        Ok(event) => {
                            if event.kind.is_modify() {
                                if let Some(cmd) = &watch.on_reload {
                                    let _ = std::process::Command::new("sh")
                                        .arg("-c")
                                        .arg(cmd)
                                        .status();
                                }
                            }
                        }
                        Err(e) => tracing::warn!("Watch error: {e:?}"),
                    }
                }
            } else {
                anyhow::bail!("Target watch file does not exist: {:?}", watch.file);
            }
        }
        Some(Commands::Thumbnail(thumb)) => {
            if !cli.quiet {
                println!(
                    "Generating {}x{} thumbnail for {:?} into {:?} (page {})",
                    thumb.size, thumb.size, thumb.input, thumb.output, thumb.page
                );
            }
            use kiyo_render::RenderBackend;
            let doc: Arc<dyn kiyo_core::universal::UniversalDocument> =
                Arc::from(kiyo_render::open_universal_document(&thumb.input)?);
            let backend = kiyo_render::UniversalRenderBackend::new(doc);
            let tile = backend.render_tile(
                kiyo_render::TileKey {
                    page_index: thumb.page,
                    level_of_detail: 1,
                    tile_col: 0,
                    tile_row: 0,
                },
                1.0,
            )?;
            let img = image::RgbaImage::from_raw(tile.width, tile.height, tile.pixels)
                .ok_or_else(|| anyhow::anyhow!("Failed to construct RGBA image from tile"))?;
            let resized = image::imageops::resize(
                &img,
                thumb.size,
                thumb.size,
                image::imageops::FilterType::Lanczos3,
            );
            if let Some(parent) = thumb.output.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            resized.save(&thumb.output)?;
            if !cli.quiet {
                println!("Thumbnail generated at {:?}", thumb.output);
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
            kiyo_core::manipulation::impose_document(
                &impose.input,
                &impose.output,
                impose.nup,
                impose.booklet,
                impose.creep,
                impose.marks,
                impose.gutter,
            )?;
            if !cli.quiet {
                println!("Imposed document written to {:?}", impose.output);
            }
        }
        Some(Commands::CropMargins(crop)) => {
            if !cli.quiet {
                println!(
                    "Auto-cropping margins for {:?} into {:?} (preserve_headers: {}, padding: {}, threshold: {})",
                    crop.input, crop.output, crop.preserve_headers, crop.padding, crop.threshold
                );
            }
            kiyo_core::manipulation::crop_document_margins(
                &crop.input,
                &crop.output,
                crop.preserve_headers,
                crop.padding,
                crop.threshold,
            )?;
            if !cli.quiet {
                println!("Cropped document written to {:?}", crop.output);
            }
        }
        Some(Commands::Remote(remote)) => {
            let dir = kiyo_gui::IpcServer::socket_dir();
            let socket_path = if let Some(pid) = remote.pid {
                dir.join(format!("{pid}.sock"))
            } else {
                dir.join("active.sock")
            };

            if !socket_path.exists() {
                println!("No active Kiyo instance found at {:?}", socket_path);
                return Ok(());
            }

            let mut stream = UnixStream::connect(&socket_path)?;
            let json_bytes = serde_json::to_vec(&remote.action)?;
            stream.write_all(&json_bytes)?;
            stream.shutdown(std::net::Shutdown::Write)?;

            let mut response = String::new();
            stream.read_to_string(&mut response)?;
            if !response.trim().is_empty() {
                println!("{response}");
            } else if !cli.quiet {
                println!("Remote command dispatched.");
            }
        }
        Some(Commands::Prefetch(prefetch)) => {
            if !cli.quiet {
                println!(
                    "Starting prefetch daemon for {:?} (max cache: {} MB)",
                    prefetch.dirs, prefetch.max_cache_mb
                );
            }
            for d in &prefetch.dirs {
                if d.exists() {
                    if let Ok(entries) = std::fs::read_dir(d) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if ["pdf", "cbz", "cbr", "epub"].contains(&ext.as_str()) {
                                let _ = kiyo_render::open_universal_document(&path);
                            }
                        }
                    }
                }
            }
            if !cli.quiet {
                println!("Prefetch pass completed.");
            }
        }
        None => {
            if let Some(file) = &cli.default_file {
                if !cli.quiet {
                    println!("Opening {:?} with default Kiyo viewer", file);
                }
                let app = kiyo_gui::GuiApp::with_file(file)?;
                app.run()?;
            } else if !cli.quiet {
                println!("Kiyo PDF Engine v0.1.0 — Run 'kiyo --help' for commands");
            }
        }
    }

    Ok(())
}
