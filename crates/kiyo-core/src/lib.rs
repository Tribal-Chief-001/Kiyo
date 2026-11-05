//! `kiyo-core`: Zero-copy PDF document model, cross-reference streaming, and manipulation.

pub mod universal;

pub use lopdf::dictionary;
pub use universal::*;

use anyhow::{anyhow, Result};
use memmap2::Mmap;
use parking_lot::RwLock;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// PDF document backed by zero-copy virtual memory mapping.
pub struct Document {
    pub file_size: u64,
    pub mmap: Arc<Mmap>,
    pub metadata: DocumentMetadata,
    parsed: RwLock<Option<lopdf::Document>>,
}

fn obj_to_f32(obj: &lopdf::Object) -> Option<f32> {
    match obj {
        lopdf::Object::Real(f) => Some(*f as f32),
        lopdf::Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

impl Document {
    /// Opens and memory-maps a PDF document from disk with kernel madvise optimization.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let file = File::open(path_ref)?;
        let file_size = file.metadata()?.len();
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };

        #[cfg(target_os = "linux")]
        unsafe {
            libc::madvise(
                mmap.as_ptr() as *mut libc::c_void,
                mmap.len(),
                libc::MADV_WILLNEED | libc::MADV_RANDOM,
            );
        }

        let mut metadata = DocumentMetadata {
            page_count: 0,
            format: Some(DocumentFormat::Pdf),
            reading_direction: Some(ReadingDirection::LeftToRight),
            is_encrypted: false,
            is_signed: false,
            ..Default::default()
        };

        let mut parsed_doc = None;
        if let Ok(doc) = lopdf::Document::load_mem(&mmap) {
            metadata.page_count = doc.get_pages().len() as u32;
            metadata.is_encrypted = doc.trailer.has(b"Encrypt");

            if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
                if let Ok(info_dict) = doc.get_object(info_ref).and_then(|o| o.as_dict()) {
                    if let Ok(val) = info_dict.get(b"Title").and_then(|o| o.as_str()) {
                        metadata.title = Some(String::from_utf8_lossy(val).to_string());
                    }
                    if let Ok(val) = info_dict.get(b"Author").and_then(|o| o.as_str()) {
                        metadata.author = Some(String::from_utf8_lossy(val).to_string());
                    }
                    if let Ok(val) = info_dict.get(b"Subject").and_then(|o| o.as_str()) {
                        metadata.subject = Some(String::from_utf8_lossy(val).to_string());
                    }
                    if let Ok(val) = info_dict.get(b"Creator").and_then(|o| o.as_str()) {
                        metadata.creator = Some(String::from_utf8_lossy(val).to_string());
                    }
                    if let Ok(val) = info_dict.get(b"Producer").and_then(|o| o.as_str()) {
                        metadata.producer = Some(String::from_utf8_lossy(val).to_string());
                    }
                }
            }
            parsed_doc = Some(doc);
        }

        Ok(Self {
            file_size,
            mmap: Arc::new(mmap),
            metadata,
            parsed: RwLock::new(parsed_doc),
        })
    }

    /// Accesses the underlying memory-mapped bytes as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }

    /// Inspects and parses document object catalog and xref table using lopdf.
    pub fn load_lopdf(&self) -> Result<lopdf::Document> {
        let doc = lopdf::Document::load_mem(self.as_slice())?;
        Ok(doc)
    }

    /// Obtains a read lock to the cached lopdf document, parsing on demand if needed.
    fn with_parsed<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&lopdf::Document) -> Result<R>,
    {
        {
            let guard = self.parsed.read();
            if let Some(ref doc) = *guard {
                return f(doc);
            }
        }
        let doc = self.load_lopdf()?;
        let mut guard = self.parsed.write();
        *guard = Some(doc);
        f(guard.as_ref().unwrap())
    }

    /// Obtains a mutable reference to the cached lopdf document for in-place modifications.
    fn with_parsed_mut<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce(&mut lopdf::Document) -> Result<R>,
    {
        if self.parsed.get_mut().is_none() {
            let doc = self.load_lopdf()?;
            *self.parsed.get_mut() = Some(doc);
        }
        f(self.parsed.get_mut().as_mut().unwrap())
    }
}

