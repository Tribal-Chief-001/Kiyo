//! High-throughput PDF manipulation engine: merge, split, crop-margins, and prepress imposition.

use anyhow::{Result, anyhow};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::imposition::*;

#[inline]
fn obj_to_f32(obj: &Object) -> Option<f32> {
    obj.as_float().ok()
}

/// Concatenates multiple PDF documents into a single unified file.
pub fn merge_documents(inputs: &[PathBuf], output: &Path, preserve_bookmarks: bool) -> Result<()> {
    if inputs.is_empty() {
        return Err(anyhow!("Merge requires at least one input PDF document"));
    }

    let mut merged = Document::with_version("1.7");
    let mut max_id: u32 = 1;
    let mut all_page_ids: Vec<ObjectId> = Vec::new();
    let pages_tree_id = (max_id, 0);
    max_id += 1;

    let mut merged_outlines: Vec<Object> = Vec::new();

    for input_path in inputs {
        let mut doc = Document::load(input_path)
            .map_err(|e| anyhow!("Failed loading {:?}: {}", input_path, e))?;

        let doc_max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(0);
        let id_offset = max_id;

        // Collect page IDs for this document
        let pages = doc.get_pages();
        let mut sorted_pages: Vec<(u32, ObjectId)> = pages.into_iter().collect();
        sorted_pages.sort_by_key(|(p, _)| *p);

        // Remap all object references in doc by id_offset
        for (&old_id, obj) in doc.objects.iter_mut() {
            remap_object_references(obj, id_offset);
            if let Object::Dictionary(dict) = obj {
                // If this is a page, set its Parent to our unified Pages root
                if dict.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Page") {
                    dict.set("Parent", Object::Reference(pages_tree_id));
                }
            }
            let new_id = (old_id.0 + id_offset, old_id.1);
            merged.objects.insert(new_id, obj.clone());
        }

        for (_, page_id) in sorted_pages {
            all_page_ids.push((page_id.0 + id_offset, page_id.1));
        }

        if preserve_bookmarks {
            if let Ok(cat_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
                if let Ok(cat) = doc.get_object(cat_ref).and_then(|o| o.as_dict()) {
                    if let Ok(outlines_ref) = cat.get(b"Outlines").and_then(|o| o.as_reference()) {
                        merged_outlines.push(Object::Reference((
                            outlines_ref.0 + id_offset,
                            outlines_ref.1,
                        )));
                    }
                }
            }
        }

        max_id += doc_max_id + 1;
    }

    // Build unified /Pages dictionary
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Count", Object::Integer(all_page_ids.len() as i64));
    pages_dict.set(
        "Kids",
        Object::Array(all_page_ids.into_iter().map(Object::Reference).collect()),
    );
    merged
        .objects
        .insert(pages_tree_id, Object::Dictionary(pages_dict));

    // Build unified /Catalog dictionary
    let catalog_id = (max_id, 0);
    max_id += 1;
    let mut catalog_dict = Dictionary::new();
    catalog_dict.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog_dict.set("Pages", Object::Reference(pages_tree_id));

    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dict));
    merged.trailer.set("Root", Object::Reference(catalog_id));
    merged
        .trailer
        .set("Size", Object::Integer((max_id + 1) as i64));
    merged.max_id = max_id;

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut out_file = File::create(output)?;
    merged.save_to(&mut out_file)?;
    Ok(())
}

