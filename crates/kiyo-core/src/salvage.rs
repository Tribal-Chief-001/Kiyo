//! 4-Stage Corrupt Xref Salvage Pipeline with Compressed Object Stream (/ObjStm) Decompression.

use anyhow::{Result, anyhow};
use flate2::read::ZlibDecoder;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::io::Read;

/// Outcome report of the salvage pipeline indicating which stage succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalvageStage {
    StandardTrailer,
    LinearStreamScan,
    BruteForceScan,
    ObjStmDecompression,
}

/// Salvages damaged, truncated, or corrupt PDF byte streams into a parsed `lopdf::Document`.
pub fn salvage_corrupted_pdf(bytes: &[u8]) -> Result<(Document, SalvageStage)> {
    // Stage 1: Standard backward trailer scan
    if let Ok(doc) = Document::load_mem(bytes) {
        if doc.trailer.has(b"Root") && !doc.get_pages().is_empty() {
            return Ok((doc, SalvageStage::StandardTrailer));
        }
    }

    // Stage 2: Linear stream search for latest valid xref or /XRef stream
    if let Ok(doc) = stage2_linear_stream_search(bytes) {
        return Ok((doc, SalvageStage::LinearStreamScan));
    }

    // Stage 3 & 4: Full brute-force byte scan + /ObjStm decompression
    let (doc, had_objstm) = stage3_and_4_brute_force_with_objstm(bytes)?;
    let stage = if had_objstm {
        SalvageStage::ObjStmDecompression
    } else {
        SalvageStage::BruteForceScan
    };
    Ok((doc, stage))
}

fn stage2_linear_stream_search(bytes: &[u8]) -> Result<Document> {
    // Scan backward for "xref\r" or "xref\n"
    let mut pos = bytes.len().saturating_sub(10);
    while pos > 10 {
        if &bytes[pos..pos + 4] == b"xref"
            && (bytes[pos + 4] == b'\r' || bytes[pos + 4] == b'\n' || bytes[pos + 4] == b' ')
        {
            // Found xref keyword. Try synthesizing a startxref pointer at EOF
            let synthetic_trailer = format!("\nstartxref\n{}\n%%EOF", pos);
            let mut patched = Vec::with_capacity(bytes.len() + synthetic_trailer.len());
            patched.extend_from_slice(bytes);
            patched.extend_from_slice(synthetic_trailer.as_bytes());

            if let Ok(doc) = Document::load_mem(&patched) {
                if doc.trailer.has(b"Root") {
                    return Ok(doc);
                }
            }
        }
        pos -= 1;
    }

    Err(anyhow!("Stage 2 linear stream search failed"))
}

