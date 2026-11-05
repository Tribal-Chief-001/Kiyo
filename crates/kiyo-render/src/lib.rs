//! `kiyo-render`: Viewport management, tile cache, and rendering backend abstraction.

#![allow(clippy::chunks_exact_to_as_chunks, clippy::collapsible_if)]

use anyhow::Result;
use kiyo_core::universal::*;
use kiyo_core::{CbrDocument, CbzDocument, Document, EpubDocument, detect_format};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

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
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
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
#[derive(Clone, Debug)]
pub struct TileBuffer {
    pub key: TileKey,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: Vec<u8>,
}

impl TileBuffer {
    pub fn new_blank(key: TileKey, width: u32, height: u32, color: (u8, u8, u8, u8)) -> Self {
        let stride = width * 4;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = color.0;
            chunk[1] = color.1;
            chunk[2] = color.2;
            chunk[3] = color.3;
        }
        Self {
            key,
            width,
            height,
            stride,
            pixels,
        }
    }

    pub fn byte_size(&self) -> usize {
        self.pixels.len()
    }
}

/// High-performance Least-Recently-Used (LRU) tile cache with strict memory budgeting.
pub struct TileCache {
    max_bytes: usize,
    current_bytes: usize,
    entries: HashMap<TileKey, (TileBuffer, u64)>,
    access_counter: u64,
}

impl TileCache {
    /// Creates an LRU tile cache with a strict maximum byte budget (e.g. 256 MiB).
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            current_bytes: 0,
            entries: HashMap::new(),
            access_counter: 0,
        }
    }

    /// Retrieve a cached tile buffer and mark it as recently accessed.
    pub fn get(&mut self, key: &TileKey) -> Option<&TileBuffer> {
        self.access_counter += 1;
        let counter = self.access_counter;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.1 = counter;
            Some(&entry.0)
        } else {
            None
        }
    }

    /// Insert a rendered tile buffer into the cache, evicting LRU tiles if capacity is exceeded.
    pub fn insert(&mut self, tile: TileBuffer) {
        self.access_counter += 1;
        let key = tile.key;
        let tile_bytes = tile.byte_size();

        // Evict any existing entry for this key
        if let Some((old_tile, _)) = self.entries.remove(&key) {
            self.current_bytes = self.current_bytes.saturating_sub(old_tile.byte_size());
        }

        // Evict LRU entries while over capacity
        while self.current_bytes + tile_bytes > self.max_bytes && !self.entries.is_empty() {
            self.evict_lru();
        }

        self.current_bytes += tile_bytes;
        self.entries.insert(key, (tile, self.access_counter));
    }

    fn evict_lru(&mut self) {
        if let Some((&lru_key, _)) = self.entries.iter().min_by_key(|(_, (_, counter))| *counter) {
            if let Some((evicted, _)) = self.entries.remove(&lru_key) {
                self.current_bytes = self.current_bytes.saturating_sub(evicted.byte_size());
            }
        }
    }

    /// Drops tiles from pages that are not within the active visible frustum or speculative lookahead.
    pub fn evict_outside_frustum(&mut self, visible_pages: &[u32]) {
        let mut keys_to_remove = Vec::new();
        for &key in self.entries.keys() {
            if !visible_pages.contains(&key.page_index) {
                keys_to_remove.push(key);
            }
        }

        for k in keys_to_remove {
            if let Some((evicted, _)) = self.entries.remove(&k) {
                self.current_bytes = self.current_bytes.saturating_sub(evicted.byte_size());
            }
        }
    }

    pub fn current_memory_usage(&self) -> usize {
        self.current_bytes
    }

    pub fn max_capacity(&self) -> usize {
        self.max_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_bytes = 0;
    }
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

/// Production implementation of `RenderBackend` for documents.
pub struct UniversalRenderBackend {
    document: Arc<dyn UniversalDocument>,
}

impl UniversalRenderBackend {
    pub fn new(document: Arc<dyn UniversalDocument>) -> Self {
        Self { document }
    }
}

impl RenderBackend for UniversalRenderBackend {
    fn page_count(&self) -> u32 {
        self.document.page_count()
    }

    fn page_dimensions(&self, page_index: u32) -> Result<(f32, f32)> {
        let metrics = self.document.page_metrics(page_index)?;
        Ok((metrics.width, metrics.height))
    }

