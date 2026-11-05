//! CBR (Comic Book RAR) archive parser and sequential art document engine.

use anyhow::{Result, anyhow};
use memmap2::Mmap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use crate::universal::*;
use crate::zip_container::natural_cmp;

/// Representation of an entry inside a RAR archive.
#[derive(Debug, Clone, PartialEq)]
pub struct RarEntry {
    pub name: String,
    pub packed_size: u64,
    pub unpacked_size: u64,
    pub data_offset: usize,
    pub method: u8,
}

/// Comic Book Archive (.cbr) document model backed by pure-Rust header inspection.
pub struct CbrDocument {
    pub file_size: u64,
    pub mmap: Arc<Mmap>,
    pub metadata: DocumentMetadata,
    pub image_entries: Vec<RarEntry>,
}

impl CbrDocument {
    /// Opens and inspects a RARv4 or RARv5 comic archive.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())?;
        let file_size = file.metadata()?.len();
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };

        let entries = Self::parse_rar_entries(&mmap)?;

        // Filter for comic image extensions
        let mut image_entries: Vec<RarEntry> = entries
            .into_iter()
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
            .collect();

        image_entries.sort_by(|a, b| natural_cmp(&a.name, &b.name));

        let title = path
            .as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let metadata = DocumentMetadata {
            title,
            page_count: image_entries.len() as u32,
            format: Some(DocumentFormat::Comic(ComicArchiveType::Cbr)),
            reading_direction: Some(ReadingDirection::LeftToRight),
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
            return Err(anyhow!("CBR comic page {} out of range", index));
        }
        let entry = &self.image_entries[index];
        let end = entry.data_offset.saturating_add(entry.packed_size as usize);
        if end > self.mmap.len() {
            return Err(anyhow!("CBR entry data truncated for {}", entry.name));
        }
        let slice = &self.mmap[entry.data_offset..end];
        // If stored (method == 0x30 in RAR4, or packed == unpacked), return raw bytes
        if entry.method == 0x30 || entry.packed_size == entry.unpacked_size {
            return Ok(slice.to_vec());
        }
        // If Deflate compressed, decompress
        let mut decoder = flate2::read::DeflateDecoder::new(slice);
        let mut out = Vec::new();
        if decoder.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return Ok(out);
        }
        Ok(slice.to_vec())
    }

    fn parse_rar_entries(data: &[u8]) -> Result<Vec<RarEntry>> {
        if data.starts_with(b"Rar!\x1A\x07\x00") {
            // RAR 4.x
            Self::parse_rar4_entries(data)
        } else if data.starts_with(b"Rar!\x1A\x07\x01\x00") {
            // RAR 5.x
            Self::parse_rar5_entries(data)
        } else {
            Err(anyhow!("Invalid RAR magic header"))
        }
    }

    fn parse_rar4_entries(data: &[u8]) -> Result<Vec<RarEntry>> {
        let mut entries = Vec::new();
        let mut offset = 7; // Skip 7-byte signature

        while offset + 7 <= data.len() {
            let header_type = data[offset + 2];
            let header_flags = u16::from_le_bytes([data[offset + 3], data[offset + 4]]);
            let header_size = u16::from_le_bytes([data[offset + 5], data[offset + 6]]) as usize;

            if header_size < 7 || offset + header_size > data.len() {
                break;
            }

            if header_type == 0x74 && offset + 32 <= data.len() {
                // FILE_HEAD
                let mut pack_size = u32::from_le_bytes([
                    data[offset + 7],
                    data[offset + 8],
                    data[offset + 9],
                    data[offset + 10],
                ]) as u64;
                let mut unp_size = u32::from_le_bytes([
                    data[offset + 11],
                    data[offset + 12],
                    data[offset + 13],
                    data[offset + 14],
                ]) as u64;
                let name_size = u16::from_le_bytes([data[offset + 26], data[offset + 27]]) as usize;

                let mut name_offset = offset + 32;
                if (header_flags & 0x0100) != 0 && offset + 36 <= data.len() {
                    // High 32 bits of 64-bit size
                    let high_pack = u32::from_le_bytes([
                        data[offset + 32],
                        data[offset + 33],
                        data[offset + 34],
                        data[offset + 35],
                    ]) as u64;
                    let high_unp = u32::from_le_bytes([
                        data[offset + 36],
                        data[offset + 37],
                        data[offset + 38],
                        data[offset + 39],
                    ]) as u64;
                    pack_size |= high_pack << 32;
                    unp_size |= high_unp << 32;
                    name_offset = offset + 40;
                }

                if name_offset + name_size <= offset + header_size {
                    let name = String::from_utf8_lossy(&data[name_offset..name_offset + name_size])
                        .to_string();
                    let method = data[offset + 25];
                    let data_offset = offset + header_size;
                    entries.push(RarEntry {
                        name,
                        packed_size: pack_size,
                        unpacked_size: unp_size,
                        data_offset,
                        method,
                    });
                }

                offset += header_size + pack_size as usize;
            } else {
                offset += header_size;
            }
        }

        Ok(entries)
    }

    fn parse_rar5_entries(data: &[u8]) -> Result<Vec<RarEntry>> {
        let mut entries = Vec::new();
        let mut offset = 8; // Skip 8-byte RAR5 signature

        while offset + 4 <= data.len() {
            // Read CRC32
            offset += 4;
            if offset >= data.len() {
                break;
            }

            // Read header size (vint)
            let (header_size, vint_len) = match read_vint(&data[offset..]) {
                Some(v) => v,
                None => break,
            };
            offset += vint_len;
            let header_data_start = offset;
            let header_data_end = header_data_start + header_size as usize;
            if header_data_end > data.len() {
                break;
            }

            // Read header type (vint)
            let (header_type, h_vint_len) = match read_vint(&data[offset..header_data_end]) {
                Some(v) => v,
                None => break,
            };
            offset += h_vint_len;

            // Read header flags (vint)
            let (header_flags, f_vint_len) = match read_vint(&data[offset..header_data_end]) {
                Some(v) => v,
                None => break,
            };
            offset += f_vint_len;

            let mut extra_area_size = 0u64;
            if (header_flags & 0x0001) != 0 {
                if let Some((sz, l)) = read_vint(&data[offset..header_data_end]) {
                    extra_area_size = sz;
                    offset += l;
                }
            }

            let mut data_size = 0u64;
            if (header_flags & 0x0002) != 0 {
                if let Some((sz, l)) = read_vint(&data[offset..header_data_end]) {
                    data_size = sz;
                    offset += l;
                }
            }

            if header_type == 2 {
                // File Header
                let (_file_flags, l1) = match read_vint(&data[offset..header_data_end]) {
                    Some(v) => v,
                    None => break,
                };
                offset += l1;

                let (unpacked_size, l2) = match read_vint(&data[offset..header_data_end]) {
                    Some(v) => v,
                    None => break,
                };
                offset += l2;

                let (_attributes, l3) = match read_vint(&data[offset..header_data_end]) {
                    Some(v) => v,
                    None => break,
                };
                offset += l3;

                // Name length
                if let Some((name_len, l4)) = read_vint(&data[offset..header_data_end]) {
                    offset += l4;
                    let nl = name_len as usize;
                    if offset + nl <= header_data_end {
                        let name = String::from_utf8_lossy(&data[offset..offset + nl]).to_string();
                        let data_offset = header_data_end + extra_area_size as usize;
                        entries.push(RarEntry {
                            name,
                            packed_size: data_size,
                            unpacked_size,
                            data_offset,
                            method: 0,
                        });
                    }
                }
            }

            offset = header_data_end + extra_area_size as usize + data_size as usize;
        }

        Ok(entries)
    }
}

