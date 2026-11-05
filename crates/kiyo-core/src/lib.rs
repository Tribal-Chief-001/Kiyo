//! `kiyo-core`: Zero-copy PDF document model, cross-reference streaming, and manipulation.

pub mod imposition;
pub mod universal;

pub use imposition::*;
pub use lopdf::dictionary;
pub use universal::*;

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
}