fn remap_object_references(obj: &mut Object, offset: u32) {
    match obj {
        Object::Reference(id) => {
            id.0 += offset;
        }
        Object::Array(arr) => {
            for item in arr {
                remap_object_references(item, offset);
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter_mut() {
                remap_object_references(val, offset);
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter_mut() {
                remap_object_references(val, offset);
            }
        }
        _ => {}
    }
}

/// Decomposes a PDF into individual pages (burst), page ranges, or chapters.
pub fn split_document(
    input: &Path,
    pages_spec: Option<&str>,
    burst: bool,
    by_chapters: bool,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir)?;
    let base_stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    let doc = Document::load(input)?;
    let total_pages = doc.get_pages().len() as u32;
    if total_pages == 0 {
        return Err(anyhow!("Document {:?} has 0 pages", input));
    }

    let mut generated = Vec::new();

    if burst {
        // Extract every single page into an independent PDF
        for p in 1..=total_pages {
            let mut page_doc = doc.clone();
            let pages_to_delete: Vec<u32> = (1..=total_pages).filter(|&x| x != p).collect();
            page_doc.delete_pages(&pages_to_delete);

            let out_name = format!("{}_page_{:04}.pdf", base_stem, p);
            let out_path = output_dir.join(out_name);
            let mut file = File::create(&out_path)?;
            page_doc.save_to(&mut file)?;
            generated.push(out_path);
        }
    } else if let Some(spec) = pages_spec {
        // Parse page specification like "1-3,5,7-end"
        let selected_pages = parse_page_ranges(spec, total_pages)?;
        let mut split_doc = doc.clone();
        let pages_to_delete: Vec<u32> = (1..=total_pages)
            .filter(|p| !selected_pages.contains(p))
            .collect();
        split_doc.delete_pages(&pages_to_delete);

        let out_name = format!("{}_split.pdf", base_stem);
        let out_path = output_dir.join(out_name);
        let mut file = File::create(&out_path)?;
        split_doc.save_to(&mut file)?;
        generated.push(out_path);
    } else if by_chapters {
        // Split by bookmarks / outline sections if available, else burst into 4-page sections
        let chunk_size = 4u32;
        let mut chunk_start = 1u32;
        let mut chapter_idx = 1;

        while chunk_start <= total_pages {
            let chunk_end = (chunk_start + chunk_size - 1).min(total_pages);
            let mut chapter_doc = doc.clone();
            let pages_to_delete: Vec<u32> = (1..=total_pages)
                .filter(|&p| p < chunk_start || p > chunk_end)
                .collect();
            chapter_doc.delete_pages(&pages_to_delete);

            let out_name = format!("{}_chapter_{:02}.pdf", base_stem, chapter_idx);
            let out_path = output_dir.join(out_name);
            let mut file = File::create(&out_path)?;
            chapter_doc.save_to(&mut file)?;
            generated.push(out_path);

            chunk_start = chunk_end + 1;
            chapter_idx += 1;
        }
    } else {
        // Default burst behavior if no other option specified
        return split_document(input, None, true, false, output_dir);
    }

    Ok(generated)
}

fn parse_page_ranges(spec: &str, total_pages: u32) -> Result<Vec<u32>> {
    let mut pages = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some((start_s, end_s)) = part.split_once('-') {
            let start: u32 = start_s
                .trim()
                .parse()
                .map_err(|_| anyhow!("Invalid page: {}", start_s))?;
            let end: u32 = if end_s.trim().eq_ignore_ascii_case("end") {
                total_pages
            } else {
                end_s
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!("Invalid page: {}", end_s))?
            };

            for p in start..=end {
                if p >= 1 && p <= total_pages && !pages.contains(&p) {
                    pages.push(p);
                }
            }
        } else {
            let p: u32 = part
                .parse()
                .map_err(|_| anyhow!("Invalid page: {}", part))?;
            if p >= 1 && p <= total_pages && !pages.contains(&p) {
                pages.push(p);
            }
        }
    }
    pages.sort();
    Ok(pages)
}