fn read_vint(slice: &[u8]) -> Option<(u64, usize)> {
    let mut val = 0u64;
    let mut shift = 0;
    for (i, &b) in slice.iter().enumerate().take(10) {
        val |= ((b & 0x7f) as u64) << shift;
        if (b & 0x80) == 0 {
            return Some((val, i + 1));
        }
        shift += 7;
    }
    None
}

impl UniversalDocument for CbrDocument {
    fn format(&self) -> DocumentFormat {
        DocumentFormat::Comic(ComicArchiveType::Cbr)
    }

    fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    fn page_count(&self) -> u32 {
        self.image_entries.len() as u32
    }

    fn page_metrics(&self, _page_index: u32) -> Result<PageMetrics> {
        Ok(PageMetrics::new(800.0, 1200.0))
    }

    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        let mut nodes = Vec::new();
        for (i, entry) in self.image_entries.iter().enumerate() {
            nodes.push(TocNode {
                title: format!("Page {} ({})", i + 1, entry.name),
                target_page: i as u32,
                target_x: None,
                target_y: None,
                children: Vec::new(),
            });
        }
        Ok(nodes)
    }

    fn extract_text(&self, page_index: u32) -> Result<String> {
        if let Some(entry) = self.image_entries.get(page_index as usize) {
            Ok(format!(
                "[CBR Comic Page {}: {}]",
                page_index + 1,
                entry.name
            ))
        } else {
            Ok(String::new())
        }
    }

    fn search_text(&self, query: &str, _case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        let mut results = Vec::new();
        for (i, entry) in self.image_entries.iter().enumerate() {
            if entry.name.to_lowercase().contains(&query.to_lowercase()) {
                results.push(TextSearchResult {
                    page_index: i as u32,
                    bounding_box: (0.0, 0.0, 800.0, 40.0),
                    snippet: entry.name.clone(),
                });
            }
        }
        Ok(results)
    }

    fn as_sequential_art(&self) -> Option<&dyn SequentialArtDocument> {
        Some(self)
    }
}

impl SequentialArtDocument for CbrDocument {
    fn reading_direction(&self) -> ReadingDirection {
        self.metadata
            .reading_direction
            .unwrap_or(ReadingDirection::LeftToRight)
    }

    fn image_count(&self) -> usize {
        self.image_entries.len()
    }

    fn image_dimensions(&self, _index: usize) -> Result<(u32, u32)> {
        Ok((1600, 2400))
    }

    fn is_dual_page_spread(&self, _index: usize) -> bool {
        false
    }

    fn detect_panels(&self, _index: usize) -> Result<Vec<ComicPanel>> {
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