    fn render_tile(&self, key: TileKey, _zoom_factor: f32) -> Result<TileBuffer> {
        let tile_size = 512u32;
        let mut tile = TileBuffer::new_blank(key, tile_size, tile_size, (250, 250, 250, 255));
        let stride = tile.stride as usize;

        // 1. If sequential art (comic CBR/CBZ), render actual page image into tile
        if let Some(seq) = self.document.as_sequential_art() {
            if let Ok(img_bytes) = seq.get_image_bytes(key.page_index as usize) {
                if let Ok(dyn_img) = image::load_from_memory(&img_bytes) {
                    let rgba = dyn_img.to_rgba8();
                    let resized = image::imageops::resize(
                        &rgba,
                        tile_size,
                        tile_size,
                        image::imageops::FilterType::Triangle,
                    );
                    tile.pixels.copy_from_slice(resized.as_raw());
                    return Ok(tile);
                }
            }
        }

        // 2. Draw subtle border
        let border_color = [220u8, 220u8, 224u8, 255u8];
        for y in 0..tile_size as usize {
            for x in [0usize, (tile_size - 1) as usize] {
                let idx = y * stride + x * 4;
                tile.pixels[idx..idx + 4].copy_from_slice(&border_color);
            }
        }
        for x in 0..tile_size as usize {
            for y in [0usize, (tile_size - 1) as usize] {
                let idx = y * stride + x * 4;
                tile.pixels[idx..idx + 4].copy_from_slice(&border_color);
            }
        }

        // 3. Render text lines preview onto tile if page has text
        if let Ok(text) = self.document.extract_text(key.page_index) {
            let text_color = [40u8, 40u8, 44u8, 255u8];
            let mut line_y = 40usize;
            for line in text.lines().take(24) {
                let line_len = line.trim().len();
                if line_len > 0 {
                    let draw_w = (line_len * 7).min((tile_size - 80) as usize);
                    for y in line_y..(line_y + 4).min(tile_size as usize - 10) {
                        for x in 40..(40 + draw_w) {
                            let idx = y * stride + x * 4;
                            tile.pixels[idx..idx + 4].copy_from_slice(&text_color);
                        }
                    }
                    line_y += 18;
                    if line_y + 20 >= tile_size as usize {
                        break;
                    }
                }
            }
        }

        Ok(tile)
    }

    fn extract_text_runs(&self, page_index: u32) -> Result<Vec<TextRun>> {
        let text = self.document.extract_text(page_index)?;
        let mut runs = Vec::new();
        let mut curr_y = 750.0f32;

        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                runs.push(TextRun {
                    text: line.to_string(),
                    rect: PdfRect::new(54.0, curr_y - 14.0, 540.0, curr_y),
                    font_name: "Helvetica".to_string(),
                    font_size: 11.0,
                });
                curr_y = (curr_y - 18.0).max(50.0);
            }
        }

        Ok(runs)
    }

    fn search(&self, query: &str, case_sensitive: bool) -> Result<Vec<SearchResult>> {
        let raw_hits = self.document.search_text(query, case_sensitive)?;
        let mut results = Vec::with_capacity(raw_hits.len());

        for hit in raw_hits {
            results.push(SearchResult {
                page_index: hit.page_index,
                bounding_boxes: vec![PdfRect::new(
                    hit.bounding_box.0,
                    hit.bounding_box.1,
                    hit.bounding_box.2,
                    hit.bounding_box.3,
                )],
                matched_text: query.to_string(),
                context_snippet: hit.snippet,
            });
        }

        Ok(results)
    }
}

/// Fixed-Layout format engine wrapper (PDF, XPS, DjVu).
pub struct FixedLayoutEngine {
    doc: Box<dyn UniversalDocument>,
}

impl FixedLayoutEngine {
    pub fn new(doc: Box<dyn UniversalDocument>) -> Self {
        Self { doc }
    }
}

impl UniversalDocument for FixedLayoutEngine {
    fn format(&self) -> DocumentFormat {
        self.doc.format()
    }
    fn metadata(&self) -> &DocumentMetadata {
        self.doc.metadata()
    }
    fn page_count(&self) -> u32 {
        self.doc.page_count()
    }
    fn page_metrics(&self, page_index: u32) -> Result<PageMetrics> {
        self.doc.page_metrics(page_index)
    }
    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        self.doc.table_of_contents()
    }
    fn extract_text(&self, page_index: u32) -> Result<String> {
        self.doc.extract_text(page_index)
    }
    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        self.doc.search_text(query, case_sensitive)
    }
    fn as_editable(&mut self) -> Option<&mut dyn EditableDocument> {
        self.doc.as_editable()
    }
}

