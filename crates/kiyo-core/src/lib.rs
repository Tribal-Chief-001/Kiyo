//! `kiyo-core`: Zero-copy PDF document model, cross-reference streaming, and manipulation.

#![allow(clippy::collapsible_if, clippy::manual_range_contains)]

pub mod cbr;
pub mod cbz;
pub mod epub;
pub mod imposition;
pub mod manipulation;
pub mod salvage;
pub mod universal;
pub mod zip_container;

pub use cbr::CbrDocument;
pub use cbz::CbzDocument;
pub use epub::EpubDocument;
pub use imposition::*;
pub use lopdf;
pub use lopdf::dictionary;
pub use manipulation::*;
pub use salvage::*;
pub use universal::*;
pub use zip_container::{ZipArchive, ZipEntry, natural_cmp};

use anyhow::{Result, anyhow};
use memmap2::Mmap;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
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
        lopdf::Object::Real(f) => Some(*f),
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

            if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference())
                && let Ok(info_dict) = doc.get_object(info_ref).and_then(|o| o.as_dict())
            {
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

fn parse_outline_item(
    doc: &lopdf::Document,
    item_ref: lopdf::ObjectId,
    page_id_to_index: &HashMap<lopdf::ObjectId, u32>,
) -> Option<TocNode> {
    let item = doc.get_object(item_ref).ok()?.as_dict().ok()?;
    let title = item
        .get(b"Title")
        .and_then(|o| o.as_str())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_else(|_| "Untitled Section".to_string());

    let mut target_page = 0;
    let mut target_x = None;
    let mut target_y = None;

    // Check direct /Dest or /A (/GoTo)
    let dest_obj = item.get(b"Dest").ok().or_else(|| {
        item.get(b"A").ok().and_then(|a| {
            if let Ok(ad) = a.as_dict()
                && ad
                    .get(b"S")
                    .is_ok_and(|s| s.as_name().ok() == Some(b"GoTo"))
            {
                return ad.get(b"D").ok();
            }
            None
        })
    });

    if let Some(dest) = dest_obj
        && let Ok(arr) = dest.as_array()
        && !arr.is_empty()
    {
        if let Ok(pref) = arr[0].as_reference()
            && let Some(&idx) = page_id_to_index.get(&pref)
        {
            target_page = idx;
        }
        if arr.len() >= 4 {
            target_x = obj_to_f32(&arr[2]);
            target_y = obj_to_f32(&arr[3]);
        }
    }

    let mut children = Vec::new();
    if let Ok(first_child_ref) = item.get(b"First").and_then(|o| o.as_reference()) {
        let mut curr = Some(first_child_ref);
        let mut visited = HashSet::new();
        while let Some(child_ref) = curr {
            if !visited.insert(child_ref) {
                break;
            }
            if let Some(child_node) = parse_outline_item(doc, child_ref, page_id_to_index) {
                children.push(child_node);
            }
            if let Ok(next_child) = doc.get_object(child_ref).and_then(|o| o.as_dict()) {
                curr = next_child
                    .get(b"Next")
                    .ok()
                    .and_then(|o| o.as_reference().ok());
            } else {
                break;
            }
        }
    }

    Some(TocNode {
        title,
        target_page,
        target_x,
        target_y,
        children,
    })
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

            let parse_rect_dict =
                |dict: &lopdf::Dictionary, name: &[u8]| -> Option<(f32, f32, f32, f32)> {
                    if let Ok(arr) = dict.get(name).and_then(|o| o.as_array())
                        && arr.len() == 4
                    {
                        let x1 = obj_to_f32(&arr[0]).unwrap_or(0.0);
                        let y1 = obj_to_f32(&arr[1]).unwrap_or(0.0);
                        let x2 = obj_to_f32(&arr[2]).unwrap_or(0.0);
                        let y2 = obj_to_f32(&arr[3]).unwrap_or(0.0);
                        return Some((x1, y1, x2, y2));
                    }
                    None
                };

            let mut media_box = parse_rect_dict(page_dict, b"MediaBox");
            if media_box.is_none() {
                // Inherit from parent /Pages per ISO 32000-1 §7.7.3.3
                if let Ok(parent_ref) = page_dict.get(b"Parent").and_then(|o| o.as_reference())
                    && let Ok(parent_dict) = doc.get_object(parent_ref).and_then(|o| o.as_dict())
                {
                    media_box = parse_rect_dict(parent_dict, b"MediaBox");
                }
            }
            let media_box = media_box.unwrap_or((0.0, 0.0, 595.0, 842.0));

            let crop_box = parse_rect_dict(page_dict, b"CropBox");
            let bleed_box = parse_rect_dict(page_dict, b"BleedBox");
            let trim_box = parse_rect_dict(page_dict, b"TrimBox");
            let art_box = parse_rect_dict(page_dict, b"ArtBox");

            let rotation_degrees = page_dict
                .get(b"Rotate")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as u16;

            let visible = crop_box.unwrap_or(media_box);
            let width = (visible.2 - visible.0).abs();
            let height = (visible.3 - visible.1).abs();

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
            let pages = doc.get_pages();
            let mut page_id_to_index = HashMap::new();
            for (&page_num, &page_id) in pages.iter() {
                page_id_to_index.insert(page_id, page_num.saturating_sub(1));
            }

            let mut toc = Vec::new();
            if let Ok(catalog_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference())
                && let Ok(catalog) = doc.get_object(catalog_ref).and_then(|o| o.as_dict())
                && let Ok(outlines_ref) = catalog.get(b"Outlines").and_then(|o| o.as_reference())
                && let Ok(outlines) = doc.get_object(outlines_ref).and_then(|o| o.as_dict())
                && let Ok(first_ref) = outlines.get(b"First").and_then(|o| o.as_reference())
            {
                let mut current_ref = Some(first_ref);
                let mut visited = HashSet::new();
                while let Some(node_ref) = current_ref {
                    if !visited.insert(node_ref) {
                        break;
                    }
                    if let Some(node) = parse_outline_item(doc, node_ref, &page_id_to_index) {
                        toc.push(node);
                    }
                    current_ref = doc
                        .get_object(node_ref)
                        .ok()
                        .and_then(|o| o.as_dict().ok())
                        .and_then(|d| d.get(b"Next").ok())
                        .and_then(|o| o.as_reference().ok());
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
        if query.is_empty() {
            return Ok(results);
        }

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

                for (char_idx, _) in text_cmp.match_indices(&query_norm) {
                    let start_byte = text_cmp[..char_idx]
                        .char_indices()
                        .rev()
                        .nth(39)
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    let end_byte = text_cmp[char_idx..]
                        .char_indices()
                        .nth(40 + query_norm.chars().count())
                        .map(|(i, _)| char_idx + i)
                        .unwrap_or(text_cmp.len());

                    let snippet_slice = &text[start_byte..end_byte];
                    let snippet = if start_byte > 0 && end_byte < text.len() {
                        format!("...{}...", snippet_slice.trim())
                    } else if start_byte > 0 {
                        format!("...{}", snippet_slice.trim())
                    } else if end_byte < text.len() {
                        format!("{}...", snippet_slice.trim())
                    } else {
                        snippet_slice.trim().to_string()
                    };

                    results.push(TextSearchResult {
                        page_index: page_idx,
                        bounding_box: (0.0, 0.0, 100.0, 20.0),
                        snippet,
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
            PageMutationOp::Rotate {
                page_index,
                degrees,
            } => self.with_parsed_mut(|doc| {
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
            }),
            PageMutationOp::Delete { page_index } => {
                let total = self.page_count();
                if page_index >= total {
                    return Err(anyhow!(
                        "Cannot delete page {}: document has only {} pages",
                        page_index,
                        total
                    ));
                }
                self.with_parsed_mut(|doc| {
                    let page_num = page_index + 1;
                    doc.delete_pages(&[page_num]);
                    Ok(())
                })?;
                self.metadata.page_count = self.metadata.page_count.saturating_sub(1);
                Ok(())
            }
            PageMutationOp::Move {
                from_index,
                to_index,
            } => {
                let total = self.page_count();
                if from_index >= total || to_index >= total {
                    return Err(anyhow!(
                        "Cannot move page from {} to {}: document has only {} pages",
                        from_index,
                        to_index,
                        total
                    ));
                }
                if from_index == to_index {
                    return Ok(());
                }
                self.with_parsed_mut(|doc| {
                    let catalog_ref = doc.trailer.get(b"Root").and_then(|o| o.as_reference())?;
                    let catalog = doc.get_object(catalog_ref).and_then(|o| o.as_dict())?;
                    let pages_root_ref = catalog.get(b"Pages").and_then(|o| o.as_reference())?;
                    let pages_dict = doc.get_object_mut(pages_root_ref)?;
                    if let lopdf::Object::Dictionary(d) = pages_dict
                        && let Ok(kids) = d.get_mut(b"Kids")
                        && let lopdf::Object::Array(arr) = kids
                    {
                        let from_pos = from_index as usize;
                        let to_pos = to_index as usize;
                        if from_pos < arr.len() && to_pos < arr.len() {
                            let item = arr.remove(from_pos);
                            arr.insert(to_pos, item);
                        }
                    }
                    Ok(())
                })
            }
            PageMutationOp::InsertBlank {
                at_index,
                width,
                height,
            } => {
                self.with_parsed_mut(|doc| {
                    let catalog_ref = doc.trailer.get(b"Root").and_then(|o| o.as_reference())?;
                    let catalog = doc.get_object(catalog_ref).and_then(|o| o.as_dict())?;
                    let pages_root_ref = catalog.get(b"Pages").and_then(|o| o.as_reference())?;

                    let page_id = doc.add_object(dictionary! {
                        "Type" => "Page",
                        "Parent" => pages_root_ref,
                        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
                    });

                    let pages_dict = doc.get_object_mut(pages_root_ref)?;
                    if let lopdf::Object::Dictionary(d) = pages_dict {
                        if let Ok(kids) = d.get_mut(b"Kids")
                            && let lopdf::Object::Array(arr) = kids
                        {
                            let insert_pos = (at_index as usize).min(arr.len());
                            arr.insert(insert_pos, page_id.into());
                        }
                        if let Ok(count) = d.get_mut(b"Count")
                            && let lopdf::Object::Integer(c) = count
                        {
                            *c += 1;
                        }
                    }
                    Ok(())
                })?;
                self.metadata.page_count += 1;
                Ok(())
            }
            PageMutationOp::ApplyRedaction(region) => self.apply_redactions(&[region]).map(|_| ()),
            PageMutationOp::ApplyBates(config) => self.apply_bates_numbering(&config),
        }
    }

    fn apply_redactions(&mut self, regions: &[RedactionRegion]) -> Result<usize> {
        let count = regions.len();
        self.with_parsed_mut(|doc| {
            for region in regions {
                let pages = doc.get_pages();
                let page_num = region.page_index + 1;
                if let Some(page_id) = pages.get(&page_num) {
                    let page_id_val = *page_id;

                    // 1. Destructive Content Stream Scrubbing: strip matching text operations
                    if let Ok(mut content) = doc.get_and_decode_page_content(page_id_val) {
                        let mut curr_x = 0.0f32;
                        let mut curr_y = 0.0f32;
                        let mut modified = false;

                        let min_rx = region.rect.0.min(region.rect.2) - 10.0;
                        let max_rx = region.rect.0.max(region.rect.2) + 10.0;
                        let min_ry = region.rect.1.min(region.rect.3) - 10.0;
                        let max_ry = region.rect.1.max(region.rect.3) + 10.0;

                        for op in content.operations.iter_mut() {
                            match op.operator.as_str() {
                                "Tm" => {
                                    if op.operands.len() >= 6 {
                                        curr_x = obj_to_f32(&op.operands[4]).unwrap_or(curr_x);
                                        curr_y = obj_to_f32(&op.operands[5]).unwrap_or(curr_y);
                                    }
                                }
                                "Td" | "TD" => {
                                    if op.operands.len() >= 2 {
                                        curr_x += obj_to_f32(&op.operands[0]).unwrap_or(0.0);
                                        curr_y += obj_to_f32(&op.operands[1]).unwrap_or(0.0);
                                    }
                                }
                                "Tj" | "'" => {
                                    let in_region = curr_x >= min_rx
                                        && curr_x <= max_rx
                                        && curr_y >= min_ry
                                        && curr_y <= max_ry;

                                    if in_region && let Some(first) = op.operands.first_mut() {
                                        *first = lopdf::Object::string_literal("");
                                        modified = true;
                                    }
                                }
                                "\"" => {
                                    let in_region = curr_x >= min_rx
                                        && curr_x <= max_rx
                                        && curr_y >= min_ry
                                        && curr_y <= max_ry;

                                    if in_region && op.operands.len() >= 3 {
                                        op.operands[2] = lopdf::Object::string_literal("");
                                        modified = true;
                                    }
                                }
                                "TJ" => {
                                    let in_region = curr_x >= min_rx
                                        && curr_x <= max_rx
                                        && curr_y >= min_ry
                                        && curr_y <= max_ry;

                                    if in_region
                                        && let Some(lopdf::Object::Array(arr)) =
                                            op.operands.first_mut()
                                    {
                                        for item in arr.iter_mut() {
                                            if let lopdf::Object::String(_, _) = item {
                                                *item = lopdf::Object::string_literal("");
                                                modified = true;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }

                        if modified && let Ok(encoded) = content.encode() {
                            let _ = doc.change_page_content(page_id_val, encoded);
                        }
                    }

                    // 2. Render physical opaque redaction box + optional overlay text
                    let w = (region.rect.2 - region.rect.0).abs();
                    let h = (region.rect.3 - region.rect.1).abs();
                    let min_x = region.rect.0.min(region.rect.2);
                    let min_y = region.rect.1.min(region.rect.3);

                    let mut redaction_stream = format!(
                        "\nq\n{} {} {} rg\n{} {} {} {} re\nf\nQ\n",
                        region.fill_color.0 as f32 / 255.0,
                        region.fill_color.1 as f32 / 255.0,
                        region.fill_color.2 as f32 / 255.0,
                        min_x,
                        min_y,
                        w,
                        h
                    );

                    if let Some(ref overlay) = region.overlay_text {
                        let is_dark = (region.fill_color.0 as u16
                            + region.fill_color.1 as u16
                            + region.fill_color.2 as u16)
                            < 384;
                        let text_color = if is_dark { "1 1 1" } else { "0 0 0" };
                        let font_size = (h * 0.45).clamp(6.0, 14.0);
                        let text_x = min_x + 4.0;
                        let text_y = min_y + (h / 2.0) - (font_size / 3.0);
                        let escaped = overlay
                            .replace('\\', "\\\\")
                            .replace('(', "\\(")
                            .replace(')', "\\)");

                        redaction_stream.push_str(&format!(
                            "q\n{} rg\nBT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\nQ\n",
                            text_color, font_size, text_x, text_y, escaped
                        ));
                    }

                    if let Ok(content) =
                        lopdf::content::Content::decode(&redaction_stream.into_bytes())
                    {
                        let _ = doc.add_to_page_content(page_id_val, content);
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
            let parse_rect = |dict: &lopdf::Dictionary, name: &[u8]| -> Option<(f32, f32, f32, f32)> {
                if let Ok(arr) = dict.get(name).and_then(|o| o.as_array())
                    && arr.len() == 4 {
                        let x1 = obj_to_f32(&arr[0]).unwrap_or(0.0);
                        let y1 = obj_to_f32(&arr[1]).unwrap_or(0.0);
                        let x2 = obj_to_f32(&arr[2]).unwrap_or(0.0);
                        let y2 = obj_to_f32(&arr[3]).unwrap_or(0.0);
                        return Some((x1, y1, x2, y2));
                    }
                None
            };

            for (idx, page_num) in (1..=total_pages).enumerate() {
                if let Some(page_id) = pages.get(&page_num) {
                    let page_dict = doc.get_object(*page_id).and_then(|o| o.as_dict())?;
                    let mbox = parse_rect(page_dict, b"CropBox")
                        .or_else(|| parse_rect(page_dict, b"MediaBox"))
                        .unwrap_or((0.0, 0.0, 595.0, 842.0));

                    let min_x = mbox.0.min(mbox.2);
                    let min_y = mbox.1.min(mbox.3);
                    let max_x = mbox.0.max(mbox.2);
                    let max_y = mbox.1.max(mbox.3);
                    let page_w = max_x - min_x;
                    let margin = 36.0f32;

                    let stamped_text = config.format_number(idx as u64);
                    let approx_width = stamped_text.len() as f32 * config.font_size * 0.55;

                    let x_pos = match config.position {
                        BatesPosition::TopLeft | BatesPosition::BottomLeft => min_x + margin,
                        BatesPosition::TopCenter | BatesPosition::BottomCenter => {
                            min_x + (page_w - approx_width) / 2.0
                        }
                        BatesPosition::TopRight | BatesPosition::BottomRight => {
                            (max_x - margin - approx_width).max(min_x)
                        }
                    };
                    let y_pos = match config.position {
                        BatesPosition::TopLeft | BatesPosition::TopCenter | BatesPosition::TopRight => {
                            max_y - margin - config.font_size
                        }
                        BatesPosition::BottomLeft | BatesPosition::BottomCenter | BatesPosition::BottomRight => {
                            min_y + margin
                        }
                    };

                    let escaped = stamped_text
                        .replace('\\', "\\\\")
                        .replace('(', "\\(")
                        .replace(')', "\\)");

                    let artifact_stream = if config.embed_as_artifact {
                        format!(
                            "\n/Artifact << /Type /Pagination /Subtype /Header >> BDC\nBT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\nEMC\n",
                            config.font_size, x_pos, y_pos, escaped
                        )
                    } else {
                        format!(
                            "\nBT\n/F1 {} Tf\n{} {} Td\n({}) Tj\nET\n",
                            config.font_size, x_pos, y_pos, escaped
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
            let info_id =
                if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
                    info_ref
                } else {
                    let new_id = doc.add_object(lopdf::Dictionary::new());
                    doc.trailer.set("Info", new_id);
                    new_id
                };

            if let Ok(info_obj) = doc.get_object_mut(info_id)
                && let lopdf::Object::Dictionary(dict) = info_obj
            {
                dict.set(key, lopdf::Object::string_literal(value));
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
        pdf_doc
            .objects
            .insert(pages_id, lopdf::Object::Dictionary(pages));
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
        editable
            .apply_bates_numbering(&bates)
            .expect("Apply Bates numbering");

        // Destructive Redaction verification
        let redactions = vec![RedactionRegion {
            page_index: 0,
            rect: (50.0, 700.0, 200.0, 750.0),
            overlay_text: Some("REDACTED".to_string()),
            fill_color: (0, 0, 0),
        }];
        let scrubbed = doc
            .as_editable()
            .unwrap()
            .apply_redactions(&redactions)
            .expect("Apply redaction");
        assert_eq!(scrubbed, 1);

        // Verify destructive redaction scrubbed the underlying text from extraction and search!
        let text_after = doc.extract_text(0).expect("Extract text after redaction");
        assert!(
            !text_after.contains("Hello Kiyo"),
            "Redacted text still found in extraction: {}",
            text_after
        );
        let hits_after = doc
            .search_text("Hello", true)
            .expect("Search after redaction");
        assert_eq!(hits_after.len(), 0, "Redacted text still searchable");

        // Verify InsertBlank at index 0 and Move
        doc.as_editable()
            .unwrap()
            .apply_page_mutation(PageMutationOp::InsertBlank {
                at_index: 0,
                width: 612.0,
                height: 792.0,
            })
            .expect("Insert blank at index 0");
        assert_eq!(doc.page_count(), 2);

        // Move page 0 to 1
        doc.as_editable()
            .unwrap()
            .apply_page_mutation(PageMutationOp::Move {
                from_index: 0,
                to_index: 1,
            })
            .expect("Move page");
        assert_eq!(doc.page_count(), 2);

        // Metadata modification
        doc.as_editable()
            .unwrap()
            .set_metadata_entry("Title", "Kiyo Universal Document")
            .expect("Set title");
        assert_eq!(
            doc.metadata.title.as_deref(),
            Some("Kiyo Universal Document")
        );

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
            detect_format(Path::new("manga_v5.cbr"), b"Rar!\x1A\x07\x01\x00").unwrap(),
            DocumentFormat::Comic(ComicArchiveType::Cbr)
        );
        assert_eq!(
            detect_format(Path::new("comic.cb7"), b"7z\xBC\xAF\x27\x1C\x00\x04").unwrap(),
            DocumentFormat::Comic(ComicArchiveType::Cb7)
        );
        assert_eq!(
            detect_format(
                Path::new("book.epub"),
                b"PK\x03\x04\x0A\x00application/epub+zip"
            )
            .unwrap(),
            DocumentFormat::Epub
        );
        // Verify real DjVu magic bytes work even on non-.djvu paths (e.g. sample.bin)
        assert_eq!(
            detect_format(Path::new("sample.bin"), b"AT&TFORM\x00\x00\x01\x00DJVU").unwrap(),
            DocumentFormat::DjVu
        );
        assert_eq!(
            detect_format(Path::new("multipage.bin"), b"AT&TFORM\x00\x00\x01\x00DJVM").unwrap(),
            DocumentFormat::DjVu
        );
        assert_eq!(
            detect_format(Path::new("manual.chm"), b"ITSF\x03\x00\x00\x00").unwrap(),
            DocumentFormat::Chm
        );
        // Verify real MOBI detection with BOOKMOBI magic header
        let mut mobi_buf = [0u8; 100];
        mobi_buf[60..68].copy_from_slice(b"BOOKMOBI");
        assert_eq!(
            detect_format(Path::new("novel.mobi"), &mobi_buf).unwrap(),
            DocumentFormat::Mobi
        );
        // Verify AZW3 detection with BOOKMOBI magic header
        assert_eq!(
            detect_format(Path::new("novel.azw3"), &mobi_buf).unwrap(),
            DocumentFormat::Azw3
        );
        // Verify FB2 XML detection
        assert_eq!(
            detect_format(
                Path::new("book.fb2"),
                b"<?xml version=\"1.0\"?><FictionBook>"
            )
            .unwrap(),
            DocumentFormat::Fb2
        );
        assert_eq!(
            detect_format(Path::new("book.fb2.zip"), b"PK\x03\x04\x14\x00").unwrap(),
            DocumentFormat::Fb2
        );
        assert_eq!(
            detect_format(Path::new("chapter.md"), b"# Heading").unwrap(),
            DocumentFormat::Markdown
        );
        // Verify non-document zip files are NOT misclassified as comics
        assert!(detect_format(Path::new("application.apk"), b"PK\x03\x04\x14\x00").is_err());
        assert!(detect_format(Path::new("document.docx"), b"PK\x03\x04\x14\x00").is_err());
    }

    #[test]
    fn test_zip_container_and_cbz_document() {
        // Construct a valid ZIP in memory with stored entries
        let mut zip_data = Vec::new();
        let entries = vec![
            (
                "ComicInfo.xml",
                "<ComicInfo><Title>Kiyo Manga</Title><Writer>Tribal</Writer><Manga>Yes</Manga></ComicInfo>",
            ),
            ("page10.jpg", "image10-data"),
            ("page2.jpg", "image2-data"),
            ("page1.jpg", "image1-data"),
        ];

        let mut cd_entries = Vec::new();

        for (name, content) in &entries {
            let local_offset = zip_data.len() as u32;
            let name_bytes = name.as_bytes();
            let content_bytes = content.as_bytes();

            // Local header
            zip_data.extend_from_slice(b"PK\x03\x04");
            zip_data.extend_from_slice(&[20, 0]); // version
            zip_data.extend_from_slice(&[0, 0]); // flags
            zip_data.extend_from_slice(&[0, 0]); // compression = stored
            zip_data.extend_from_slice(&[0, 0, 0, 0]); // time/date
            zip_data.extend_from_slice(&[0, 0, 0, 0]); // crc32
            zip_data.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes()); // comp size
            zip_data.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes()); // uncomp size
            zip_data.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // fn len
            zip_data.extend_from_slice(&[0, 0]); // extra len
            zip_data.extend_from_slice(name_bytes);
            zip_data.extend_from_slice(content_bytes);

            cd_entries.push((name_bytes, content_bytes.len() as u32, local_offset));
        }

        let cd_offset = zip_data.len() as u32;
        for (name_bytes, size, local_offset) in &cd_entries {
            zip_data.extend_from_slice(b"PK\x01\x02");
            zip_data.extend_from_slice(&[20, 0]); // version made by
            zip_data.extend_from_slice(&[20, 0]); // version needed
            zip_data.extend_from_slice(&[0, 0]); // flags
            zip_data.extend_from_slice(&[0, 0]); // method
            zip_data.extend_from_slice(&[0, 0, 0, 0]); // time
            zip_data.extend_from_slice(&[0, 0, 0, 0]); // crc
            zip_data.extend_from_slice(&size.to_le_bytes()); // comp size
            zip_data.extend_from_slice(&size.to_le_bytes()); // uncomp size
            zip_data.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // fn len
            zip_data.extend_from_slice(&[0, 0]); // extra len
            zip_data.extend_from_slice(&[0, 0]); // comment len
            zip_data.extend_from_slice(&[0, 0]); // disk start
            zip_data.extend_from_slice(&[0, 0]); // int attr
            zip_data.extend_from_slice(&[0, 0, 0, 0]); // ext attr
            zip_data.extend_from_slice(&local_offset.to_le_bytes()); // offset
            zip_data.extend_from_slice(name_bytes);
        }
        let cd_size = (zip_data.len() as u32) - cd_offset;

        // EOCD
        zip_data.extend_from_slice(b"PK\x05\x06");
        zip_data.extend_from_slice(&[0, 0]); // disk
        zip_data.extend_from_slice(&[0, 0]); // cd disk
        zip_data.extend_from_slice(&(cd_entries.len() as u16).to_le_bytes());
        zip_data.extend_from_slice(&(cd_entries.len() as u16).to_le_bytes());
        zip_data.extend_from_slice(&cd_size.to_le_bytes());
        zip_data.extend_from_slice(&cd_offset.to_le_bytes());
        zip_data.extend_from_slice(&[0, 0]); // comment len

        // Test ZipArchive parse
        let archive = ZipArchive::parse(&zip_data).expect("Parse ZIP");
        assert_eq!(archive.entries().len(), 4);
        let extracted = archive.read_file("page1.jpg").expect("Read page1.jpg");
        assert_eq!(extracted, b"image1-data");

        // Write to temp cbz file
        let temp_dir = std::env::temp_dir();
        let cbz_path = temp_dir.join(format!("kiyo_test_{}.cbz", std::process::id()));
        std::fs::write(&cbz_path, &zip_data).expect("Write CBZ file");

        let doc = CbzDocument::open(&cbz_path).expect("Open CBZ document");
        assert_eq!(doc.format(), DocumentFormat::Comic(ComicArchiveType::Cbz));
        assert_eq!(doc.page_count(), 3); // 3 images, ComicInfo.xml excluded
        assert_eq!(doc.metadata.title.as_deref(), Some("Kiyo Manga"));
        assert_eq!(doc.metadata.author.as_deref(), Some("Tribal"));
        assert_eq!(
            doc.metadata.reading_direction,
            Some(ReadingDirection::RightToLeft)
        );

        // Test natural sort order: page1.jpg, page2.jpg, page10.jpg
        assert_eq!(doc.image_entries[0], "page1.jpg");
        assert_eq!(doc.image_entries[1], "page2.jpg");
        assert_eq!(doc.image_entries[2], "page10.jpg");

        let img_bytes = doc.read_page_image(2).expect("Read page 2 image");
        assert_eq!(img_bytes, b"image10-data");

        // Test corrupt ZIP linear salvage by stripping EOCD
        let truncated_zip = &zip_data[..cd_offset as usize];
        let salvaged_archive = ZipArchive::parse(truncated_zip).expect("Salvage ZIP without EOCD");
        assert_eq!(salvaged_archive.entries().len(), 4);

        let _ = std::fs::remove_file(cbz_path);
    }

    #[test]
    fn test_cbr_document_rar4_and_rar5() {
        // Construct minimal valid RAR4 header with one image entry
        let mut rar4_data = Vec::new();
        rar4_data.extend_from_slice(b"Rar!\x1A\x07\x00");
        // Main archive header (0x73)
        rar4_data.extend_from_slice(&[0, 0, 0x73, 0, 0, 7, 0]);
        // File header (0x74) for "page01.jpg", size 10 bytes
        let fn_bytes = b"page01.jpg";
        let header_sz = 32 + fn_bytes.len() as u16;
        rar4_data.extend_from_slice(&[0, 0, 0x74, 0, 0]);
        rar4_data.extend_from_slice(&header_sz.to_le_bytes());
        rar4_data.extend_from_slice(&10u32.to_le_bytes()); // pack size
        rar4_data.extend_from_slice(&10u32.to_le_bytes()); // unp size
        rar4_data.extend_from_slice(&[0; 11]); // host, crc, time, ver, method
        rar4_data.extend_from_slice(&(fn_bytes.len() as u16).to_le_bytes());
        rar4_data.extend_from_slice(&[0; 4]); // attr
        rar4_data.extend_from_slice(fn_bytes);
        rar4_data.extend_from_slice(b"0123456789"); // 10 bytes data

        let temp_dir = std::env::temp_dir();
        let cbr_path = temp_dir.join(format!("kiyo_test_{}.cbr", std::process::id()));
        std::fs::write(&cbr_path, &rar4_data).expect("Write CBR file");

        let doc = CbrDocument::open(&cbr_path).expect("Open CBR document");
        assert_eq!(doc.format(), DocumentFormat::Comic(ComicArchiveType::Cbr));
        assert_eq!(doc.page_count(), 1);
        assert_eq!(doc.image_entries[0].name, "page01.jpg");
        assert_eq!(doc.image_entries[0].unpacked_size, 10);

        let img = doc.read_page_image(0).expect("Read CBR page 0 image");
        assert_eq!(img, b"0123456789");
        let seq_img = doc
            .as_sequential_art()
            .unwrap()
            .get_image_bytes(0)
            .expect("Get sequential image bytes");
        assert_eq!(seq_img, b"0123456789");

        let _ = std::fs::remove_file(cbr_path);
    }

    #[test]
    fn test_epub_document_and_reflow() {
        // Create an in-memory EPUB
        let mut zip_data = Vec::new();
        let container_xml = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let content_opf = r#"<?xml version="1.0"?><package version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Kiyo Philosophy</dc:title><dc:creator>Tribal Chief</dc:creator></metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="chap1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest><spine toc="ncx"><itemref idref="chap1"/></spine></package>"#;
        let toc_ncx = r#"<?xml version="1.0"?><ncx version="2005-1"><navMap><navPoint id="np1"><navLabel><text>First Chapter</text></navLabel><content src="chapter1.xhtml"/></navPoint></navMap></ncx>"#;
        let chap1_xhtml = r#"<html><head><title>First Chapter</title></head><body><h1>Chapter One</h1><p>High performance document engineering in Rust.</p></body></html>"#;

        let entries = vec![
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", container_xml),
            ("OEBPS/content.opf", content_opf),
            ("OEBPS/toc.ncx", toc_ncx),
            ("OEBPS/chapter1.xhtml", chap1_xhtml),
        ];

        let mut cd_entries = Vec::new();
        for (name, content) in &entries {
            let local_offset = zip_data.len() as u32;
            let name_bytes = name.as_bytes();
            let content_bytes = content.as_bytes();

            zip_data.extend_from_slice(b"PK\x03\x04");
            zip_data.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            zip_data.extend_from_slice(&[0, 0, 0, 0]);
            zip_data.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
            zip_data.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
            zip_data.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            zip_data.extend_from_slice(&[0, 0]);
            zip_data.extend_from_slice(name_bytes);
            zip_data.extend_from_slice(content_bytes);

            cd_entries.push((name_bytes, content_bytes.len() as u32, local_offset));
        }

        let cd_offset = zip_data.len() as u32;
        for (name_bytes, size, local_offset) in &cd_entries {
            zip_data.extend_from_slice(b"PK\x01\x02");
            zip_data.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            zip_data.extend_from_slice(&[0, 0, 0, 0]);
            zip_data.extend_from_slice(&size.to_le_bytes());
            zip_data.extend_from_slice(&size.to_le_bytes());
            zip_data.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            zip_data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            zip_data.extend_from_slice(&local_offset.to_le_bytes());
            zip_data.extend_from_slice(name_bytes);
        }
        let cd_size = (zip_data.len() as u32) - cd_offset;

        zip_data.extend_from_slice(b"PK\x05\x06");
        zip_data.extend_from_slice(&[0, 0, 0, 0]);
        zip_data.extend_from_slice(&(cd_entries.len() as u16).to_le_bytes());
        zip_data.extend_from_slice(&(cd_entries.len() as u16).to_le_bytes());
        zip_data.extend_from_slice(&cd_size.to_le_bytes());
        zip_data.extend_from_slice(&cd_offset.to_le_bytes());
        zip_data.extend_from_slice(&[0, 0]);

        let temp_dir = std::env::temp_dir();
        let epub_path = temp_dir.join(format!("kiyo_test_{}.epub", std::process::id()));
        std::fs::write(&epub_path, &zip_data).expect("Write EPUB file");

        let doc = EpubDocument::open(&epub_path).expect("Open EPUB document");
        assert_eq!(doc.format(), DocumentFormat::Epub);
        assert_eq!(doc.metadata.title.as_deref(), Some("Kiyo Philosophy"));
        assert_eq!(doc.metadata.author.as_deref(), Some("Tribal Chief"));
        assert_eq!(doc.page_count(), 1);

        let content = doc.extract_text(0).expect("Extract chapter 1 text");
        assert!(content.contains("Chapter One"));
        assert!(content.contains("High performance document engineering in Rust."));

        let toc = doc.table_of_contents().expect("TOC");
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "First Chapter");

        let search = doc.search_text("Rust", true).expect("Search text");
        assert_eq!(search.len(), 1);

        let reflow = doc.as_reflowable().expect("Reflow downcast");
        let layout = reflow
            .layout_pages(600.0, 800.0, 12.0, 1.4)
            .expect("Compute layout");
        assert!(layout.computed_page_count >= 1);

        let _ = std::fs::remove_file(epub_path);
    }

    #[test]
    fn test_pdf_manipulation_merge_split_crop_impose() {
        let temp_dir = std::env::temp_dir();
        let doc1_path = temp_dir.join(format!("kiyo_doc1_{}.pdf", std::process::id()));
        let doc2_path = temp_dir.join(format!("kiyo_doc2_{}.pdf", std::process::id()));
        let merged_path = temp_dir.join(format!("kiyo_merged_{}.pdf", std::process::id()));
        let cropped_path = temp_dir.join(format!("kiyo_cropped_{}.pdf", std::process::id()));
        let imposed_path = temp_dir.join(format!("kiyo_imposed_{}.pdf", std::process::id()));
        let split_dir = temp_dir.join(format!("kiyo_split_{}", std::process::id()));

        create_test_pdf_file(&doc1_path);
        create_test_pdf_file(&doc2_path);

        // 1. Merge
        merge_documents(&[doc1_path.clone(), doc2_path.clone()], &merged_path, true)
            .expect("Merge two documents");
        let merged_doc = Document::open(&merged_path).expect("Open merged document");
        assert_eq!(merged_doc.page_count(), 2);

        // 2. Split (burst and ranges)
        let burst_files =
            split_document(&merged_path, None, true, false, &split_dir).expect("Split burst");
        assert_eq!(burst_files.len(), 2);

        let range_files = split_document(&merged_path, Some("1"), false, false, &split_dir)
            .expect("Split range 1");
        assert_eq!(range_files.len(), 1);

        // 3. Crop margins
        crop_document_margins(&doc1_path, &cropped_path, true, 8.0, 0.98)
            .expect("Crop document margins");
        let cropped_doc = Document::open(&cropped_path).expect("Open cropped document");
        let metrics = cropped_doc.page_metrics(0).expect("Page metrics");
        assert!(metrics.crop_box.is_some());
        let crop = metrics.crop_box.unwrap();
        assert!(crop.0 > 0.0); // Has inward margin crop

        // 4. Impose (Booklet and N-up)
        impose_document(&merged_path, &imposed_path, 2, true, 0.2835, true, 14.17)
            .expect("Booklet imposition with marks");
        let imposed_doc = Document::open(&imposed_path).expect("Open imposed document");
        assert!(imposed_doc.page_count() >= 1);
        let imposed_lopdf = imposed_doc.load_lopdf().unwrap();
        let imposed_pages = imposed_lopdf.get_pages();
        let sheet1_id = imposed_pages.get(&1).unwrap();
        let sheet1 = imposed_lopdf
            .get_object(*sheet1_id)
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(sheet1.has(b"Resources"));
        let res = sheet1.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(res.has(b"XObject"));

        // Cleanup
        let _ = std::fs::remove_file(doc1_path);
        let _ = std::fs::remove_file(doc2_path);
        let _ = std::fs::remove_file(merged_path);
        let _ = std::fs::remove_file(cropped_path);
        let _ = std::fs::remove_file(imposed_path);
        let _ = std::fs::remove_dir_all(split_dir);
    }

    #[test]
    fn test_salvage_corrupted_pdf_pipeline() {
        // 1. Valid PDF with damaged startxref trailer
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join(format!("kiyo_salvage_test_{}.pdf", std::process::id()));
        create_test_pdf_file(&test_path);
        let mut pdf_bytes = std::fs::read(&test_path).expect("Read test PDF");

        // Damage the file header: lopdf load_mem strictly fails on non-%PDF- headers
        pdf_bytes[0..5].copy_from_slice(b"NOPE-");

        assert!(lopdf::Document::load_mem(&pdf_bytes).is_err());

        // Salvage pipeline recovers the document via Stage 3 brute-force scan!
        let (salvaged, stage) = salvage_corrupted_pdf(&pdf_bytes).expect("Salvage damaged PDF");
        assert!(salvaged.trailer.has(b"Root"));
        assert_eq!(salvaged.get_pages().len(), 1);
        assert!(
            stage == SalvageStage::BruteForceScan || stage == SalvageStage::ObjStmDecompression
        );

        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_mixed_orientation_imposition() {
        let temp_dir = std::env::temp_dir();
        let input_path = temp_dir.join(format!("kiyo_mixed_input_{}.pdf", std::process::id()));
        let imposed_path = temp_dir.join(format!("kiyo_mixed_imposed_{}.pdf", std::process::id()));

        // Create PDF with Page 1 = portrait (595x842) and Page 2 = landscape (842x595)
        let mut pdf_doc = lopdf::Document::with_version("1.4");
        let pages_id = pdf_doc.new_object_id();
        let content_id = pdf_doc.add_object(lopdf::Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 72 712 Td (Page Text) Tj ET".to_vec(),
        ));
        let page1_id = pdf_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        let page2_id = pdf_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 842.into(), 595.into()],
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page1_id.into(), page2_id.into()],
            "Count" => 2,
        };
        pdf_doc
            .objects
            .insert(pages_id, lopdf::Object::Dictionary(pages));
        let catalog_id = pdf_doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        pdf_doc.trailer.set("Root", catalog_id);
        let mut file = File::create(&input_path).expect("Create mixed orientation file");
        pdf_doc.save_to(&mut file).expect("Save mixed file");

        // Impose booklet
        impose_document(&input_path, &imposed_path, 2, true, 0.2835, true, 14.17)
            .expect("Booklet imposition of mixed orientation doc");

        let imposed_doc = Document::open(&imposed_path).expect("Open mixed imposed doc");
        assert!(imposed_doc.page_count() >= 1);
        let lopdf_doc = imposed_doc.load_lopdf().expect("Load lopdf");
        let sheet1 = lopdf_doc
            .get_object(*lopdf_doc.get_pages().get(&1).unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(sheet1.has(b"Resources"));

        let _ = std::fs::remove_file(input_path);
        let _ = std::fs::remove_file(imposed_path);
    }
}