fn stage3_and_4_brute_force_with_objstm(bytes: &[u8]) -> Result<(Document, bool)> {
    let mut raw_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut root_id: Option<ObjectId> = None;
    let info_id: Option<ObjectId> = None;
    let mut objstm_ids: Vec<ObjectId> = Vec::new();

    // 1. Scan for "\d+ \d+ obj" tokens
    let mut i = 0;
    while i + 10 < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut num1 = 0u32;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                num1 = num1
                    .saturating_mul(10)
                    .saturating_add((bytes[i] - b'0') as u32);
                i += 1;
            }

            // Must have whitespace
            if i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
                let mut num2 = 0u16;
                let gen_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    num2 = num2
                        .saturating_mul(10)
                        .saturating_add((bytes[i] - b'0') as u16);
                    i += 1;
                }

                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }

                if i > gen_start && i + 3 <= bytes.len() && &bytes[i..i + 3] == b"obj" {
                    let obj_id = (num1, num2);
                    i += 3;

                    // Parse the object payload starting here
                    if let Some((obj, next_pos)) = parse_single_pdf_object(&bytes[i..]) {
                        // Check if this is Catalog or Info or ObjStm
                        if let Object::Dictionary(dict) = &obj {
                            if let Ok(t) = dict.get(b"Type").and_then(|o| o.as_name()) {
                                if t == b"Catalog" {
                                    root_id = Some(obj_id);
                                } else if t == b"ObjStm" {
                                    objstm_ids.push(obj_id);
                                }
                            }
                        } else if let Object::Stream(stream) = &obj {
                            if let Ok(t) = stream.dict.get(b"Type").and_then(|o| o.as_name()) {
                                if t == b"Catalog" {
                                    root_id = Some(obj_id);
                                } else if t == b"ObjStm" {
                                    objstm_ids.push(obj_id);
                                }
                            }
                        }

                        raw_objects.insert(obj_id, obj);
                        i += next_pos;
                        continue;
                    }
                }
            }
            i = start + 1;
            continue;
        }
        i += 1;
    }

    // 2. Stage 4: Decompress and index Compressed Object Streams (/ObjStm) per ISO 32000-1 §7.5.7
    let mut had_objstm = false;
    for &stm_id in &objstm_ids {
        if let Some(Object::Stream(stream)) = raw_objects.get(&stm_id) {
            let n_count = stream.dict.get(b"N").and_then(|o| o.as_i64()).unwrap_or(0) as usize;
            let first_offset = stream
                .dict
                .get(b"First")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as usize;

            // Decompress stream content
            let decompressed = decompress_stream_bytes(stream);
            if let Ok(decompressed) = decompressed {
                if first_offset <= decompressed.len() && n_count > 0 {
                    had_objstm = true;
                    // Parse header: n pairs of (obj_num, offset)
                    let header_str = String::from_utf8_lossy(&decompressed[..first_offset]);
                    let mut tokens = header_str.split_whitespace();
                    let mut pairs = Vec::with_capacity(n_count);

                    for _ in 0..n_count {
                        if let (Some(obj_str), Some(off_str)) = (tokens.next(), tokens.next()) {
                            if let (Ok(o_num), Ok(o_off)) =
                                (obj_str.parse::<u32>(), off_str.parse::<usize>())
                            {
                                pairs.push((o_num, o_off));
                            }
                        }
                    }

                    // Extract and parse each nested object
                    for k in 0..pairs.len() {
                        let obj_num = pairs[k].0;
                        let start = first_offset + pairs[k].1;
                        let end = if k + 1 < pairs.len() {
                            first_offset + pairs[k + 1].1
                        } else {
                            decompressed.len()
                        };

                        if start < decompressed.len() && end <= decompressed.len() && start < end {
                            let obj_slice = &decompressed[start..end];
                            if let Some((inner_obj, _)) = parse_single_pdf_object(obj_slice) {
                                let inner_id = (obj_num, 0);
                                if let Object::Dictionary(ref dict) = inner_obj {
                                    if let Ok(t) = dict.get(b"Type").and_then(|o| o.as_name()) {
                                        if t == b"Catalog" && root_id.is_none() {
                                            root_id = Some(inner_id);
                                        }
                                    }
                                }
                                raw_objects.insert(inner_id, inner_obj);
                            }
                        }
                    }
                }
            }
        }
    }

    if raw_objects.is_empty() {
        return Err(anyhow!(
            "Stage 3/4 salvage found no indirect objects in corrupted PDF"
        ));
    }

    // Assemble salvaged document
    let mut doc = Document::with_version("1.7");
    doc.objects = raw_objects;

    // Resolve or synthesize Root Catalog
    if let Some(r_id) = root_id {
        doc.trailer.set("Root", r_id);
    } else {
        // Look for any dictionary with Type = Catalog
        for (&id, obj) in &doc.objects {
            if let Object::Dictionary(dict) = obj {
                if let Ok(t) = dict.get(b"Type").and_then(|o| o.as_name()) {
                    if t == b"Catalog" {
                        doc.trailer.set("Root", id);
                        break;
                    }
                }
            }
        }
    }

    if let Some(i_id) = info_id {
        doc.trailer.set("Info", i_id);
    }

    doc.trailer.set("Size", doc.objects.len() as i64 + 1);

    Ok((doc, had_objstm))
}

fn decompress_stream_bytes(stream: &lopdf::Stream) -> Result<Vec<u8>> {
    // If Filter is FlateDecode
    let filter = stream.dict.get(b"Filter").and_then(|o| o.as_name()).ok();
    if filter == Some(b"FlateDecode") || filter.is_none() {
        let mut decoder = ZlibDecoder::new(&stream.content[..]);
        let mut out = Vec::new();
        if decoder.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return Ok(out);
        }
        // Try raw deflate
        let mut raw_decoder = flate2::read::DeflateDecoder::new(&stream.content[..]);
        let mut raw_out = Vec::new();
        if raw_decoder.read_to_end(&mut raw_out).is_ok() && !raw_out.is_empty() {
            return Ok(raw_out);
        }
    }
    Ok(stream.content.clone())
}