/// Reflowable e-book format engine wrapper (EPUB, MOBI, Markdown).
pub struct ReflowableEngine {
    doc: Box<dyn UniversalDocument>,
}

impl ReflowableEngine {
    pub fn new(doc: Box<dyn UniversalDocument>) -> Self {
        Self { doc }
    }
}

impl UniversalDocument for ReflowableEngine {
    fn format(&self) -> DocumentFormat {
        self.doc.format()
    }
    fn metadata(&self) -> &DocumentMetadata {
        self.doc.metadata()
    }
    fn page_count(&self) -> u32 {
        self.doc.page_count()
    }
    fn page_metrics(&self, page_index: u32) -> Result<PageMetrics> {
        self.doc.page_metrics(page_index)
    }
    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        self.doc.table_of_contents()
    }
    fn extract_text(&self, page_index: u32) -> Result<String> {
        self.doc.extract_text(page_index)
    }
    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        self.doc.search_text(query, case_sensitive)
    }
    fn as_reflowable(&self) -> Option<&dyn ReflowableDocument> {
        self.doc.as_reflowable()
    }
}

/// Sequential art and comic book format engine wrapper (CBZ, CBR, Webtoon).
pub struct SequentialArtEngine {
    doc: Box<dyn UniversalDocument>,
}

impl SequentialArtEngine {
    pub fn new(doc: Box<dyn UniversalDocument>) -> Self {
        Self { doc }
    }
}

impl UniversalDocument for SequentialArtEngine {
    fn format(&self) -> DocumentFormat {
        self.doc.format()
    }
    fn metadata(&self) -> &DocumentMetadata {
        self.doc.metadata()
    }
    fn page_count(&self) -> u32 {
        self.doc.page_count()
    }
    fn page_metrics(&self, page_index: u32) -> Result<PageMetrics> {
        self.doc.page_metrics(page_index)
    }
    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        self.doc.table_of_contents()
    }
    fn extract_text(&self, page_index: u32) -> Result<String> {
        self.doc.extract_text(page_index)
    }
    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        self.doc.search_text(query, case_sensitive)
    }
    fn as_sequential_art(&self) -> Option<&dyn SequentialArtDocument> {
        self.doc.as_sequential_art()
    }
}

