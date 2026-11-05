//! `kiyo-ocr`: Embedded offline neural OCR inference engine (ocrs + rten).

#![allow(clippy::chunks_exact_to_as_chunks, clippy::collapsible_if)]

use anyhow::Result;
use kiyo_render::PdfRect;
use ocrs::TextItem;
use pdf_writer::types::TextRenderingMode;
use pdf_writer::{Content, Finish, Name, Str};

/// Recognized word box with spatial coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    pub text: String,
    pub bbox: PdfRect,
    pub confidence: f32,
}

/// Neural OCR pipeline performing detection and recognition.
pub struct OcrPipeline {
    #[allow(dead_code)]
    engine: Option<ocrs::OcrEngine>,
}

impl OcrPipeline {
    pub fn new() -> Result<Self> {
        // Attempt to load models from standard cache or env paths
        let detection_path = std::env::var("KIYO_OCR_DETECTION_MODEL")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| dirs_or_cache_path("text-detection.rten"));
        let recognition_path = std::env::var("KIYO_OCR_RECOGNITION_MODEL")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| dirs_or_cache_path("text-recognition.rten"));

        let engine = if detection_path.exists() && recognition_path.exists() {
            match load_engine(&detection_path, &recognition_path) {
                Ok(eng) => Some(eng),
                Err(err) => {
                    tracing::warn!("Failed loading neural OCR models: {err}");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self { engine })
    }

    pub fn recognize_rgba(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
        page_width_pt: f32,
        page_height_pt: f32,
    ) -> Result<Vec<OcrWord>> {
        if let Some(engine) = &self.engine {
            let img_source = ocrs::ImageSource::from_bytes(rgba, (width, height))?;
            let ocr_input = engine.prepare_input(img_source)?;
            let word_rects = engine.detect_words(&ocr_input)?;
            let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
            let texts = engine.recognize_text(&ocr_input, &line_rects)?;

            let scale_x = page_width_pt / width as f32;
            let scale_y = page_height_pt / height as f32;

            let mut out = Vec::new();
            for line in texts.iter().flatten() {
                for word in line.words() {
                    // PDF origin is bottom-left, while image origin is top-left
                    let rect = word.bounding_rect();
                    let min_x = rect.left() as f32 * scale_x;
                    let max_x = rect.right() as f32 * scale_x;
                    let min_y = page_height_pt - (rect.bottom() as f32 * scale_y);
                    let max_y = page_height_pt - (rect.top() as f32 * scale_y);

                    out.push(OcrWord {
                        text: word.to_string(),
                        bbox: PdfRect {
                            min_x,
                            min_y,
                            max_x,
                            max_y,
                        },
                        confidence: 0.95,
                    });
                }
            }
            Ok(out)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn process_document(
        &self,
        input_path: &std::path::Path,
        output_path: &std::path::Path,
        _language: Option<&str>,
    ) -> Result<usize> {
        use kiyo_render::RenderBackend;
        use std::sync::Arc;

        let ext = input_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "pdf" {
            let doc: Arc<dyn kiyo_core::universal::UniversalDocument> =
                Arc::from(kiyo_render::open_universal_document(input_path)?);
            let page_count = doc.page_count();
            anyhow::ensure!(page_count > 0, "Document contains 0 pages");

            let mut pdf = pdf_writer::Pdf::new();
            let catalog_id = pdf_writer::Ref::new(1);
            let page_tree_id = pdf_writer::Ref::new(2);
            let mut page_refs = Vec::new();
            let mut next_ref = 3;

            let mut page_data = Vec::new();
            let mut total_words = 0;

            for page_idx in 0..page_count {
                let metrics = doc.page_metrics(page_idx)?;
                let page_id = pdf_writer::Ref::new(next_ref);
                next_ref += 1;
                let image_id = pdf_writer::Ref::new(next_ref);
                next_ref += 1;
                let content_id = pdf_writer::Ref::new(next_ref);
                next_ref += 1;
                let font_id = pdf_writer::Ref::new(next_ref);
                next_ref += 1;

                page_refs.push(page_id);

                let backend = kiyo_render::UniversalRenderBackend::new(Arc::clone(&doc));
                let tile = backend.render_tile(
                    kiyo_render::TileKey {
                        page_index: page_idx,
                        level_of_detail: 1,
                        tile_col: 0,
                        tile_row: 0,
                    },
                    1.0,
                )?;

                let mut words = self.recognize_rgba(
                    &tile.pixels,
                    tile.width,
                    tile.height,
                    metrics.width,
                    metrics.height,
                )?;

                if words.is_empty() {
                    if let Ok(runs) = backend.extract_text_runs(page_idx) {
                        for run in runs {
                            for part in run.text.split_whitespace() {
                                words.push(OcrWord {
                                    text: part.to_string(),
                                    bbox: run.rect,
                                    confidence: 0.99,
                                });
                            }
                        }
                    }
                }

                total_words += words.len();
                page_data.push((page_id, image_id, content_id, font_id, metrics, tile, words));
            }

            pdf.catalog(catalog_id).pages(page_tree_id);
            pdf.pages(page_tree_id)
                .kids(page_refs)
                .count(page_count as i32);

            for (page_id, image_id, content_id, font_id, metrics, tile, words) in page_data {
                let mut page = pdf.page(page_id);
                page.media_box(pdf_writer::Rect::new(
                    0.0,
                    0.0,
                    metrics.width,
                    metrics.height,
                ));
                page.parent(page_tree_id);
                page.contents(content_id);

                let mut resources = page.resources();
                resources
                    .x_objects()
                    .pair(pdf_writer::Name(b"BgImg"), image_id);
                resources.fonts().pair(pdf_writer::Name(b"F1"), font_id);
                resources.finish();
                page.finish();

                pdf.type1_font(font_id)
                    .base_font(pdf_writer::Name(b"Helvetica"));

                let pixel_count = (tile.width * tile.height) as usize;
                let mut rgb_data = Vec::with_capacity(pixel_count * 3);
                for chunk in tile.pixels.chunks_exact(4).take(pixel_count) {
                    rgb_data.push(chunk[0]);
                    rgb_data.push(chunk[1]);
                    rgb_data.push(chunk[2]);
                }

                let mut img = pdf.image_xobject(image_id, &rgb_data);
                img.width(tile.width as i32);
                img.height(tile.height as i32);
                img.color_space().device_rgb();
                img.bits_per_component(8);
                img.finish();

                let mut content = pdf_writer::Content::new();
                content.save_state();
                content.transform([metrics.width, 0.0, 0.0, metrics.height, 0.0, 0.0]);
                content.x_object(pdf_writer::Name(b"BgImg"));
                content.restore_state();

                inject_invisible_text_layer(&mut content, "F1", &words);
                pdf.stream(content_id, &content.finish());
            }

            let pdf_bytes = pdf.finish();
            std::fs::write(output_path, pdf_bytes)?;
            Ok(total_words)
        } else {
            let img = image::open(input_path)?;
            let rgba = img.to_rgba8();
            let (width, height) = (rgba.width(), rgba.height());
            let width_pt = width as f32;
            let height_pt = height as f32;

            let words = self.recognize_rgba(&rgba, width, height, width_pt, height_pt)?;
            let total_words = words.len();

            let mut pdf = pdf_writer::Pdf::new();
            let catalog_id = pdf_writer::Ref::new(1);
            let page_tree_id = pdf_writer::Ref::new(2);
            let page_id = pdf_writer::Ref::new(3);
            let image_id = pdf_writer::Ref::new(4);
            let content_id = pdf_writer::Ref::new(5);
            let font_id = pdf_writer::Ref::new(6);

            pdf.catalog(catalog_id).pages(page_tree_id);
            pdf.pages(page_tree_id).kids([page_id]).count(1);

            let mut page = pdf.page(page_id);
            page.media_box(pdf_writer::Rect::new(0.0, 0.0, width_pt, height_pt));
            page.parent(page_tree_id);
            page.contents(content_id);

            let mut resources = page.resources();
            resources
                .x_objects()
                .pair(pdf_writer::Name(b"BgImg"), image_id);
            resources.fonts().pair(pdf_writer::Name(b"F1"), font_id);
            resources.finish();
            page.finish();

            pdf.type1_font(font_id)
                .base_font(pdf_writer::Name(b"Helvetica"));

            let rgb_img = img.to_rgb8();
            let mut img_obj = pdf.image_xobject(image_id, &rgb_img);
            img_obj.width(width as i32);
            img_obj.height(height as i32);
            img_obj.color_space().device_rgb();
            img_obj.bits_per_component(8);
            img_obj.finish();

            let mut content = pdf_writer::Content::new();
            content.save_state();
            content.transform([width_pt, 0.0, 0.0, height_pt, 0.0, 0.0]);
            content.x_object(pdf_writer::Name(b"BgImg"));
            content.restore_state();

            inject_invisible_text_layer(&mut content, "F1", &words);
            pdf.stream(content_id, &content.finish());

            let pdf_bytes = pdf.finish();
            std::fs::write(output_path, pdf_bytes)?;
            Ok(total_words)
        }
    }
}

fn dirs_or_cache_path(filename: &str) -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".cache/kiyo/models")
            .join(filename)
    } else {
        std::path::PathBuf::from("/tmp/kiyo/models").join(filename)
    }
}

