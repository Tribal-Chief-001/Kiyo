//! EPUB 2.0.1 and EPUB 3.3 reflowable document engine.

use anyhow::{Result, anyhow};
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use crate::universal::*;
use crate::zip_container::ZipArchive;

/// A chapter or section entry in an EPUB spine.
#[derive(Debug, Clone, PartialEq)]
pub struct EpubChapter {
    pub id: String,
    pub href: String,
    pub title: Option<String>,
}

/// EPUB document model backed by zero-copy memory mapping.
pub struct EpubDocument {
    pub file_size: u64,
    pub mmap: Arc<Mmap>,
    pub metadata: DocumentMetadata,
    pub opf_dir: String,
    pub chapters: Vec<EpubChapter>,
    pub manifest: HashMap<String, String>, // id -> href
    pub toc_tree: Vec<TocNode>,
}

impl EpubDocument {
    /// Opens and parses an EPUB archive package from disk.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())?;
        let file_size = file.metadata()?.len();
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };

        let archive = ZipArchive::parse(&mmap)?;

        // 1. Parse META-INF/container.xml
        let container_bytes = archive
            .read_file("META-INF/container.xml")
            .map_err(|_| anyhow!("Invalid EPUB: META-INF/container.xml not found in archive"))?;
        let container_str = String::from_utf8_lossy(&container_bytes);
        let opf_path = extract_attr_value(&container_str, "rootfile", "full-path")
            .ok_or_else(|| anyhow!("Rootfile full-path not found in container.xml"))?;

        let opf_dir = Path::new(&opf_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

        // 2. Parse OPF file
        let opf_bytes = archive.read_file(&opf_path)?;
        let opf_str = String::from_utf8_lossy(&opf_bytes);

        let title = extract_xml_tag(&opf_str, "dc:title")
            .or_else(|| extract_xml_tag(&opf_str, "title"))
            .or_else(|| {
                path.as_ref()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            });
        let author = extract_xml_tag(&opf_str, "dc:creator")
            .or_else(|| extract_xml_tag(&opf_str, "creator"));
        let subject = extract_xml_tag(&opf_str, "dc:description")
            .or_else(|| extract_xml_tag(&opf_str, "description"));
        let creation_date = extract_xml_tag(&opf_str, "dc:date");

        // 3. Parse manifest
        let mut manifest = HashMap::new();
        let mut toc_href = None;
        let mut pos = 0;

        while let Some(idx) = opf_str[pos..].find("<item") {
            let abs_start = pos + idx;
            let end = match opf_str[abs_start..].find('>') {
                Some(e) => abs_start + e + 1,
                None => break,
            };
            let item_str = &opf_str[abs_start..end];
            if item_str.starts_with("<item ")
                || item_str.starts_with("<item\t")
                || item_str.starts_with("<item\n")
            {
                if let Some(id) = extract_attr_value(item_str, "item", "id")
                    && let Some(href) = extract_attr_value(item_str, "item", "href")
                {
                    let full_href = if opf_dir.is_empty() {
                        href.clone()
                    } else {
                        format!("{}/{}", opf_dir, href)
                    };

                    let media_type =
                        extract_attr_value(item_str, "item", "media-type").unwrap_or_default();
                    if media_type == "application/x-dtbncx+xml"
                        || id == "ncx"
                        || href.ends_with(".ncx")
                    {
                        toc_href = Some(full_href.clone());
                    }

                    manifest.insert(id, full_href);
                }
            }
            pos = end;
        }

        // 4. Parse spine
        let mut chapters = Vec::new();
        pos = 0;
        while let Some(idx) = opf_str[pos..].find("<itemref") {
            let abs_start = pos + idx;
            let end = match opf_str[abs_start..].find('>') {
                Some(e) => abs_start + e + 1,
                None => break,
            };
            let itemref_str = &opf_str[abs_start..end];
            if let Some(idref) = extract_attr_value(itemref_str, "itemref", "idref") {
                if let Some(href) = manifest.get(&idref) {
                    chapters.push(EpubChapter {
                        id: idref.clone(),
                        href: href.clone(),
                        title: None,
                    });
                }
            }
            pos = end;
        }

        // 5. Parse TOC if present
        let mut toc_tree = Vec::new();
        if let Some(ref toc_path) = toc_href {
            if let Ok(toc_bytes) = archive.read_file(toc_path) {
                let toc_str = String::from_utf8_lossy(&toc_bytes);
                toc_tree = parse_ncx_toc(&toc_str, &chapters);
            }
        }

        if toc_tree.is_empty() {
            // Synthesize TOC from chapters
            for (i, ch) in chapters.iter().enumerate() {
                let name = Path::new(&ch.href)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Chapter")
                    .to_string();
                toc_tree.push(TocNode {
                    title: format!("Chapter {} ({})", i + 1, name),
                    target_page: i as u32,
                    target_x: None,
                    target_y: None,
                    children: Vec::new(),
                });
            }
        }

        let metadata = DocumentMetadata {
            title,
            author,
            subject,
            creation_date,
            page_count: chapters.len().max(1) as u32,
            format: Some(DocumentFormat::Epub),
            reading_direction: Some(ReadingDirection::LeftToRight),
            ..Default::default()
        };

        Ok(Self {
            file_size,
            mmap: Arc::new(mmap),
            metadata,
            opf_dir,
            chapters,
            manifest,
            toc_tree,
        })
    }

    /// Read raw XHTML bytes for a specific chapter index.
    pub fn read_chapter_bytes(&self, chapter_index: usize) -> Result<Vec<u8>> {
        let ch = self
            .chapters
            .get(chapter_index)
            .ok_or_else(|| anyhow!("Chapter index {} out of range", chapter_index))?;

        let archive = ZipArchive::parse(&self.mmap)?;
        archive.read_file(&ch.href)
    }
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    if let Some(start_idx) = xml.find(&open_tag) {
        let start = start_idx + open_tag.len();
        let end = xml[start..].find(&close_tag)? + start;
        return Some(xml[start..end].trim().to_string());
    }
    // Tag with attributes: <tag attr="...">
    let prefix = format!("<{} ", tag);
    if let Some(p_idx) = xml.find(&prefix) {
        let tag_close = xml[p_idx..].find('>')? + p_idx;
        let start = tag_close + 1;
        let end = xml[start..].find(&close_tag)? + start;
        return Some(xml[start..end].trim().to_string());
    }
    None
}