fn parse_single_pdf_object(slice: &[u8]) -> Option<(Object, usize)> {
    // Skip leading whitespace
    let mut i = 0;
    while i < slice.len() && (slice[i].is_ascii_whitespace() || slice[i] == 0) {
        i += 1;
    }
    if i >= slice.len() {
        return None;
    }

    let remaining = &slice[i..];

    // Dictionary or Stream
    if remaining.starts_with(b"<<") {
        let mut depth = 0;
        let mut pos = 0;
        while pos + 1 < remaining.len() {
            if &remaining[pos..pos + 2] == b"<<" {
                depth += 1;
                pos += 2;
            } else if &remaining[pos..pos + 2] == b">>" {
                depth -= 1;
                pos += 2;
                if depth == 0 {
                    break;
                }
            } else {
                pos += 1;
            }
        }

        if depth == 0 && pos <= remaining.len() {
            let dict_bytes = &remaining[..pos];
            let dict_str = String::from_utf8_lossy(dict_bytes);
            let mut dict = Dictionary::new();
            parse_simple_dict_entries(&dict_str, &mut dict);

            // Check if followed by "stream\r\n" ... "endstream"
            let after_dict = &remaining[pos..];
            let mut s_idx = 0;
            while s_idx < after_dict.len() && after_dict[s_idx].is_ascii_whitespace() {
                s_idx += 1;
            }

            if after_dict[s_idx..].starts_with(b"stream") {
                let stream_keyword_end = s_idx + 6;
                let stream_start = if after_dict[stream_keyword_end..].starts_with(b"\r\n") {
                    stream_keyword_end + 2
                } else if after_dict[stream_keyword_end..].starts_with(b"\n") {
                    stream_keyword_end + 1
                } else {
                    stream_keyword_end
                };

                let endstream_pos = after_dict[stream_start..]
                    .windows(9)
                    .position(|w| w == b"endstream");

                if let Some(es) = endstream_pos {
                    let stream_content = &after_dict[stream_start..stream_start + es];
                    let stream_obj = lopdf::Stream::new(dict, stream_content.to_vec());
                    return Some((Object::Stream(stream_obj), i + pos + stream_start + es + 9));
                }
            }

            return Some((Object::Dictionary(dict), i + pos));
        }
    }

    // Number or boolean or array fallback
    None
}

fn parse_simple_dict_entries(dict_str: &str, dict: &mut Dictionary) {
    let words = dict_str
        .replace("<<", " ")
        .replace(">>", " ")
        .replace('/', " /");

    let tokens: Vec<&str> = words.split_whitespace().collect();
    let mut idx = 0;

    while idx + 1 < tokens.len() {
        let key = tokens[idx];
        let val = tokens[idx + 1];

        if let Some(clean_key) = key.strip_prefix('/') {
            if val.starts_with('/') {
                let clean_val = val.strip_prefix('/').unwrap_or(val);
                dict.set(
                    clean_key.as_bytes().to_vec(),
                    Object::Name(clean_val.as_bytes().to_vec()),
                );
                idx += 2;
                continue;
            } else if let Ok(n) = val.parse::<i64>() {
                // Check if reference (n 0 R)
                if idx + 2 < tokens.len() && tokens[idx + 2] == "R" {
                    dict.set(
                        clean_key.as_bytes().to_vec(),
                        Object::Reference((n as u32, 0)),
                    );
                    idx += 3;
                    continue;
                } else if idx + 3 < tokens.len() && tokens[idx + 3] == "R" {
                    let r#gen = tokens[idx + 2].parse::<u16>().unwrap_or(0);
                    dict.set(
                        clean_key.as_bytes().to_vec(),
                        Object::Reference((n as u32, r#gen)),
                    );
                    idx += 4;
                    continue;
                } else {
                    dict.set(clean_key.as_bytes().to_vec(), Object::Integer(n));
                    idx += 2;
                    continue;
                }
            }
        }
        idx += 1;
    }
}
