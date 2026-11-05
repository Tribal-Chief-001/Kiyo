//! `kiyo-render`: Viewport management, tile cache, and rendering backend abstraction.

use anyhow::Result;

/// Normalized PDF coordinate point (Points: 1/72 inch, origin at bottom-left).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PdfPoint {
    pub x: f32,
    pub y: f32,
}

/// Normalized PDF bounding box rectangle.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PdfRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl PdfRect {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }

    pub fn width(&self) -> f32 {
        (self.max_x - self.min_x).abs()
    }

    pub fn height(&self) -> f32 {
        (self.max_y - self.min_y).abs()
    }

    pub fn contains_point(&self, p: PdfPoint) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }

    pub fn intersects(&self, other: &PdfRect) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }
}

/// Tile identification key for caching and frustum culling.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub page_index: u32,
    pub level_of_detail: u8,
    pub tile_col: u32,
    pub tile_row: u32,
}

/// Raw RGBA8 raster tile buffer.
pub struct TileBuffer {
    pub key: TileKey,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: Vec<u8>,
}

/// Text glyph layout extracted from document streams.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub rect: PdfRect,
    pub font_name: String,
    pub font_size: f32,
}

/// Search match occurrence.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub page_index: u32,
    pub bounding_boxes: Vec<PdfRect>,
    pub matched_text: String,
    pub context_snippet: String,
}

/// Abstract rendering backend interface.
pub trait RenderBackend: Send + Sync {
    /// Returns total page count.
    fn page_count(&self) -> u32;

    /// Retrieves dimensional page metrics (in PDF points).
    fn page_dimensions(&self, page_index: u32) -> Result<(f32, f32)>;

    /// Synchronously or asynchronously renders a single tile into memory.
    fn render_tile(&self, key: TileKey, zoom_factor: f32) -> Result<TileBuffer>;

    /// Extracts structural text runs for text selection and clipboard operations.
    fn extract_text_runs(&self, page_index: u32) -> Result<Vec<TextRun>>;

    /// Executes SIMD-accelerated text search across pages.
    fn search(&self, query: &str, case_sensitive: bool) -> Result<Vec<SearchResult>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_pdf_rect_containment_and_intersection() {
        let r1 = PdfRect::new(10.0, 10.0, 50.0, 50.0);
        let p_inside = PdfPoint { x: 25.0, y: 25.0 };
        let p_outside = PdfPoint { x: 5.0, y: 25.0 };

        assert!(r1.contains_point(p_inside));
        assert!(!r1.contains_point(p_outside));
        assert_eq!(r1.width(), 40.0);
        assert_eq!(r1.height(), 40.0);

        let r2 = PdfRect::new(30.0, 30.0, 70.0, 70.0);
        let r3 = PdfRect::new(60.0, 60.0, 90.0, 90.0);

        assert!(r1.intersects(&r2));
        assert!(!r1.intersects(&r3));
    }

    #[test]
    fn test_tile_key_hash_and_equality() {
        let mut set = HashSet::new();
        let k1 = TileKey { page_index: 0, level_of_detail: 0, tile_col: 1, tile_row: 2 };
        let k2 = TileKey { page_index: 0, level_of_detail: 0, tile_col: 1, tile_row: 2 };
        let k3 = TileKey { page_index: 1, level_of_detail: 0, tile_col: 1, tile_row: 2 };

        set.insert(k1);
        assert!(set.contains(&k2));
        assert!(!set.contains(&k3));
    }
}
