//! `kiyo-ocr`: Embedded offline neural OCR inference engine (ocrs + rten).

use anyhow::Result;
use kiyo_render::PdfRect;

/// Recognized word box with spatial coordinates.
#[derive(Clone, Debug)]
pub struct OcrWord {
    pub text: String,
    pub bbox: PdfRect,
    pub confidence: f32,
}

/// Neural OCR pipeline performing detection and recognition.
pub struct OcrPipeline {
    // Engine state
}

impl OcrPipeline {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}