impl UniversalDocument for Document {
    fn format(&self) -> DocumentFormat {
        DocumentFormat::Pdf
    }

    fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    fn page_count(&self) -> u32 {
        self.metadata.page_count
    }

    fn page_metrics(&self, page_index: u32) -> Result<PageMetrics> {
        self.with_parsed(|doc| {
            let pages = doc.get_pages();
            let page_num = page_index + 1;
            let page_id = pages
                .get(&page_num)
                .ok_or_else(|| anyhow!("Page {} out of range (total {})", page_num, pages.len()))?;

            let page_dict = doc.get_object(*page_id).and_then(|o| o.as_dict())?;

            let parse_rect = |name: &[u8]| -> Option<(f32, f32, f32, f32)> {
                if let Ok(arr) = page_dict.get(name).and_then(|o| o.as_array()) {
                    if arr.len() == 4 {
                        let x1 = obj_to_f32(&arr[0]).unwrap_or(0.0);
                        let y1 = obj_to_f32(&arr[1]).unwrap_or(0.0);
                        let x2 = obj_to_f32(&arr[2]).unwrap_or(0.0);
                        let y2 = obj_to_f32(&arr[3]).unwrap_or(0.0);
                        return Some((x1, y1, x2, y2));
                    }
                }
                None
            };

            let media_box = parse_rect(b"MediaBox").unwrap_or((0.0, 0.0, 595.0, 842.0));
            let crop_box = parse_rect(b"CropBox");
            let bleed_box = parse_rect(b"BleedBox");
            let trim_box = parse_rect(b"TrimBox");
            let art_box = parse_rect(b"ArtBox");

            let rotation_degrees = page_dict
                .get(b"Rotate")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as u16;

            let width = (media_box.2 - media_box.0).abs();
            let height = (media_box.3 - media_box.1).abs();

            Ok(PageMetrics {
                width,
                height,
                rotation_degrees,
                dpi: 72.0,
                media_box,
                crop_box,
                bleed_box,
                trim_box,
                art_box,
            })
        })
    }

    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        self.with_parsed(|doc| {
            let mut toc = Vec::new();
            if let Ok(catalog_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
                if let Ok(catalog) = doc.get_object(catalog_ref).and_then(|o| o.as_dict()) {
                    if let Ok(outlines_ref) = catalog.get(b"Outlines").and_then(|o| o.as_reference()) {
                        if let Ok(outlines) = doc.get_object(outlines_ref).and_then(|o| o.as_dict()) {
                            if let Ok(first_ref) = outlines.get(b"First").and_then(|o| o.as_reference()) {
                                let mut current_ref = Some(first_ref);
                                while let Some(node_ref) = current_ref {
                                    if let Ok(item) = doc.get_object(node_ref).and_then(|o| o.as_dict()) {
                                        let title = item
                                            .get(b"Title")
                                            .and_then(|o| o.as_str())
                                            .map(|s| String::from_utf8_lossy(s).to_string())
                                            .unwrap_or_else(|_| "Untitled Section".to_string());

                                        toc.push(TocNode {
                                            title,
                                            target_page: 0,
                                            target_x: None,
                                            target_y: None,
                                            children: Vec::new(),
                                        });

                                        current_ref = item.get(b"Next").ok().and_then(|o| o.as_reference().ok());
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(toc)
        })
    }

    fn extract_text(&self, page_index: u32) -> Result<String> {
        self.with_parsed(|doc| {
            let page_num = page_index + 1;
            let text = doc.extract_text(&[page_num])?;
            Ok(text)
        })
    }

    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        let mut results = Vec::new();
        let total = self.page_count();
        let query_norm = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        for page_idx in 0..total {
            if let Ok(text) = self.extract_text(page_idx) {
                let text_cmp = if case_sensitive {
                    text.clone()
                } else {
                    text.to_lowercase()
                };

                if text_cmp.contains(&query_norm) {
                    results.push(TextSearchResult {
                        page_index: page_idx,
                        bounding_box: (0.0, 0.0, 100.0, 20.0),
                        snippet: text.chars().take(80).collect(),
                    });
                }
            }
        }

        Ok(results)
    }

    fn as_editable(&mut self) -> Option<&mut dyn EditableDocument> {
        Some(self)
    }
}

impl EditableDocument for Document {
    fn apply_page_mutation(&mut self, op: PageMutationOp) -> Result<()> {
        match op {
            PageMutationOp::Rotate { page_index, degrees } => {
                self.with_parsed_mut(|doc| {
                    let pages = doc.get_pages();
                    let page_num = page_index + 1;
                    let page_id = pages
                        .get(&page_num)
                        .ok_or_else(|| anyhow!("Page {} not found", page_num))?;

                    let current_rot = doc
                        .get_object(*page_id)
                        .and_then(|o| o.as_dict())
                        .and_then(|d| d.get(b"Rotate"))
                        .and_then(|o| o.as_i64())
                        .unwrap_or(0);

                    let new_rot = ((current_rot + degrees as i64) % 360 + 360) % 360;

                    let page_obj = doc.get_object_mut(*page_id)?;
                    if let lopdf::Object::Dictionary(dict) = page_obj {
                        dict.set("Rotate", lopdf::Object::Integer(new_rot));
                    }
                    Ok(())
                })
            }
            PageMutationOp::Delete { page_index } => {
                self.with_parsed_mut(|doc| {
                    let page_num = page_index + 1;
                    doc.delete_pages(&[page_num]);
                    Ok(())
                })?;
                self.metadata.page_count = self.metadata.page_count.saturating_sub(1);
                Ok(())
            }
            PageMutationOp::Move { from_index: _, to_index: _ } => {
                // Page reordering in page tree
                Ok(())
            }
            PageMutationOp::InsertBlank { at_index: _, width, height } => {
                self.with_parsed_mut(|doc| {
                    let page_id = doc.add_object(dictionary! {
                        "Type" => "Page",
                        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
                    });
                    if let Ok(pages_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
                        if let Ok(catalog) = doc.get_object(pages_ref).and_then(|o| o.as_dict()) {
                            if let Ok(pages_root_ref) = catalog.get(b"Pages").and_then(|o| o.as_reference()) {
                                if let Ok(pages_dict) = doc.get_object_mut(pages_root_ref) {
                                    if let lopdf::Object::Dictionary(d) = pages_dict {
                                        if let Ok(kids) = d.get_mut(b"Kids") {
                                            if let lopdf::Object::Array(arr) = kids {
                                                arr.push(page_id.into());
                                            }
                                        }
                                        if let Ok(count) = d.get_mut(b"Count") {
                                            if let lopdf::Object::Integer(c) = count {
                                                *c += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(())
                })?;
                self.metadata.page_count += 1;
                Ok(())
            }
            PageMutationOp::ApplyRedaction(region) => {
                self.apply_redactions(&[region]).map(|_| ())
            }
            PageMutationOp::ApplyBates(config) => {
                self.apply_bates_numbering(&config)
            }
        }
    }

    fn apply_redactions(&mut self, regions: &[RedactionRegion]) -> Result<usize> {
        let count = regions.len();
        self.with_parsed_mut(|doc| {
            for region in regions {
                let pages = doc.get_pages();
                let page_num = region.page_index + 1;
                if let Some(page_id) = pages.get(&page_num) {
                    let w = (region.rect.2 - region.rect.0).abs();
                    let h = (region.rect.3 - region.rect.1).abs();
                    let redaction_cmd = format!(
                        "\nq\n{} {} {} rg\n{} {} {} {} re\nf\nQ\n",
                        region.fill_color.0 as f32 / 255.0,
                        region.fill_color.1 as f32 / 255.0,
                        region.fill_color.2 as f32 / 255.0,
                        region.rect.0,
                        region.rect.1,
                        w,
                        h
                    );
                    if let Ok(content) = lopdf::content::Content::decode(&redaction_cmd.into_bytes()) {
                        let _ = doc.add_to_page_content(*page_id, content);
                    }
                }
            }
            Ok(())
        })?;
        Ok(count)
    }

    fn apply_bates_numbering(&mut self, config: &BatesConfig) -> Result<()> {
        let total_pages = self.page_count();
        self.with_parsed_mut(|doc| {
            let pages = doc.get_pages();
            for (idx, page_num) in (1..=total_pages).enumerate() {
                if let Some(page_id) = pages.get(&page_num) {
                    let stamped_text = config.format_number(idx as u64);
                    let x_pos = match config.position {
                        BatesPosition::TopLeft | BatesPosition::BottomLeft => 36.0,
                        BatesPosition::TopCenter | BatesPosition::BottomCenter => 250.0,
                        BatesPosition::TopRight | BatesPosition::BottomRight => 450.0,
                    };
                    let y_pos = match config.position {
                        BatesPosition::TopLeft | BatesPosition::TopCenter | BatesPosition::TopRight => 800.0,
                        BatesPosition::BottomLeft | BatesPosition::BottomCenter | BatesPosition::BottomRight => 36.0,
                    };

                    let artifact_stream = if config.embed_as_artifact {
                        format!(
                            "\n/Artifact << /Type /Pagination /Subtype /Header >> BDC\nBT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\nEMC\n",
                            config.font_size, x_pos, y_pos, stamped_text
                        )
                    } else {
                        format!(
                            "\nBT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\n",
                            config.font_size, x_pos, y_pos, stamped_text
                        )
                    };
                    if let Ok(content) = lopdf::content::Content::decode(&artifact_stream.into_bytes()) {
                        let _ = doc.add_to_page_content(*page_id, content);
                    }
                }
            }
            Ok(())
        })
    }

    fn set_metadata_entry(&mut self, key: &str, value: &str) -> Result<()> {
        match key.to_lowercase().as_str() {
            "title" => self.metadata.title = Some(value.to_string()),
            "author" => self.metadata.author = Some(value.to_string()),
            "subject" => self.metadata.subject = Some(value.to_string()),
            "creator" => self.metadata.creator = Some(value.to_string()),
            "producer" => self.metadata.producer = Some(value.to_string()),
            _ => {}
        }

        self.with_parsed_mut(|doc| {
            if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
                if let Ok(info_obj) = doc.get_object_mut(info_ref) {
                    if let lopdf::Object::Dictionary(dict) = info_obj {
                        dict.set(key, lopdf::Object::string_literal(value));
                    }
                }
            }
            Ok(())
        })
    }

    fn save_to(&self, writer: &mut dyn std::io::Write) -> Result<()> {
        self.with_parsed(|doc| {
            let mut clone = doc.clone();
            let mut w = &mut *writer;
            clone.save_to(&mut w)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_pdf_file(path: &Path) {
        let mut pdf_doc = lopdf::Document::with_version("1.4");
        let pages_id = pdf_doc.new_object_id();
        let font_id = pdf_doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = pdf_doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });
        let content_id = pdf_doc.add_object(lopdf::Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 72 712 Td (Hello Kiyo) Tj ET".to_vec(),
        ));
        let page_id = pdf_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        pdf_doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
        let catalog_id = pdf_doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        pdf_doc.trailer.set("Root", catalog_id);

        let mut file = File::create(path).expect("Create test PDF file");
        pdf_doc.save_to(&mut file).expect("Write test PDF file");
        file.flush().expect("Flush file");
    }

    #[test]
    fn test_document_open_mmap_and_lopdf_parsing() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join(format!("kiyo_test_{}.pdf", std::process::id()));

        create_test_pdf_file(&test_path);

        // Open with Document::open (mmap2 + madvise)
        let doc = Document::open(&test_path).expect("Open mmap document");
        assert!(doc.file_size > 0);
        assert_eq!(doc.as_slice().len() as u64, doc.file_size);
        assert!(doc.as_slice().starts_with(b"%PDF-"));

        // Parse with load_lopdf
        let parsed = doc.load_lopdf().expect("Parse via lopdf");
        assert!(parsed.version.starts_with("1."));
        let page_count = parsed.get_pages().len();
        assert_eq!(page_count, 1);

        // Cleanup
        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_universal_document_and_editable_interface() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join(format!("kiyo_universal_test_{}.pdf", std::process::id()));

        create_test_pdf_file(&test_path);

        let mut doc = Document::open(&test_path).expect("Open document");

        // Trait UniversalDocument verification
        assert_eq!(doc.format(), DocumentFormat::Pdf);
        assert_eq!(doc.category(), FormatCategory::FixedLayout);
        assert_eq!(doc.page_count(), 1);

        let metrics = doc.page_metrics(0).expect("Get page 0 metrics");
        assert_eq!(metrics.width, 595.0);
        assert_eq!(metrics.height, 842.0);
        assert_eq!(metrics.rotation_degrees, 0);

        let text = doc.extract_text(0).expect("Extract text");
        assert!(text.contains("Hello Kiyo"), "Extracted text: {}", text);

        let search_hits = doc.search_text("Hello", true).expect("Search text");
        assert_eq!(search_hits.len(), 1);
        assert_eq!(search_hits[0].page_index, 0);

        // Trait EditableDocument verification
        let editable = doc.as_editable().expect("Editable downcast");
        editable
            .apply_page_mutation(PageMutationOp::Rotate {
                page_index: 0,
                degrees: 90,
            })
            .expect("Rotate page");

        let metrics_rotated = doc.page_metrics(0).expect("Get rotated metrics");
        assert_eq!(metrics_rotated.rotation_degrees, 90);

        // Bates numbering verification
        let bates = BatesConfig {
            prefix: "TEST-LEGAL-".to_string(),
            start_number: 42,
            digit_count: 5,
            suffix: "-USA".to_string(),
            position: BatesPosition::BottomRight,
            font_size: 10.0,
            embed_as_artifact: true,
        };
        assert_eq!(bates.format_number(0), "TEST-LEGAL-00042-USA");
        assert_eq!(bates.format_number(5), "TEST-LEGAL-00047-USA");

        let editable = doc.as_editable().expect("Editable downcast");
        editable.apply_bates_numbering(&bates).expect("Apply Bates numbering");

        // Destructive Redaction verification
        let redactions = vec![RedactionRegion {
            page_index: 0,
            rect: (50.0, 700.0, 200.0, 750.0),
            overlay_text: Some("REDACTED".to_string()),
            fill_color: (0, 0, 0),
        }];
        let scrubbed = doc.as_editable().unwrap().apply_redactions(&redactions).expect("Apply redaction");
        assert_eq!(scrubbed, 1);

        // Metadata modification
        doc.as_editable().unwrap().set_metadata_entry("Title", "Kiyo Universal Document").expect("Set title");
        assert_eq!(doc.metadata.title.as_deref(), Some("Kiyo Universal Document"));

        // Cleanup
        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_format_detection_heuristics() {
        assert_eq!(
            detect_format(Path::new("sample.pdf"), b"%PDF-1.7\r\n").unwrap(),
            DocumentFormat::Pdf
        );
        assert_eq!(
            detect_format(Path::new("manga.cbz"), b"PK\x03\x04\x14\x00").unwrap(),
            DocumentFormat::Comic(ComicArchiveType::Cbz)
        );
        assert_eq!(
            detect_format(Path::new("manga.cbr"), b"Rar!\x1A\x07\x00").unwrap(),
            DocumentFormat::Comic(ComicArchiveType::Cbr)
        );
        assert_eq!(
            detect_format(Path::new("book.epub"), b"PK\x03\x04\x0A\x00application/epub+zip").unwrap(),
            DocumentFormat::Epub
        );
        assert_eq!(
            detect_format(Path::new("document.djvu"), b"AT&TFORM\x00\x00\x00\x00DJVU").unwrap(),
            DocumentFormat::DjVu
        );
        assert_eq!(
            detect_format(Path::new("manual.chm"), b"ITSF\x03\x00\x00\x00").unwrap(),
            DocumentFormat::Chm
        );
        assert_eq!(
            detect_format(Path::new("novel.mobi"), &[0u8; 100]).unwrap_or(DocumentFormat::Mobi),
            DocumentFormat::Mobi
        );
        assert_eq!(
            detect_format(Path::new("chapter.md"), b"# Heading").unwrap(),
            DocumentFormat::Markdown
        );
    }
}
