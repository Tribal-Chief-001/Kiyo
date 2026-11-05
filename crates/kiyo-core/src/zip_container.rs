//! Zero-copy, high-resilience pure-Rust ZIP container parser with corrupt EOCD salvage.

use anyhow::{Result, anyhow};
use flate2::read::DeflateDecoder;
use std::collections::HashMap;
use std::io::Read;

/// Representation of a single file entry within a ZIP archive.
#[derive(Debug, Clone, PartialEq)]
pub struct ZipEntry {
    pub name: String,
    pub compression_method: u16,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub local_header_offset: u64,
    pub data_offset: u64,
}

/// Zero-copy read-only ZIP archive view backed by a memory slice.
pub struct ZipArchive<'a> {
    data: &'a [u8],
    entries: Vec<ZipEntry>,
    name_map: HashMap<String, usize>,
}

impl<'a> ZipArchive<'a> {
    /// Parses a ZIP archive from a memory slice.
    ///
    /// First attempts standard Central Directory parsing via End of Central Directory (EOCD).
    /// If EOCD is damaged, falls back to a linear recovery scan for all local file headers (`PK\x03\x04`).
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 22 {
            return Err(anyhow!("Archive data too short for ZIP container"));
        }

        // 1. Try standard EOCD parsing
        if let Ok((entries, name_map)) = Self::parse_central_directory(data) {
            return Ok(Self {
                data,
                entries,
                name_map,
            });
        }

