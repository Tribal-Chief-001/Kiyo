//! 4-Stage Corrupt Xref Salvage Pipeline with Compressed Object Stream (/ObjStm) Decompression.

use anyhow::{Result, anyhow};
use flate2::read::ZlibDecoder;
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
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

    // Stage 1.5: Lenient reconstruction via lopdf force_reconstruct
    let options = lopdf::LoadOptions {
        strict: false,
        force_reconstruct: true,
        ..Default::default()
    };
    if let Ok(doc) = Document::load_mem_with_options(bytes, options) {
        if doc.trailer.has(b"Root") && !doc.get_pages().is_empty() {
            return Ok((doc, SalvageStage::LinearStreamScan));
        }
    }

    // Stage 2: Linear stream search for latest valid xref or /XRef stream
    if let Ok(doc) = stage2_linear_stream_search(bytes) {
        if !doc.get_pages().is_empty() {
            return Ok((doc, SalvageStage::LinearStreamScan));
        }
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
            if i < bytes.len()
                && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\r' || bytes[i] == b'\n')
            {
                while i < bytes.len()
                    && (bytes[i] == b' '
                        || bytes[i] == b'\t'
                        || bytes[i] == b'\r'
                        || bytes[i] == b'\n')
                {
                    i += 1;
                }
                let mut num2 = 0u16;
                let gen_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    num2 = num2
                        .saturating_mul(10)
                        .saturating_add((bytes[i] - b'0') as u16);
                    i += 1;
                }

                while i < bytes.len()
                    && (bytes[i] == b' '
                        || bytes[i] == b'\t'
                        || bytes[i] == b'\r'
                        || bytes[i] == b'\n')
                {
                    i += 1;
                }

                if i > gen_start && i + 3 <= bytes.len() && &bytes[i..i + 3] == b"obj" {
                    let obj_id = (num1, num2);
                    i += 3;

                    // Parse the object payload starting here
                    let mut parser = PdfParser::new(&bytes[i..]);
                    if let Some(obj) = parser.parse_object() {
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

                        let parsed_len = parser.pos;
                        raw_objects.insert(obj_id, obj);
                        i += parsed_len;
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
            // First try lopdf's official ObjectStream parser
            if let Ok(obj_stream) = lopdf::ObjectStream::new(stream) {
                if !obj_stream.objects.is_empty() {
                    had_objstm = true;
                    for (inner_id, inner_obj) in obj_stream.objects {
                        if let Object::Dictionary(ref dict) = inner_obj {
                            if let Ok(t) = dict.get(b"Type").and_then(|o| o.as_name()) {
                                if t == b"Catalog" && root_id.is_none() {
                                    root_id = Some(inner_id);
                                }
                            }
                        }
                        raw_objects.insert(inner_id, inner_obj);
                    }
                    continue;
                }
            }

            // Fallback manual decompression and parsing
            let n_count = stream.dict.get(b"N").and_then(|o| o.as_i64()).unwrap_or(0) as usize;
            let first_offset = stream
                .dict
                .get(b"First")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as usize;

            let decompressed = decompress_stream_bytes(stream);
            if let Ok(decompressed) = decompressed {
                if first_offset <= decompressed.len() && n_count > 0 {
                    had_objstm = true;
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
                            let mut inner_parser = PdfParser::new(obj_slice);
                            if let Some(inner_obj) = inner_parser.parse_object() {
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

    // Identify all Page objects and Pages parent
    let mut page_ids: Vec<ObjectId> = Vec::new();
    let mut pages_id: Option<ObjectId> = None;

    for (&id, obj) in &raw_objects {
        if let Object::Dictionary(dict) = obj {
            if let Ok(t) = dict.get(b"Type").and_then(|o| o.as_name()) {
                if t == b"Page" {
                    page_ids.push(id);
                } else if t == b"Pages" && pages_id.is_none() {
                    pages_id = Some(id);
                } else if t == b"Catalog" && root_id.is_none() {
                    root_id = Some(id);
                }
            }
        }
    }

    page_ids.sort();

    // Max ID in raw_objects
    let mut max_id_num = raw_objects.keys().map(|k| k.0).max().unwrap_or(0);

    // If no Pages object exists, synthesize one
    let target_pages_id = if let Some(p_id) = pages_id {
        p_id
    } else {
        max_id_num += 1;
        let new_pages_id = (max_id_num, 0);
        let mut p_dict = Dictionary::new();
        p_dict.set("Type", Object::Name(b"Pages".to_vec()));
        p_dict.set("Count", Object::Integer(page_ids.len() as i64));
        p_dict.set(
            "Kids",
            Object::Array(page_ids.iter().map(|&pid| Object::Reference(pid)).collect()),
        );
        raw_objects.insert(new_pages_id, Object::Dictionary(p_dict));
        new_pages_id
    };

    // Repair Page objects: ensure Parent points to target_pages_id, MediaBox is set
    for &pid in &page_ids {
        if let Some(Object::Dictionary(p_dict)) = raw_objects.get_mut(&pid) {
            if !p_dict.has(b"Parent") {
                p_dict.set("Parent", Object::Reference(target_pages_id));
            }
            if !p_dict.has(b"MediaBox") {
                p_dict.set(
                    "MediaBox",
                    Object::Array(vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Integer(595),
                        Object::Integer(842),
                    ]),
                );
            }
        }
    }

    // Repair Pages object: ensure Kids contains all page_ids
    if let Some(Object::Dictionary(pages_dict)) = raw_objects.get_mut(&target_pages_id) {
        let has_kids = pages_dict
            .get(b"Kids")
            .and_then(|o| o.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if !has_kids && !page_ids.is_empty() {
            pages_dict.set(
                "Kids",
                Object::Array(page_ids.iter().map(|&pid| Object::Reference(pid)).collect()),
            );
            pages_dict.set("Count", Object::Integer(page_ids.len() as i64));
        }
    }

    // Identify or synthesize Catalog / Root
    let final_root_id = if let Some(r_id) = root_id {
        if let Some(Object::Dictionary(cat_dict)) = raw_objects.get_mut(&r_id) {
            if !cat_dict.has(b"Pages") {
                cat_dict.set("Pages", Object::Reference(target_pages_id));
            }
        }
        r_id
    } else {
        max_id_num += 1;
        let new_cat_id = (max_id_num, 0);
        let mut cat_dict = Dictionary::new();
        cat_dict.set("Type", Object::Name(b"Catalog".to_vec()));
        cat_dict.set("Pages", Object::Reference(target_pages_id));
        raw_objects.insert(new_cat_id, Object::Dictionary(cat_dict));
        new_cat_id
    };

    // Assemble salvaged document
    let mut doc = Document::with_version("1.7");
    doc.objects = raw_objects;
    doc.trailer.set("Root", final_root_id);

    if let Some(i_id) = info_id {
        doc.trailer.set("Info", i_id);
    }

    doc.trailer.set("Size", doc.objects.len() as i64 + 1);
    doc.max_id = max_id_num;

    Ok((doc, had_objstm))
}

fn decompress_stream_bytes(stream: &lopdf::Stream) -> Result<Vec<u8>> {
    let filter = stream.dict.get(b"Filter").and_then(|o| o.as_name()).ok();
    if filter == Some(b"FlateDecode") || filter.is_none() {
        let mut decoder = ZlibDecoder::new(&stream.content[..]);
        let mut out = Vec::new();
        if decoder.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return Ok(out);
        }
        let mut raw_decoder = flate2::read::DeflateDecoder::new(&stream.content[..]);
        let mut raw_out = Vec::new();
        if raw_decoder.read_to_end(&mut raw_out).is_ok() && !raw_out.is_empty() {
            return Ok(raw_out);
        }
    }
    Ok(stream.content.clone())
}

/// Recursive-descent PDF object parser for raw byte buffers.
pub struct PdfParser<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> PdfParser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == 0 {
                self.pos += 1;
            } else if b == b'%' {
                // Comment runs to end of line
                self.pos += 1;
                while self.pos < self.data.len()
                    && self.data[self.pos] != b'\r'
                    && self.data[self.pos] != b'\n'
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    pub fn parse_object(&mut self) -> Option<Object> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.data.len() {
            return None;
        }

        let b = self.data[self.pos];

        // Dictionary or Hex string
        if b == b'<' {
            if self.pos + 1 < self.data.len() && self.data[self.pos + 1] == b'<' {
                return self.parse_dictionary_or_stream();
            } else {
                return self.parse_hex_string();
            }
        }

        // Array
        if b == b'[' {
            return self.parse_array();
        }

        // Literal String
        if b == b'(' {
            return self.parse_literal_string();
        }

        // Name
        if b == b'/' {
            return self.parse_name();
        }

        // Boolean or Null
        if self.data[self.pos..].starts_with(b"true") {
            self.pos += 4;
            return Some(Object::Boolean(true));
        }
        if self.data[self.pos..].starts_with(b"false") {
            self.pos += 5;
            return Some(Object::Boolean(false));
        }
        if self.data[self.pos..].starts_with(b"null") {
            self.pos += 4;
            return Some(Object::Null);
        }

        // Number or Reference
        if b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.' {
            return self.parse_number_or_reference();
        }

        None
    }

    fn parse_dictionary_or_stream(&mut self) -> Option<Object> {
        self.pos += 2; // Skip '<<'
        let mut dict = Dictionary::new();

        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.data.len() {
                break;
            }
            if self.pos + 1 < self.data.len() && &self.data[self.pos..self.pos + 2] == b">>" {
                self.pos += 2;
                break;
            }

            // Key must be a Name
            if self.data[self.pos] != b'/' {
                // If corrupted or misplaced token, advance until next '/' or '>>'
                self.pos += 1;
                continue;
            }

            let key_obj = self.parse_name()?;
            let key_bytes = match key_obj {
                Object::Name(bytes) => bytes,
                _ => continue,
            };

            let val_obj = self.parse_object()?;
            dict.set(key_bytes, val_obj);
        }

        self.skip_whitespace_and_comments();

        // Check if followed by "stream"
        if self.data[self.pos..].starts_with(b"stream") {
            self.pos += 6;
            // Skip single CRLF or LF after stream keyword
            if self.pos < self.data.len() && self.data[self.pos] == b'\r' {
                self.pos += 1;
            }
            if self.pos < self.data.len() && self.data[self.pos] == b'\n' {
                self.pos += 1;
            }

            let stream_content_start = self.pos;
            // Check for endstream
            let endstream_idx = self.data[stream_content_start..]
                .windows(9)
                .position(|w| w == b"endstream");

            let stream_content = if let Some(offset) = endstream_idx {
                let content_slice = &self.data[stream_content_start..stream_content_start + offset];
                self.pos = stream_content_start + offset + 9;
                content_slice.to_vec()
            } else {
                let content_slice = &self.data[stream_content_start..];
                self.pos = self.data.len();
                content_slice.to_vec()
            };

            return Some(Object::Stream(lopdf::Stream::new(dict, stream_content)));
        }

        Some(Object::Dictionary(dict))
    }

    fn parse_array(&mut self) -> Option<Object> {
        self.pos += 1; // Skip '['
        let mut elements = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.data.len() {
                break;
            }
            if self.data[self.pos] == b']' {
                self.pos += 1;
                break;
            }

            if let Some(obj) = self.parse_object() {
                elements.push(obj);
            } else {
                self.pos += 1;
            }
        }

        Some(Object::Array(elements))
    }

    fn parse_literal_string(&mut self) -> Option<Object> {
        self.pos += 1; // Skip '('
        let mut depth = 1;
        let mut bytes = Vec::new();

        while self.pos < self.data.len() && depth > 0 {
            let b = self.data[self.pos];
            if b == b'\\' {
                self.pos += 1;
                if self.pos < self.data.len() {
                    let esc = self.data[self.pos];
                    match esc {
                        b'n' => bytes.push(b'\n'),
                        b'r' => bytes.push(b'\r'),
                        b't' => bytes.push(b'\t'),
                        b'b' => bytes.push(0x08),
                        b'f' => bytes.push(0x0C),
                        b'(' => bytes.push(b'('),
                        b')' => bytes.push(b')'),
                        b'\\' => bytes.push(b'\\'),
                        _ => bytes.push(esc),
                    }
                    self.pos += 1;
                }
            } else if b == b'(' {
                depth += 1;
                bytes.push(b);
                self.pos += 1;
            } else if b == b')' {
                depth -= 1;
                if depth > 0 {
                    bytes.push(b);
                }
                self.pos += 1;
            } else {
                bytes.push(b);
                self.pos += 1;
            }
        }

        Some(Object::String(bytes, StringFormat::Literal))
    }

    fn parse_hex_string(&mut self) -> Option<Object> {
        self.pos += 1; // Skip '<'
        let mut hex_chars = Vec::new();
        while self.pos < self.data.len() && self.data[self.pos] != b'>' {
            let b = self.data[self.pos];
            if b.is_ascii_hexdigit() {
                hex_chars.push(b);
            }
            self.pos += 1;
        }
        if self.pos < self.data.len() && self.data[self.pos] == b'>' {
            self.pos += 1;
        }

        let mut bytes = Vec::new();
        let mut idx = 0;
        while idx < hex_chars.len() {
            let h1 = hex_chars[idx];
            let h2 = if idx + 1 < hex_chars.len() {
                hex_chars[idx + 1]
            } else {
                b'0'
            };
            if let Ok(byte_val) =
                u8::from_str_radix(std::str::from_utf8(&[h1, h2]).unwrap_or("00"), 16)
            {
                bytes.push(byte_val);
            }
            idx += 2;
        }

        Some(Object::String(bytes, StringFormat::Hexadecimal))
    }

    fn parse_name(&mut self) -> Option<Object> {
        self.pos += 1; // Skip '/'
        let mut name = Vec::new();

        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if b.is_ascii_whitespace()
                || b == 0
                || b == b'/'
                || b == b'['
                || b == b']'
                || b == b'('
                || b == b')'
                || b == b'<'
                || b == b'>'
                || b == b'%'
            {
                break;
            }
            if b == b'#' && self.pos + 2 < self.data.len() {
                if let Ok(val) = u8::from_str_radix(
                    std::str::from_utf8(&self.data[self.pos + 1..self.pos + 3]).unwrap_or(""),
                    16,
                ) {
                    name.push(val);
                    self.pos += 3;
                    continue;
                }
            }
            name.push(b);
            self.pos += 1;
        }

        Some(Object::Name(name))
    }

    fn parse_number_or_reference(&mut self) -> Option<Object> {
        let start = self.pos;
        let mut is_float = false;

        if self.pos < self.data.len()
            && (self.data[self.pos] == b'+' || self.data[self.pos] == b'-')
        {
            self.pos += 1;
        }
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if b.is_ascii_digit() {
                self.pos += 1;
            } else if b == b'.' {
                is_float = true;
                self.pos += 1;
            } else {
                break;
            }
        }

        let num_str = std::str::from_utf8(&self.data[start..self.pos]).ok()?;

        if !is_float {
            if let Ok(num1) = num_str.parse::<u32>() {
                // Lookahead for generation number + 'R'
                let saved_pos = self.pos;
                self.skip_whitespace_and_comments();
                let gen_start = self.pos;
                while self.pos < self.data.len() && self.data[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                if self.pos > gen_start {
                    if let Ok(num2) = std::str::from_utf8(&self.data[gen_start..self.pos])
                        .unwrap_or("")
                        .parse::<u16>()
                    {
                        self.skip_whitespace_and_comments();
                        if self.pos < self.data.len() && self.data[self.pos] == b'R' {
                            // Check that next char after R is delimiter
                            let next = self.pos + 1;
                            if next >= self.data.len()
                                || self.data[next].is_ascii_whitespace()
                                || self.data[next] == 0
                                || self.data[next] == b'/'
                                || self.data[next] == b'['
                                || self.data[next] == b']'
                                || self.data[next] == b'<'
                                || self.data[next] == b'>'
                                || self.data[next] == b'('
                                || self.data[next] == b')'
                            {
                                self.pos += 1; // Skip 'R'
                                return Some(Object::Reference((num1, num2)));
                            }
                        }
                    }
                }
                self.pos = saved_pos;
                return Some(Object::Integer(num1 as i64));
            } else if let Ok(n) = num_str.parse::<i64>() {
                return Some(Object::Integer(n));
            }
        }

        if let Ok(f) = num_str.parse::<f32>() {
            Some(Object::Real(f))
        } else {
            None
        }
    }
}
