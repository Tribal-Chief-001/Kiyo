//! `kiyo-ocr`: Embedded offline neural OCR inference engine (ocrs + rten).

use anyhow::Result;
use kiyo_render::PdfRect;
use pdf_writer::types::TextRenderingMode;
use pdf_writer::{Content, Name, Str};

/// Recognized word box with spatial coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    pub text: String,
    pub bbox: PdfRect,
    pub confidence: f32,
}

/// Neural OCR pipeline performing detection and recognition.
pub struct OcrPipeline {
    // Engine state placeholder for ocrs + rten inference models
}

impl OcrPipeline {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
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
}