        // 2. Corrupt EOCD salvage: Linear scan for local headers (PK\x03\x04)
        let (entries, name_map) = Self::salvage_linear_scan(data)?;
        Ok(Self {
            data,
            entries,
            name_map,
        })
    }

    /// Access all file entries in the archive.
    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    /// Locate an entry by exact path name.
    pub fn get_entry(&self, name: &str) -> Option<&ZipEntry> {
        self.name_map.get(name).map(|&idx| &self.entries[idx])
    }

    /// Decompresses and extracts the byte payload for a specific entry.
    pub fn read_entry(&self, entry: &ZipEntry) -> Result<Vec<u8>> {
        let start = entry.data_offset as usize;
        let end = start + entry.compressed_size as usize;
        if end > self.data.len() {
            return Err(anyhow!(
                "Corrupt ZIP entry {}: data offset {} exceeds buffer length {}",
                entry.name,
                end,
                self.data.len()
            ));
        }

        let slice = &self.data[start..end];
        match entry.compression_method {
            0 => {
                // Stored (no compression)
                Ok(slice.to_vec())
            }
            8 => {
                // Deflated
                let mut decoder = DeflateDecoder::new(slice);
                let mut out = Vec::with_capacity(entry.uncompressed_size as usize);
                decoder.read_to_end(&mut out)?;
                Ok(out)
            }
            other => Err(anyhow!(
                "Unsupported ZIP compression method {} for entry {}",
                other,
                entry.name
            )),
        }
    }

    /// Decompresses and extracts the byte payload for an entry by name.
    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let entry = self
            .get_entry(name)
            .ok_or_else(|| anyhow!("Entry {:?} not found in ZIP archive", name))?;
        self.read_entry(entry)
    }

    fn parse_central_directory(data: &'a [u8]) -> Result<(Vec<ZipEntry>, HashMap<String, usize>)> {
        // Search backwards up to 65557 bytes for EOCD signature PK\x05\x06
        let max_search = (data.len().saturating_sub(22)).min(65557);
        let search_start = data.len().saturating_sub(max_search + 22);
        let mut eocd_pos = None;

        for i in (search_start..=data.len() - 22).rev() {
            if &data[i..i + 4] == b"PK\x05\x06" {
                eocd_pos = Some(i);
                break;
            }
        }

        let eocd_pos = eocd_pos.ok_or_else(|| anyhow!("EOCD signature not found"))?;
        let total_entries = u16::from_le_bytes([data[eocd_pos + 10], data[eocd_pos + 11]]) as usize;
        let cd_size = u32::from_le_bytes([
            data[eocd_pos + 12],
            data[eocd_pos + 13],
            data[eocd_pos + 14],
            data[eocd_pos + 15],
        ]) as usize;
        let cd_offset = u32::from_le_bytes([
            data[eocd_pos + 16],
            data[eocd_pos + 17],
            data[eocd_pos + 18],
            data[eocd_pos + 19],
        ]) as usize;

        if cd_offset + cd_size > data.len() {
            return Err(anyhow!("Central Directory exceeds file bounds"));
        }

        let mut entries = Vec::with_capacity(total_entries);
        let mut name_map = HashMap::with_capacity(total_entries);
        let mut pos = cd_offset;

        while pos + 46 <= cd_offset + cd_size {
            if &data[pos..pos + 4] != b"PK\x01\x02" {
                break;
            }

            let compression_method = u16::from_le_bytes([data[pos + 10], data[pos + 11]]);
            let compressed_size = u32::from_le_bytes([
                data[pos + 20],
                data[pos + 21],
                data[pos + 22],
                data[pos + 23],
            ]) as u64;
            let uncompressed_size = u32::from_le_bytes([
                data[pos + 24],
                data[pos + 25],
                data[pos + 26],
                data[pos + 27],
            ]) as u64;
            let filename_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
            let extra_len = u16::from_le_bytes([data[pos + 30], data[pos + 31]]) as usize;
            let comment_len = u16::from_le_bytes([data[pos + 32], data[pos + 33]]) as usize;
            let local_header_offset = u32::from_le_bytes([
                data[pos + 42],
                data[pos + 43],
                data[pos + 44],
                data[pos + 45],
            ]) as u64;

            let fn_start = pos + 46;
            let fn_end = fn_start + filename_len;
            if fn_end > data.len() {
                break;
            }

            let name = String::from_utf8_lossy(&data[fn_start..fn_end]).to_string();

            // Compute data offset from local header
            let lo = local_header_offset as usize;
            let data_offset = if lo + 30 <= data.len() && &data[lo..lo + 4] == b"PK\x03\x04" {
                let l_fn_len = u16::from_le_bytes([data[lo + 26], data[lo + 27]]) as usize;
                let l_extra_len = u16::from_le_bytes([data[lo + 28], data[lo + 29]]) as usize;
                (lo + 30 + l_fn_len + l_extra_len) as u64
            } else {
                (lo + 30 + filename_len + extra_len) as u64
            };

            let entry = ZipEntry {
                name: name.clone(),
                compression_method,
                compressed_size,
                uncompressed_size,
                local_header_offset,
                data_offset,
            };

            let idx = entries.len();
            name_map.insert(name, idx);
            entries.push(entry);

            pos = fn_end + extra_len + comment_len;
        }

        Ok((entries, name_map))
    }

    fn salvage_linear_scan(data: &'a [u8]) -> Result<(Vec<ZipEntry>, HashMap<String, usize>)> {
        let mut entries = Vec::new();
        let mut name_map = HashMap::new();
        let mut i = 0;

        while i + 30 <= data.len() {
            if &data[i..i + 4] == b"PK\x03\x04" {
                let compression_method = u16::from_le_bytes([data[i + 8], data[i + 9]]);
                let compressed_size =
                    u32::from_le_bytes([data[i + 18], data[i + 19], data[i + 20], data[i + 21]])
                        as u64;
                let uncompressed_size =
                    u32::from_le_bytes([data[i + 22], data[i + 23], data[i + 24], data[i + 25]])
                        as u64;
                let fn_len = u16::from_le_bytes([data[i + 26], data[i + 27]]) as usize;
                let extra_len = u16::from_le_bytes([data[i + 28], data[i + 29]]) as usize;

                let fn_start = i + 30;
                let fn_end = fn_start + fn_len;
                if fn_end <= data.len() {
                    let name = String::from_utf8_lossy(&data[fn_start..fn_end]).to_string();
                    let data_offset = (fn_end + extra_len) as u64;

                    let entry = ZipEntry {
                        name: name.clone(),
                        compression_method,
                        compressed_size,
                        uncompressed_size,
                        local_header_offset: i as u64,
                        data_offset,
                    };

                    let idx = entries.len();
                    name_map.insert(name, idx);
                    entries.push(entry);

                    i = (data_offset + compressed_size) as usize;
                    continue;
                }
            }
            i += 1;
        }

        if entries.is_empty() {
            return Err(anyhow!(
                "No valid ZIP local file headers found during linear salvage scan"
            ));
        }

        Ok((entries, name_map))
    }
}

/// Natural sort comparator for filenames (e.g. "page2.jpg" < "page10.jpg").
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    let mut a_num = 0u64;
                    while let Some(c) = a_chars.peek() {
                        if let Some(digit) = c.to_digit(10) {
                            a_num = a_num.saturating_mul(10).saturating_add(digit as u64);
                            a_chars.next();
                        } else {
                            break;
                        }
                    }

                    let mut b_num = 0u64;
                    while let Some(c) = b_chars.peek() {
                        if let Some(digit) = c.to_digit(10) {
                            b_num = b_num.saturating_mul(10).saturating_add(digit as u64);
                            b_chars.next();
                        } else {
                            break;
                        }
                    }

                    match a_num.cmp(&b_num) {
                        std::cmp::Ordering::Equal => continue,
                        ord => return ord,
                    }
                } else {
                    let c1 = a_chars.next().unwrap();
                    let c2 = b_chars.next().unwrap();
                    let c1_lower = c1.to_ascii_lowercase();
                    let c2_lower = c2.to_ascii_lowercase();
                    match c1_lower.cmp(&c2_lower) {
                        std::cmp::Ordering::Equal => {
                            if c1 != c2 {
                                return c1.cmp(&c2);
                            }
                        }
                        ord => return ord,
                    }
                }
            }
        }
    }
}