fn extract_attr_value(xml_line: &str, _tag_prefix: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    if let Some(idx) = xml_line.find(&pattern) {
        let start = idx + pattern.len();
        let end = xml_line[start..].find('"')? + start;
        return Some(xml_line[start..end].to_string());
    }
    let pattern_single = format!("{}='", attr);
    if let Some(idx) = xml_line.find(&pattern_single) {
        let start = idx + pattern_single.len();
        let end = xml_line[start..].find('\'')? + start;
        return Some(xml_line[start..end].to_string());
    }
    None
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script_or_style = false;

    let lower = html.to_lowercase();
    for (i, c) in html.chars().enumerate() {
        if c == '<' {
            in_tag = true;
            if lower[i..].starts_with("<script") || lower[i..].starts_with("<style") {
                in_script_or_style = true;
            } else if lower[i..].starts_with("</script") || lower[i..].starts_with("</style") {
                in_script_or_style = false;
            }
        } else if c == '>' {
            in_tag = false;
            out.push(' ');
        } else if !in_tag && !in_script_or_style {
            out.push(c);
        }
    }

    // Collapse multiple whitespaces
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_ncx_toc(ncx_xml: &str, chapters: &[EpubChapter]) -> Vec<TocNode> {
    let mut nodes = Vec::new();
    let mut pos = 0;

    while let Some(np_start) = ncx_xml[pos..].find("<navPoint") {
        let abs_np = pos + np_start;
        let np_end = match ncx_xml[abs_np..].find("</navPoint>") {
            Some(e) => abs_np + e + 11,
            None => break,
        };

        let chunk = &ncx_xml[abs_np..np_end];
        let label = extract_xml_tag(chunk, "text").unwrap_or_else(|| "Section".to_string());
        let src = extract_attr_value(chunk, "content", "src").unwrap_or_default();

        let clean_src = src.split('#').next().unwrap_or(&src);
        let target_page = chapters
            .iter()
            .position(|c| c.href.ends_with(clean_src))
            .unwrap_or(nodes.len()) as u32;

        nodes.push(TocNode {
            title: label,
            target_page,
            target_x: None,
            target_y: None,
            children: Vec::new(),
        });

        pos = np_end;
    }

    nodes
}

impl UniversalDocument for EpubDocument {
    fn format(&self) -> DocumentFormat {
        DocumentFormat::Epub
    }

    fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    fn page_count(&self) -> u32 {
        self.chapters.len().max(1) as u32
    }

    fn page_metrics(&self, _page_index: u32) -> Result<PageMetrics> {
        // Standard reflow e-reader virtual page metrics (600 x 800 pt)
        Ok(PageMetrics::new(600.0, 800.0))
    }

    fn table_of_contents(&self) -> Result<Vec<TocNode>> {
        Ok(self.toc_tree.clone())
    }

    fn extract_text(&self, page_index: u32) -> Result<String> {
        let bytes = self.read_chapter_bytes(page_index as usize)?;
        let html = String::from_utf8_lossy(&bytes);
        Ok(strip_html_tags(&html))
    }

    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>> {
        let mut results = Vec::new();
        if query.is_empty() {
            return Ok(results);
        }

        let query_norm = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        for (idx, _) in self.chapters.iter().enumerate() {
            if let Ok(text) = self.extract_text(idx as u32) {
                let text_cmp = if case_sensitive {
                    text.clone()
                } else {
                    text.to_lowercase()
                };

                for (char_idx, _) in text_cmp.match_indices(&query_norm) {
                    let start = char_idx.saturating_sub(40);
                    let end = (char_idx + query_norm.len() + 40).min(text.len());
                    let snippet = text[start..end].trim().to_string();

                    results.push(TextSearchResult {
                        page_index: idx as u32,
                        bounding_box: (50.0, 100.0, 550.0, 140.0),
                        snippet,
                    });
                }
            }
        }

        Ok(results)
    }

    fn as_reflowable(&self) -> Option<&dyn ReflowableDocument> {
        Some(self)
    }
}

impl ReflowableDocument for EpubDocument {
    fn layout_pages(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        font_size: f32,
        line_height: f32,
    ) -> Result<ReflowLayout> {
        let mut total_words = 0usize;
        for i in 0..self.chapters.len() {
            if let Ok(text) = self.extract_text(i as u32) {
                total_words += text.split_whitespace().count();
            }
        }

        let words_per_line = (viewport_width / (font_size * 0.55)).max(1.0) as usize;
        let lines_per_page = (viewport_height / (font_size * line_height)).max(1.0) as usize;
        let words_per_page = (words_per_line * lines_per_page).max(100);

        let computed_page_count = total_words
            .div_ceil(words_per_page)
            .max(self.chapters.len());

        Ok(ReflowLayout {
            computed_page_count,
            font_size,
            line_height,
            viewport_width,
            viewport_height,
        })
    }

    fn chapter_count(&self) -> usize {
        self.chapters.len()
    }

    fn chapter_title(&self, chapter_index: usize) -> Option<String> {
        self.chapters
            .get(chapter_index)
            .and_then(|c| c.title.clone())
            .or_else(|| {
                self.toc_tree
                    .iter()
                    .find(|n| n.target_page == chapter_index as u32)
                    .map(|n| n.title.clone())
            })
    }

    fn chapter_content(&self, chapter_index: usize) -> Result<String> {
        self.extract_text(chapter_index as u32)
    }
}