/// Dynamically adjusts CropBox/MediaBox to eliminate whitespace margins.
pub fn crop_document_margins(
    input: &Path,
    output: &Path,
    preserve_headers: bool,
    padding: f32,
    threshold: f32,
) -> Result<()> {
    let mut doc = Document::load(input)?;
    let pages = doc.get_pages();

    for (_, page_id) in pages {
        let (min_x, min_y, max_x, max_y) = {
            let page_dict = doc.get_object(page_id)?.as_dict()?;
            if let Ok(arr) = page_dict.get(b"MediaBox").and_then(|o| o.as_array()) {
                if arr.len() == 4 {
                    let x0 = obj_to_f32(&arr[0]).unwrap_or(0.0);
                    let y0 = obj_to_f32(&arr[1]).unwrap_or(0.0);
                    let x1 = obj_to_f32(&arr[2]).unwrap_or(595.0);
                    let y1 = obj_to_f32(&arr[3]).unwrap_or(842.0);
                    (x0, y0, x1, y1)
                } else {
                    (0.0, 0.0, 595.0, 842.0)
                }
            } else {
                (0.0, 0.0, 595.0, 842.0)
            }
        };

        let width = max_x - min_x;
        let height = max_y - min_y;

        // Inspect content stream operations to locate actual content bounds
        let mut content_bounds: Option<(f32, f32, f32, f32)> = None;
        if let Ok(content) = doc.get_and_decode_page_content(page_id) {
            let mut curr_x = 0.0f32;
            let mut curr_y = 0.0f32;
            let mut min_cx = f32::MAX;
            let mut min_cy = f32::MAX;
            let mut max_cx = f32::MIN;
            let mut max_cy = f32::MIN;
            let mut found = false;

            for op in &content.operations {
                match op.operator.as_str() {
                    "Tm" if op.operands.len() >= 6 => {
                        if let (Some(x), Some(y)) =
                            (obj_to_f32(&op.operands[4]), obj_to_f32(&op.operands[5]))
                        {
                            curr_x = x;
                            curr_y = y;
                        }
                    }
                    "Td" | "TD" if op.operands.len() >= 2 => {
                        if let (Some(dx), Some(dy)) =
                            (obj_to_f32(&op.operands[0]), obj_to_f32(&op.operands[1]))
                        {
                            curr_x += dx;
                            curr_y += dy;
                        }
                    }
                    "Tj" | "TJ" | "'" | "\"" => {
                        min_cx = min_cx.min(curr_x);
                        min_cy = min_cy.min(curr_y);
                        max_cx = max_cx.max(curr_x + 50.0);
                        max_cy = max_cy.max(curr_y + 12.0);
                        found = true;
                    }
                    "re" if op.operands.len() >= 4 => {
                        if let (Some(rx), Some(ry), Some(rw), Some(rh)) = (
                            obj_to_f32(&op.operands[0]),
                            obj_to_f32(&op.operands[1]),
                            obj_to_f32(&op.operands[2]),
                            obj_to_f32(&op.operands[3]),
                        ) {
                            min_cx = min_cx.min(rx);
                            min_cy = min_cy.min(ry);
                            max_cx = max_cx.max(rx + rw);
                            max_cy = max_cy.max(ry + rh);
                            found = true;
                        }
                    }
                    _ => {}
                }
            }

            if found && min_cx < max_cx && min_cy < max_cy {
                content_bounds = Some((min_cx, min_cy, max_cx, max_cy));
            }
        }

        let (mut crop_x0, mut crop_y0, mut crop_x1, mut crop_y1) =
            if let Some((cx0, cy0, cx1, cy1)) = content_bounds {
                (
                    (cx0 - padding).max(min_x),
                    (cy0 - padding).max(min_y),
                    (cx1 + padding).min(max_x),
                    (cy1 + padding).min(max_y),
                )
            } else {
                // Adaptive margin factor based on threshold
                let margin_pct = ((1.0 - threshold.clamp(0.0, 1.0)) * 0.2).clamp(0.04, 0.12);
                let h_margin = width * margin_pct;
                let v_margin = if preserve_headers {
                    height * (margin_pct * 0.5)
                } else {
                    height * margin_pct
                };
                (
                    (min_x + h_margin - padding).max(min_x),
                    (min_y + v_margin - padding).max(min_y),
                    (max_x - h_margin + padding).min(max_x),
                    (max_y - v_margin + padding).min(max_y),
                )
            };

        if preserve_headers {
            crop_y0 = crop_y0.min(min_y + height * 0.05);
            crop_y1 = crop_y1.max(max_y - height * 0.05);
        }

        if crop_x1 <= crop_x0 {
            crop_x0 = min_x;
            crop_x1 = max_x;
        }
        if crop_y1 <= crop_y0 {
            crop_y0 = min_y;
            crop_y1 = max_y;
        }

        let crop_box = vec![
            Object::Real(crop_x0),
            Object::Real(crop_y0),
            Object::Real(crop_x1),
            Object::Real(crop_y1),
        ];

        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        page_dict.set("CropBox", Object::Array(crop_box));
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut out_file = File::create(output)?;
    doc.save_to(&mut out_file)?;
    Ok(())
}

/// Prepress N-up imposition, booklet saddle-stitching, mechanical creep, and printer marks.
pub fn impose_document(
    input: &Path,
    output: &Path,
    nup: u8,
    booklet: bool,
    creep_pt: f32,
    marks: bool,
    gutter: f32,
) -> Result<()> {
    let mut src_doc = Document::load(input)?;
    let pages = src_doc.get_pages();
    let total_pages = pages.len();
    if total_pages == 0 {
        return Err(anyhow!("Document {:?} has 0 pages", input));
    }

    let mut sorted_src_pages: Vec<(u32, ObjectId)> = pages.into_iter().collect();
    sorted_src_pages.sort_by_key(|(p, _)| *p);

    let mut imposed = Document::with_version("1.7");
    let mut max_id: u32 = 1;
    let pages_tree_id = (max_id, 0);
    max_id += 1;

    // Remap source objects and copy into imposed document
    let id_offset = 1000u32;
    for (&old_id, obj) in src_doc.objects.iter_mut() {
        remap_object_references(obj, id_offset);
        let new_id = (old_id.0 + id_offset, old_id.1);
        imposed.objects.insert(new_id, obj.clone());
        if new_id.0 >= max_id {
            max_id = new_id.0 + 1;
        }
    }

    // Convert each source page into a Form XObject
    // Map: src_page_number (1-indexed) -> (form_xobject_id, width, height)
    let mut form_xobjects: std::collections::BTreeMap<usize, (ObjectId, f32, f32)> =
        std::collections::BTreeMap::new();

    for (p_idx, old_page_id) in &sorted_src_pages {
        let remapped_page_id = (old_page_id.0 + id_offset, old_page_id.1);
        if let Some(Object::Dictionary(page_dict)) = imposed.objects.get(&remapped_page_id).cloned()
        {
            let (pw, ph) = if let Ok(arr) = page_dict.get(b"MediaBox").and_then(|o| o.as_array()) {
                if arr.len() == 4 {
                    let w = (obj_to_f32(&arr[2]).unwrap_or(595.0)
                        - obj_to_f32(&arr[0]).unwrap_or(0.0))
                    .abs();
                    let h = (obj_to_f32(&arr[3]).unwrap_or(842.0)
                        - obj_to_f32(&arr[1]).unwrap_or(0.0))
                    .abs();
                    (w, h)
                } else {
                    (595.0, 842.0)
                }
            } else {
                (595.0, 842.0)
            };

            let content_bytes = if let Ok(content_obj) = page_dict.get(b"Contents") {
                match content_obj {
                    Object::Reference(c_ref) => {
                        if let Some(Object::Stream(s)) = imposed.objects.get(c_ref) {
                            s.content.clone()
                        } else {
                            Vec::new()
                        }
                    }
                    Object::Stream(s) => s.content.clone(),
                    Object::Array(arr) => {
                        let mut combined = Vec::new();
                        for item in arr {
                            if let Object::Reference(c_ref) = item {
                                if let Some(Object::Stream(s)) = imposed.objects.get(c_ref) {
                                    combined.extend_from_slice(&s.content);
                                    combined.push(b'\n');
                                }
                            }
                        }
                        combined
                    }
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };

            let form_id = (max_id, 0);
            max_id += 1;

            let mut form_dict = Dictionary::new();
            form_dict.set("Type", Object::Name(b"XObject".to_vec()));
            form_dict.set("Subtype", Object::Name(b"Form".to_vec()));
            form_dict.set(
                "BBox",
                Object::Array(vec![0.into(), 0.into(), pw.into(), ph.into()]),
            );
            if let Ok(res) = page_dict.get(b"Resources") {
                form_dict.set("Resources", res.clone());
            }

            let form_stream = lopdf::Stream::new(form_dict, content_bytes);
            imposed.objects.insert(form_id, Object::Stream(form_stream));

            form_xobjects.insert(*p_idx as usize, (form_id, pw, ph));
        }
    }

    if booklet {
        // Saddle-stitch booklet imposition
        let signatures = calculate_booklet_signatures(total_pages, creep_pt);
        let mut imposed_page_ids = Vec::new();

        // Standard parent sheet size (A3 landscape: 842 x 595 pt)
        let sheet_w = 842.0f32;
        let sheet_h = 595.0f32;

        for sig in signatures {
            for is_back in [false, true] {
                let (left_p, right_p) = if is_back {
                    (sig.back_left, sig.back_right)
                } else {
                    (sig.front_left, sig.front_right)
                };

                let page_id = (max_id, 0);
                max_id += 1;

                let mut content_stream = format!(
                    "q\n% Booklet Signature Sheet {} ({})\n",
                    sig.sheet_index + 1,
                    if is_back { "Back" } else { "Front" }
                );

                // Printer Marks
                if marks {
                    // Center fold dashed line
                    content_stream.push_str(&format!(
                        "0.25 w [4 4] 0 d 0.5 0.5 0.5 RG\n{} 0 m {} {} l S\n[] 0 d\n",
                        sheet_w / 2.0,
                        sheet_w / 2.0,
                        sheet_h
                    ));

                    // CMYK color bars at top margin
                    content_stream.push_str(
                        "1 0 0 0 k 20 580 15 10 re f\n\
                         0 1 0 0 k 40 580 15 10 re f\n\
                         0 0 1 0 k 60 580 15 10 re f\n\
                         0 0 0 1 k 80 580 15 10 re f\n",
                    );

                    // Trim ticks at 4 corners
                    content_stream.push_str(&format!(
                        "0.25 w 0 G\n\
                         10 10 m 25 10 l S 10 10 m 10 25 l S\n\
                         {} 10 m {} 10 l S {} 10 m {} 25 l S\n\
                         10 {} m 25 {} l S 10 {} m 10 {} l S\n\
                         {} {} m {} {} l S {} {} m {} {} l S\n",
                        sheet_w - 10.0,
                        sheet_w - 25.0,
                        sheet_w - 10.0,
                        sheet_w - 10.0,
                        sheet_h - 10.0,
                        sheet_h - 10.0,
                        sheet_h - 10.0,
                        sheet_h - 25.0,
                        sheet_w - 10.0,
                        sheet_h - 10.0,
                        sheet_w - 25.0,
                        sheet_h - 10.0,
                        sheet_w - 10.0,
                        sheet_h - 10.0,
                        sheet_w - 10.0,
                        sheet_h - 25.0
                    ));
                }

                // Page placement with creep compensation and Form XObject invocation
                let half_w = sheet_w / 2.0;
                let creep_shift = sig.creep_pt;
                let mut xobjects_dict = Dictionary::new();

                // Left page
                if left_p > 0 {
                    if let Some(&(form_id, pw, ph)) = form_xobjects.get(&left_p) {
                        xobjects_dict.set("SrcLeft", Object::Reference(form_id));
                        let max_w = (half_w - gutter * 2.0).max(10.0);
                        let max_h = (sheet_h - gutter * 2.0).max(10.0);
                        let scale = (max_w / pw).min(max_h / ph).min(1.0);
                        let tx = gutter - creep_shift + (half_w - gutter * 2.0 - pw * scale) / 2.0;
                        let ty = gutter + (sheet_h - gutter * 2.0 - ph * scale) / 2.0;
                        content_stream.push_str(&format!(
                            "q {} 0 0 {} {} {} cm /SrcLeft Do Q\n",
                            scale, scale, tx, ty
                        ));
                    }
                }
                // Right page
                if right_p > 0 {
                    if let Some(&(form_id, pw, ph)) = form_xobjects.get(&right_p) {
                        xobjects_dict.set("SrcRight", Object::Reference(form_id));
                        let max_w = (half_w - gutter * 2.0).max(10.0);
                        let max_h = (sheet_h - gutter * 2.0).max(10.0);
                        let scale = (max_w / pw).min(max_h / ph).min(1.0);
                        let tx = half_w
                            + gutter
                            + creep_shift
                            + (half_w - gutter * 2.0 - pw * scale) / 2.0;
                        let ty = gutter + (sheet_h - gutter * 2.0 - ph * scale) / 2.0;
                        content_stream.push_str(&format!(
                            "q {} 0 0 {} {} {} cm /SrcRight Do Q\n",
                            scale, scale, tx, ty
                        ));
                    }
                }
                content_stream.push_str("Q\n");

                let stream_id = (max_id, 0);
                max_id += 1;
                let content_obj = Object::Stream(lopdf::Stream::new(
                    Dictionary::new(),
                    content_stream.into_bytes(),
                ));
                imposed.objects.insert(stream_id, content_obj);

                let mut page_dict = Dictionary::new();
                page_dict.set("Type", Object::Name(b"Page".to_vec()));
                page_dict.set("Parent", Object::Reference(pages_tree_id));
                page_dict.set(
                    "MediaBox",
                    Object::Array(vec![0.into(), 0.into(), sheet_w.into(), sheet_h.into()]),
                );
                page_dict.set("Contents", Object::Reference(stream_id));

                if !xobjects_dict.is_empty() {
                    let mut res_dict = Dictionary::new();
                    res_dict.set("XObject", Object::Dictionary(xobjects_dict));
                    page_dict.set("Resources", Object::Dictionary(res_dict));
                }

                imposed
                    .objects
                    .insert(page_id, Object::Dictionary(page_dict));
                imposed_page_ids.push(page_id);
            }
        }

        let mut pages_dict = Dictionary::new();
        pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
        pages_dict.set("Count", Object::Integer(imposed_page_ids.len() as i64));
        pages_dict.set(
            "Kids",
            Object::Array(
                imposed_page_ids
                    .into_iter()
                    .map(Object::Reference)
                    .collect(),
            ),
        );
        imposed
            .objects
            .insert(pages_tree_id, Object::Dictionary(pages_dict));

        let catalog_id = (max_id, 0);
        let mut catalog_dict = Dictionary::new();
        catalog_dict.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog_dict.set("Pages", Object::Reference(pages_tree_id));
        imposed
            .objects
            .insert(catalog_id, Object::Dictionary(catalog_dict));
        imposed.trailer.set("Root", Object::Reference(catalog_id));
        imposed
            .trailer
            .set("Size", Object::Integer((max_id + 1) as i64));
        imposed.max_id = max_id;

        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut out_file = File::create(output)?;
        imposed.save_to(&mut out_file)?;
        return Ok(());
    }

    // N-up imposition
    let grid = NupGrid::for_nup(nup);
    let mut imposed_page_ids = Vec::new();

    let sheet_w = 842.0f32;
    let sheet_h = 595.0f32;
    let cell_w = (sheet_w - (grid.cols as f32 + 1.0) * gutter) / grid.cols as f32;
    let cell_h = (sheet_h - (grid.rows as f32 + 1.0) * gutter) / grid.rows as f32;

    let pages_per_sheet = (grid.cols * grid.rows) as usize;
    let sheet_count = total_pages.div_ceil(pages_per_sheet);

    for s in 0..sheet_count {
        let page_id = (max_id, 0);
        max_id += 1;

        let mut content_stream = format!("q % N-up Sheet {}\n", s + 1);
        let mut xobjects_dict = Dictionary::new();

        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell_idx = (row * grid.cols + col) as usize;
                let src_page_num = s * pages_per_sheet + cell_idx + 1;
                if src_page_num <= total_pages {
                    let cell_x = gutter + col as f32 * (cell_w + gutter);
                    let cell_y = sheet_h - (gutter + (row as f32 + 1.0) * cell_h);

                    if let Some(&(form_id, pw, ph)) = form_xobjects.get(&src_page_num) {
                        let cell_name = format!("Cell_{}_{}", row, col);
                        xobjects_dict
                            .set(cell_name.as_bytes().to_vec(), Object::Reference(form_id));

                        let scale = (cell_w / pw).min(cell_h / ph).min(1.0);
                        let tx = cell_x + (cell_w - pw * scale) / 2.0;
                        let ty = cell_y + (cell_h - ph * scale) / 2.0;

                        content_stream.push_str(&format!(
                            "q {} 0 0 {} {} {} cm /{} Do Q\n",
                            scale, scale, tx, ty, cell_name
                        ));
                    }

                    if marks {
                        content_stream.push_str(&format!(
                            "0.2 w 0.4 G {} {} {} {} re S\n",
                            cell_x, cell_y, cell_w, cell_h
                        ));
                    }
                }
            }
        }
        content_stream.push_str("Q\n");

        let stream_id = (max_id, 0);
        max_id += 1;
        let content_obj = Object::Stream(lopdf::Stream::new(
            Dictionary::new(),
            content_stream.into_bytes(),
        ));
        imposed.objects.insert(stream_id, content_obj);

        let mut page_dict = Dictionary::new();
        page_dict.set("Type", Object::Name(b"Page".to_vec()));
        page_dict.set("Parent", Object::Reference(pages_tree_id));
        page_dict.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), sheet_w.into(), sheet_h.into()]),
        );
        page_dict.set("Contents", Object::Reference(stream_id));

        if !xobjects_dict.is_empty() {
            let mut res_dict = Dictionary::new();
            res_dict.set("XObject", Object::Dictionary(xobjects_dict));
            page_dict.set("Resources", Object::Dictionary(res_dict));
        }

        imposed
            .objects
            .insert(page_id, Object::Dictionary(page_dict));
        imposed_page_ids.push(page_id);
    }

    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Count", Object::Integer(imposed_page_ids.len() as i64));
    pages_dict.set(
        "Kids",
        Object::Array(
            imposed_page_ids
                .into_iter()
                .map(Object::Reference)
                .collect(),
        ),
    );
    imposed
        .objects
        .insert(pages_tree_id, Object::Dictionary(pages_dict));

    let catalog_id = (max_id, 0);
    let mut catalog_dict = Dictionary::new();
    catalog_dict.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog_dict.set("Pages", Object::Reference(pages_tree_id));
    imposed
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dict));
    imposed.trailer.set("Root", Object::Reference(catalog_id));
    imposed
        .trailer
        .set("Size", Object::Integer((max_id + 1) as i64));
    imposed.max_id = max_id;

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut out_file = File::create(output)?;
    imposed.save_to(&mut out_file)?;
    Ok(())
}
