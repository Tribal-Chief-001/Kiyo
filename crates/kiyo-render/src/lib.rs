//! `kiyo-render`: Viewport management, tile cache, and rendering backend abstraction.

use anyhow::Result;

/// Normalized PDF coordinate point (Points: 1/72 inch).
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

/// Abstract rendering backend interface.
pub trait RenderBackend: Send + Sync {
    fn page_count(&self) -> u32;
    fn page_dimensions(&self, page_index: u32) -> Result<(f32, f32)>;
    fn render_tile(&self, key: TileKey, zoom_factor: f32) -> Result<TileBuffer>;
}