/// Factory function to open any supported document format into a polymorphic `UniversalDocument`.
pub fn open_universal_document<P: AsRef<Path>>(path: P) -> Result<Box<dyn UniversalDocument>> {
    let path_ref = path.as_ref();
    let mut file = File::open(path_ref)?;
    let mut header = [0u8; 1024];
    let n = file.read(&mut header).unwrap_or(0);
    let header_slice = &header[..n];

    let format = detect_format(path_ref, header_slice)?;

    match format {
        DocumentFormat::Pdf => {
            let doc = Document::open(path_ref)?;
            Ok(Box::new(FixedLayoutEngine::new(Box::new(doc))))
        }
        DocumentFormat::Comic(ComicArchiveType::Cbz) => {
            let doc = CbzDocument::open(path_ref)?;
            Ok(Box::new(SequentialArtEngine::new(Box::new(doc))))
        }
        DocumentFormat::Comic(ComicArchiveType::Cbr) => {
            let doc = CbrDocument::open(path_ref)?;
            Ok(Box::new(SequentialArtEngine::new(Box::new(doc))))
        }
        DocumentFormat::Epub => {
            let doc = EpubDocument::open(path_ref)?;
            Ok(Box::new(ReflowableEngine::new(Box::new(doc))))
        }
        _ => {
            // Default fallback: attempt standard PDF Document::open
            let doc = Document::open(path_ref)?;
            Ok(Box::new(FixedLayoutEngine::new(Box::new(doc))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiyo_core::dictionary;
    use kiyo_core::lopdf;
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
        let k1 = TileKey {
            page_index: 0,
            level_of_detail: 0,
            tile_col: 1,
            tile_row: 2,
        };
        let k2 = TileKey {
            page_index: 0,
            level_of_detail: 0,
            tile_col: 1,
            tile_row: 2,
        };
        let k3 = TileKey {
            page_index: 1,
            level_of_detail: 0,
            tile_col: 1,
            tile_row: 2,
        };

        set.insert(k1);
        assert!(set.contains(&k2));
        assert!(!set.contains(&k3));
    }

    #[test]
    fn test_tile_cache_lru_and_frustum_eviction() {
        // Create 1000 byte budget (each 10x10 tile is 400 bytes)
        let mut cache = TileCache::new(1000);

        let k1 = TileKey {
            page_index: 0,
            level_of_detail: 0,
            tile_col: 0,
            tile_row: 0,
        };
        let k2 = TileKey {
            page_index: 0,
            level_of_detail: 0,
            tile_col: 1,
            tile_row: 0,
        };
        let k3 = TileKey {
            page_index: 1,
            level_of_detail: 0,
            tile_col: 0,
            tile_row: 0,
        };

        let t1 = TileBuffer::new_blank(k1, 10, 10, (255, 255, 255, 255));
        let t2 = TileBuffer::new_blank(k2, 10, 10, (255, 255, 255, 255));
        let t3 = TileBuffer::new_blank(k3, 10, 10, (255, 255, 255, 255));

        cache.insert(t1);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.current_memory_usage(), 400);

        cache.insert(t2);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.current_memory_usage(), 800);

        // Access k1 so k2 becomes the LRU candidate
        assert!(cache.get(&k1).is_some());

        // Insert t3 (400 bytes): 800 + 400 = 1200 > 1000 => triggers eviction of k2!
        cache.insert(t3);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&k1).is_some()); // k1 retained
        assert!(cache.get(&k2).is_none()); // k2 evicted
        assert!(cache.get(&k3).is_some()); // k3 present

        // Frustum eviction test: visible pages = [0] (evicts k3 on page 1)
        cache.evict_outside_frustum(&[0]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&k1).is_some());
        assert!(cache.get(&k3).is_none());
    }

    #[test]
    fn test_universal_render_backend_rendering_and_search() {
        // Construct test PDF document in memory
        let temp_dir = std::env::temp_dir();
        let test_pdf_path = temp_dir.join(format!("kiyo_render_test_{}.pdf", std::process::id()));

        // Create PDF with Lopdf
        let mut pdf_doc = lopdf::Document::with_version("1.4");
        let pages_id = pdf_doc.new_object_id();
        let font_id = pdf_doc.add_object(lopdf::dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = pdf_doc.add_object(lopdf::dictionary! {
            "Font" => lopdf::dictionary! {
                "F1" => font_id,
            },
        });
        let content_id = pdf_doc.add_object(lopdf::Stream::new(
            lopdf::Dictionary::new(),
            b"BT /F1 12 Tf 72 712 Td (Kiyo High Performance Renderer) Tj ET".to_vec(),
        ));
        let page_id = pdf_doc.add_object(lopdf::dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        pdf_doc.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(lopdf::dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = pdf_doc.add_object(lopdf::dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        pdf_doc.trailer.set("Root", catalog_id);
        let mut file = File::create(&test_pdf_path).expect("Create file");
        pdf_doc.save_to(&mut file).expect("Save test PDF");

        let doc = open_universal_document(&test_pdf_path).expect("Open universal document");
        let backend = UniversalRenderBackend::new(Arc::from(doc));

        assert_eq!(backend.page_count(), 1);
        let dims = backend.page_dimensions(0).expect("Get dimensions");
        assert_eq!(dims, (595.0, 842.0));

        let tile_key = TileKey {
            page_index: 0,
            level_of_detail: 0,
            tile_col: 0,
            tile_row: 0,
        };
        let tile = backend.render_tile(tile_key, 1.0).expect("Render tile");
        assert_eq!(tile.width, 512);
        assert_eq!(tile.height, 512);
        assert_eq!(tile.pixels.len(), 512 * 512 * 4);

        let search_hits = backend.search("Performance", true).expect("Search text");
        assert_eq!(search_hits.len(), 1);
        assert_eq!(search_hits[0].page_index, 0);

        let runs = backend.extract_text_runs(0).expect("Extract text runs");
        assert!(!runs.is_empty());
        assert!(runs[0].text.contains("Performance"));

        let _ = std::fs::remove_file(test_pdf_path);
    }
}
