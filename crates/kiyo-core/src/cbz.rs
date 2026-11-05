//! CBZ Comic Book Archive parser and sequential art document engine.

use anyhow::{Result, anyhow};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use crate::universal::*;
use crate::zip_container::{ZipArchive, natural_cmp};

/// Comic Book Archive (.cbz) document model backed by zero-copy memory mapping.
pub struct CbzDocument {
    pub file_size: u64,
    pub mmap: Arc<Mmap>,
    pub metadata: DocumentMetadata,
    pub image_entries: Vec<String>,
}

impl CbzDocument {
    /// Opens a CBZ archive from disk, naturally sorting all comic image frames.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())?;
        let file_size = file.metadata()?.len();
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };

        let archive = ZipArchive::parse(&mmap)?;

        // Filter and naturally sort image files
        let mut image_entries: Vec<String> = archive
            .entries()
            .iter()
            .filter(|e| {
                let name = e.name.to_lowercase();
                !name.starts_with("__macosx")
                    && !name.starts_with('.')
                    && (name.ends_with(".jpg")
                        || name.ends_with(".jpeg")
                        || name.ends_with(".png")
                        || name.ends_with(".webp")
                        || name.ends_with(".bmp")
                        || name.ends_with(".jxl")
                        || name.ends_with(".avif"))
            })
            .map(|e| e.name.clone())
            .collect();

        image_entries.sort_by(|a, b| natural_cmp(a, b));

        // Attempt reading ComicInfo.xml metadata if present
        let mut title = None;
        let mut author = None;
        let mut direction = ReadingDirection::LeftToRight;

        if let Ok(comic_info_bytes) = archive.read_file("ComicInfo.xml") {
            let info_str = String::from_utf8_lossy(&comic_info_bytes);
            if let Some(t) = extract_xml_tag(&info_str, "Title") {
                title = Some(t);
            }
            if let Some(w) = extract_xml_tag(&info_str, "Writer") {
                author = Some(w);
            }
            if let Some(manga) = extract_xml_tag(&info_str, "Manga") {
                if manga.eq_ignore_ascii_case("yesandrighttoleft")
                    || manga.eq_ignore_ascii_case("yes")
                {
                    direction = ReadingDirection::RightToLeft;
                }
            }
        }

        let metadata = DocumentMetadata {
            title: title.or_else(|| {
                path.as_ref()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            }),
            author,
            page_count: image_entries.len() as u32,
            format: Some(DocumentFormat::Comic(ComicArchiveType::Cbz)),
            reading_direction: Some(direction),
            ..Default::default()
        };

        Ok(Self {
            file_size,
            mmap: Arc::new(mmap),
            metadata,
            image_entries,
        })
    }

    /// Read raw image bytes for a frame by page index.
    pub fn read_page_image(&self, index: usize) -> Result<Vec<u8>> {
        if index >= self.image_entries.len() {
            return Err(anyhow!("Comic page {} out of range", index));
        }
        let entry_name = &self.image_entries[index];
        let archive = ZipArchive::parse(&self.mmap)?;
        archive.read_file(entry_name)
    }
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    let start = xml.find(&open_tag)? + open_tag.len();
    let end = xml[start..].find(&close_tag)? + start;
    Some(xml[start..end].trim().to_string())
}

impl UniversalDocument for CbzDocument {
    fn format(&self) -> DocumentFormat {
        DocumentFormat::Comic(ComicArchiveType::Cbz)
    }

    fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    fn page_count(&self) -> u32 {
        self.image_entries.len() as u32
    }

    fn page_metrics(&self, _page_index: u32) -> Result<PageMetrics> {
        // Comic pages default to standard portrait page geometry (e.g. 800 x 1200 pt)
        Ok(PageMetrics::new(800.0, 1200.0))
    }

    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        let mut nodes = Vec::new();
        for (i, name) in self.image_entries.iter().enumerate() {
            nodes.push(TocNode {
                title: format!("Page {} ({})", i + 1, name),
                target_page: i as u32,
                target_x: None,
                target_y: None,
                children: Vec::new(),
            });
        }
        Ok(nodes)
    }

    fn extract_text(&self, page_index: u32) -> Result<String> {
        if let Some(name) = self.image_entries.get(page_index as usize) {
            Ok(format!("[Comic Page {}: {}]", page_index + 1, name))
        } else {
            Ok(String::new())
        }
    }

    fn search_text(&self, query: &str, _case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        let mut results = Vec::new();
        for (i, name) in self.image_entries.iter().enumerate() {
            if name.to_lowercase().contains(&query.to_lowercase()) {
                results.push(TextSearchResult {
                    page_index: i as u32,
                    bounding_box: (0.0, 0.0, 800.0, 40.0),
                    snippet: name.clone(),
                });
            }
        }
        Ok(results)
    }

    fn as_sequential_art(&self) -> Option<&dyn SequentialArtDocument> {
        Some(self)
    }
}

impl SequentialArtDocument for CbzDocument {
    fn reading_direction(&self) -> ReadingDirection {
        self.metadata
            .reading_direction
            .unwrap_or(ReadingDirection::LeftToRight)
    }

    fn image_count(&self) -> usize {
        self.image_entries.len()
    }

    fn image_dimensions(&self, _index: usize) -> Result<(u32, u32)> {
        // Standard comic aspect ratio placeholder
        Ok((1600, 2400))
    }

    fn is_dual_page_spread(&self, _index: usize) -> bool {
        false
    }

    fn detect_panels(&self, _index: usize) -> Result<Vec<ComicPanel>> {
        // Full single page panel
        Ok(vec![ComicPanel {
            panel_index: 0,
            bounds: (0.0, 0.0, 800.0, 1200.0),
            reading_order: 0,
        }])
    }

    fn get_image_bytes(&self, index: usize) -> Result<Vec<u8>> {
        self.read_page_image(index)
    }
}