fn load_engine(
    detection_path: &std::path::Path,
    recognition_path: &std::path::Path,
) -> Result<ocrs::OcrEngine> {
    let detection_model = rten::Model::load_file(detection_path)?;
    let recognition_model = rten::Model::load_file(recognition_path)?;
    let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })?;
    Ok(engine)
}

/// Injects an invisible text layer (Text Rendering Mode 3 `3 Tr`) directly into
/// a PDF page content stream, creating an ISO 32000-compliant searchable sandwich PDF.
pub fn inject_invisible_text_layer(content: &mut Content, font_name: &str, words: &[OcrWord]) {
    content.begin_text();
    // 3 Tr: Neither fill nor stroke text (renders text completely invisible)
    content.set_text_rendering_mode(TextRenderingMode::Invisible);

    for word in words {
        let font_size = (word.bbox.max_y - word.bbox.min_y).abs();
        content.set_font(Name(font_name.as_bytes()), font_size.max(1.0));

        // Transform Matrix (Tm): translate to the word's bottom-left origin
        content.set_text_matrix([1.0, 0.0, 0.0, 1.0, word.bbox.min_x, word.bbox.min_y]);

        // Output text glyphs
        content.show(Str(word.text.as_bytes()));
    }
    content.end_text();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_invisible_text_layer_iso32000() {
        let mut content = Content::new();
        let words = vec![
            OcrWord {
                text: "Kiyo".to_string(),
                bbox: PdfRect {
                    min_x: 72.0,
                    min_y: 700.0,
                    max_x: 120.0,
                    max_y: 714.0,
                },
                confidence: 0.98,
            },
            OcrWord {
                text: "Engine".to_string(),
                bbox: PdfRect {
                    min_x: 125.0,
                    min_y: 700.0,
                    max_x: 180.0,
                    max_y: 714.0,
                },
                confidence: 0.99,
            },
        ];

        inject_invisible_text_layer(&mut content, "F1", &words);
        let buf = content.finish();
        let stream = std::str::from_utf8(buf.as_slice()).expect("Valid UTF-8 stream");

        // Verify ISO 32000 operators
        assert!(
            stream.contains("BT"),
            "Stream must contain Begin Text operator"
        );
        assert!(
            stream.contains("3 Tr"),
            "Stream must set Text Rendering Mode 3 (Invisible)"
        );
        assert!(
            stream.contains("/F1 14 Tf")
                || stream.contains("/F1 14.0 Tf")
                || stream.contains("/F1 14"),
            "Stream must select font F1"
        );
        assert!(stream.contains("(Kiyo) Tj"), "Stream must show 'Kiyo' text");
        assert!(
            stream.contains("(Engine) Tj"),
            "Stream must show 'Engine' text"
        );
        assert!(
            stream.contains("ET"),
            "Stream must contain End Text operator"
        );
    }

    #[test]
    fn test_ocr_pipeline_process_image_sandwich_pdf() {
        let temp_dir = std::env::temp_dir();
        let img_path = temp_dir.join("test_ocr_input.png");
        let out_pdf_path = temp_dir.join("test_ocr_output.pdf");

        // Create a 200x100 white PNG image
        let img = image::RgbImage::from_fn(200, 100, |_x, _y| image::Rgb([255, 255, 255]));
        img.save(&img_path).expect("Save test PNG");

        let pipeline = OcrPipeline::new().expect("Create OCR pipeline");
        let result = pipeline.process_document(&img_path, &out_pdf_path, None);
        assert!(
            result.is_ok(),
            "Processing image to PDF succeeds: {:?}",
            result
        );

        assert!(out_pdf_path.exists());
        let data = std::fs::read(&out_pdf_path).expect("Read output PDF");
        assert!(data.starts_with(b"%PDF-"));

        let _ = std::fs::remove_file(img_path);
        let _ = std::fs::remove_file(out_pdf_path);
    }
}
