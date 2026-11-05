//! Polymorphic multi-format document model and format engine abstractions.
//!
//! Provides the `UniversalDocument` trait hierarchy spanning:
//! 1. Fixed-layout engines (PDF, XPS, DjVu, CHM)
//! 2. Reflowable e-book engines (EPUB, MOBI, AZW3, FB2, Markdown)
//! 3. Sequential art & comic engines (CBZ, CBR, CB7, CBT, Webtoon)
//! 4. Full workstation document manipulation (Acrobat-style mutations, Bates numbering, Destructive Redaction)

use anyhow::{Result, anyhow};
use std::path::Path;

/// Canonical classification of supported document formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentFormat {
    // Fixed-Layout
    Pdf,
    Xps,
    DjVu,
    Chm,

    // Reflowable E-Books
    Epub,
    Mobi,
    Azw3,
    Fb2,
    Markdown,

    // Sequential Art & Comics
    Comic(ComicArchiveType),
    Webtoon,
}

impl DocumentFormat {
    /// Categorizes the format into its high-level layout family.
    pub fn category(&self) -> FormatCategory {
        match self {
            Self::Pdf | Self::Xps | Self::DjVu | Self::Chm => FormatCategory::FixedLayout,
            Self::Epub | Self::Mobi | Self::Azw3 | Self::Fb2 | Self::Markdown => {
                FormatCategory::Reflowable
            }
            Self::Comic(_) | Self::Webtoon => FormatCategory::SequentialArt,
        }
    }

    /// Default recommended user interaction mode.
    pub fn default_reading_mode(&self) -> ReadingMode {
        match self.category() {
            FormatCategory::FixedLayout => ReadingMode::Academic,
            FormatCategory::Reflowable => ReadingMode::EReader,
            FormatCategory::SequentialArt => ReadingMode::ComicManga,
        }
    }

    /// Standard IANA MIME type for the format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Xps => "application/oxps",
            Self::DjVu => "image/vnd.djvu",
            Self::Chm => "application/vnd.ms-htmlhelp",
            Self::Epub => "application/epub+zip",
            Self::Mobi => "application/x-mobipocket-ebook",
            Self::Azw3 => "application/vnd.amazon.ebook",
            Self::Fb2 => "application/x-fictionbook+xml",
            Self::Markdown => "text/markdown",
            Self::Comic(ComicArchiveType::Cbz) => "application/vnd.comicbook+zip",
            Self::Comic(ComicArchiveType::Cbr) => "application/vnd.comicbook-rar",
            Self::Comic(ComicArchiveType::Cb7) => "application/x-cb7",
            Self::Comic(ComicArchiveType::Cbt) => "application/x-cbt",
            Self::Comic(ComicArchiveType::ImageDirectory) => "inode/directory",
            Self::Webtoon => "application/x-webtoon-ribbon",
        }
    }

    /// Typical file extensions associated with the format.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Pdf => &["pdf"],
            Self::Xps => &["xps", "oxps"],
            Self::DjVu => &["djvu", "djv"],
            Self::Chm => &["chm"],
            Self::Epub => &["epub"],
            Self::Mobi => &["mobi", "prc"],
            Self::Azw3 => &["azw3", "azw", "kf8"],
            Self::Fb2 => &["fb2", "fb2.zip"],
            Self::Markdown => &["md", "markdown", "typ"],
            Self::Comic(ComicArchiveType::Cbz) => &["cbz"],
            Self::Comic(ComicArchiveType::Cbr) => &["cbr"],
            Self::Comic(ComicArchiveType::Cb7) => &["cb7"],
            Self::Comic(ComicArchiveType::Cbt) => &["cbt"],
            Self::Comic(ComicArchiveType::ImageDirectory) => &[],
            Self::Webtoon => &["webtoon"],
        }
    }
}

/// High-level format categorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatCategory {
    /// Absolute coordinate pages with rigid bounds (PDF, XPS, DjVu, CHM).
    FixedLayout,
    /// Fluid layout reflowed dynamically into viewport pages (EPUB, MOBI, Markdown).
    Reflowable,
    /// Image collections and vertical continuous strips (CBZ, CBR, Webtoon).
    SequentialArt,
}

/// Application interaction mode tailored to specific reading paradigms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadingMode {
    /// Portals, dual reading ruler, SyncTeX bidirectional sync, citation popups.
    Academic,
    /// Fluid typography, customizable margins, lemma dictionary lookup, e-ink palettes.
    EReader,
    /// RTL manga flow, double-page spreads, panel guided view, binarization shaders.
    ComicManga,
    /// Full Acrobat Pro suite: 5-box geometry, redaction, Bates numbering, AcroForms.
    Workstation,
}

/// Archive and storage format for sequential comic art.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComicArchiveType {
    Cbz,
    Cbr,
    Cb7,
    Cbt,
    ImageDirectory,
}

/// Sequential art reading progression direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadingDirection {
    /// Standard Western comics and books (Left to Right).
    LeftToRight,
    /// Traditional Eastern Manga (Right to Left).
    RightToLeft,
    /// Continuous vertical strip for Webtoons and Manhwa.
    VerticalRibbon,
}

/// Comprehensive document metadata extracted across formats.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub page_count: u32,
    pub format: Option<DocumentFormat>,
    pub reading_direction: Option<ReadingDirection>,
    pub is_encrypted: bool,
    pub is_signed: bool,
}

/// 5-Box Geometry metrics per ISO 32000-1 §14.11.2 and display bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageMetrics {
    pub width: f32,
    pub height: f32,
    pub rotation_degrees: u16,
    pub dpi: f32,
    /// Physical medium boundary [min_x, min_y, max_x, max_y]
    pub media_box: (f32, f32, f32, f32),
    /// Visible display and clipping boundary
    pub crop_box: Option<(f32, f32, f32, f32)>,
    /// Production printing boundary with bleed margins
    pub bleed_box: Option<(f32, f32, f32, f32)>,
    /// Dimensions of finished cut page after trimming
    pub trim_box: Option<(f32, f32, f32, f32)>,
    /// Extent of page's meaningful content block
    pub art_box: Option<(f32, f32, f32, f32)>,
}

impl PageMetrics {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            rotation_degrees: 0,
            dpi: 72.0,
            media_box: (0.0, 0.0, width, height),
            crop_box: None,
            bleed_box: None,
            trim_box: None,
            art_box: None,
        }
    }

    /// Effective visible bounding box accounting for CropBox override.
    pub fn visible_box(&self) -> (f32, f32, f32, f32) {
        self.crop_box.unwrap_or(self.media_box)
    }

    /// Visible width based on visible bounding box.
    pub fn visible_width(&self) -> f32 {
        let b = self.visible_box();
        (b.2 - b.0).abs()
    }

    /// Visible height based on visible bounding box.
    pub fn visible_height(&self) -> f32 {
        let b = self.visible_box();
        (b.3 - b.1).abs()
    }

    /// Viewport dimensions swapping width and height when rotated 90 or 270 degrees.
    pub fn viewport_dimensions(&self) -> (f32, f32) {
        let w = self.visible_width();
        let h = self.visible_height();
        if self.rotation_degrees == 90 || self.rotation_degrees == 270 {
            (h, w)
        } else {
            (w, h)
        }
    }
}

/// Hierarchical Table of Contents node.
#[derive(Debug, Clone, PartialEq)]
pub struct TocNode {
    pub title: String,
    pub target_page: u32,
    pub target_x: Option<f32>,
    pub target_y: Option<f32>,
    pub children: Vec<TocNode>,
}

/// Hit result for text searches.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSearchResult {
    pub page_index: u32,
    pub bounding_box: (f32, f32, f32, f32),
    pub snippet: String,
}

/// Bates numbering configuration for legal discovery indexing inside `/Artifact`.
#[derive(Debug, Clone, PartialEq)]
pub struct BatesConfig {
    pub prefix: String,
    pub start_number: u64,
    pub digit_count: usize,
    pub suffix: String,
    pub position: BatesPosition,
    pub font_size: f32,
    pub embed_as_artifact: bool,
}

impl Default for BatesConfig {
    fn default() -> Self {
        Self {
            prefix: "CONFIDENTIAL-".to_string(),
            start_number: 1,
            digit_count: 6,
            suffix: "".to_string(),
            position: BatesPosition::BottomRight,
            font_size: 9.0,
            embed_as_artifact: true,
        }
    }
}

impl BatesConfig {
    /// Formats the Bates number string for a given sequence offset.
    pub fn format_number(&self, offset: u64) -> String {
        let num = self.start_number + offset;
        format!(
            "{}{:0width$}{}",
            self.prefix,
            num,
            self.suffix,
            width = self.digit_count
        )
    }
}

/// Screen positioning for Bates stamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatesPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Destructive redaction area descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionRegion {
    pub page_index: u32,
    pub rect: (f32, f32, f32, f32),
    pub overlay_text: Option<String>,
    pub fill_color: (u8, u8, u8),
}

/// Document page-tree mutation actions.
#[derive(Debug, Clone, PartialEq)]
pub enum PageMutationOp {
    Rotate {
        page_index: u32,
        degrees: i32,
    },
    Delete {
        page_index: u32,
    },
    Move {
        from_index: u32,
        to_index: u32,
    },
    InsertBlank {
        at_index: u32,
        width: f32,
        height: f32,
    },
    ApplyRedaction(RedactionRegion),
    ApplyBates(BatesConfig),
}

/// Reflow layout metrics computed by headless layout engine (Taffy / Typst).
#[derive(Debug, Clone, PartialEq)]
pub struct ReflowLayout {
    pub computed_page_count: usize,
    pub font_size: f32,
    pub line_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Individual comic panel detected via neural or contour segmentation.
#[derive(Debug, Clone, PartialEq)]
pub struct ComicPanel {
    pub panel_index: usize,
    pub bounds: (f32, f32, f32, f32),
    pub reading_order: usize,
}

/// Polymorphic base trait implemented by all document formats in Kiyo.
pub trait UniversalDocument: Send + Sync {
    /// Document format identifier.
    fn format(&self) -> DocumentFormat;

    /// Layout category (Fixed, Reflowable, or SequentialArt).
    fn category(&self) -> FormatCategory {
        self.format().category()
    }

    /// Read-only metadata summary.
    fn metadata(&self) -> &DocumentMetadata;

    /// Total number of pages or discrete panels.
    fn page_count(&self) -> u32;

    /// Geometric dimensions and 5-box prepress metrics for a page.
    fn page_metrics(&self, page_index: u32) -> Result<PageMetrics>;

    /// Hierarchical Table of Contents or outline tree.
    fn table_of_contents(&self) -> Result<Vec<TocNode>>;

    /// Plaintext content extraction for a given page.
    fn extract_text(&self, page_index: u32) -> Result<String>;

    /// Document-wide text search.
    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>>;

    /// Optional downcast to reflowable e-book capabilities.
    fn as_reflowable(&self) -> Option<&dyn ReflowableDocument> {
        None
    }

    /// Optional downcast to sequential art and comic capabilities.
    fn as_sequential_art(&self) -> Option<&dyn SequentialArtDocument> {
        None
    }

    /// Optional downcast to Acrobat-style in-place content editing.
    fn as_editable(&mut self) -> Option<&mut dyn EditableDocument> {
        None
    }
}

/// Specialized trait for reflowable e-books (EPUB, MOBI, Markdown).
pub trait ReflowableDocument: Send + Sync {
    /// Recomputes dynamic pagination based on font size and viewport bounds.
    fn layout_pages(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        font_size: f32,
        line_height: f32,
    ) -> Result<ReflowLayout>;

    /// Total structural chapters or sections.
    fn chapter_count(&self) -> usize;

    /// Section heading for chapter.
    fn chapter_title(&self, chapter_index: usize) -> Option<String>;

    /// Raw or Markdown formatted text for a specific chapter.
    fn chapter_content(&self, chapter_index: usize) -> Result<String>;
}

/// Specialized trait for comic books, manga, and vertical webtoon ribbons.
pub trait SequentialArtDocument: Send + Sync {
    /// Reading progression direction (LTR, RTL, or VerticalRibbon).
    fn reading_direction(&self) -> ReadingDirection;

    /// Total number of raster image frames.
    fn image_count(&self) -> usize;

    /// Pixel dimensions of a specific comic frame.
    fn image_dimensions(&self, index: usize) -> Result<(u32, u32)>;

    /// Indicates whether a given page is part of a 2-page landscape spread.
    fn is_dual_page_spread(&self, index: usize) -> bool;

    /// Extracts panel bounding boxes for guided panel-by-panel navigation.
    fn detect_panels(&self, index: usize) -> Result<Vec<ComicPanel>>;

    /// Retrieves raw image bytes for a page frame.
    fn get_image_bytes(&self, _index: usize) -> Result<Vec<u8>> {
        Err(anyhow::anyhow!(
            "get_image_bytes not implemented for this format"
        ))
    }
}

/// Full Adobe Acrobat X / Pro content manipulation interface.
pub trait EditableDocument: Send + Sync {
    /// Applies a structural page manipulation operation (rotate, move, delete, insert).
    fn apply_page_mutation(&mut self, op: PageMutationOp) -> Result<()>;

    /// Executes multi-layer destructive redaction, stripping underlying text and pixels.
    fn apply_redactions(&mut self, regions: &[RedactionRegion]) -> Result<usize>;

    /// Stamped Bates discovery numbers formatted inside tagged `/Artifact` elements.
    fn apply_bates_numbering(&mut self, config: &BatesConfig) -> Result<()>;

    /// Mutates document metadata catalog entries.
    fn set_metadata_entry(&mut self, key: &str, value: &str) -> Result<()>;

    /// Serializes modified document to an arbitrary writer.
    fn save_to(&self, writer: &mut dyn std::io::Write) -> Result<()>;
}

/// Detects document format based on magic bytes and file extension fallback.
pub fn detect_format(path: &Path, header: &[u8]) -> Result<DocumentFormat> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Check compound extension first (.fb2.zip)
    if file_name.ends_with(".fb2.zip") {
        return Ok(DocumentFormat::Fb2);
    }

    // 1. Magic byte inspection
    if header.starts_with(b"%PDF-") {
        return Ok(DocumentFormat::Pdf);
    }

    // DjVu IFF container:
    // AT&TFORM <4-byte length> DJVU or DJVM
    // FORM <4-byte length> DJVU or DJVM
    if header.starts_with(b"AT&TFORM") {
        if header.len() >= 16 && (&header[12..16] == b"DJVU" || &header[12..16] == b"DJVM") {
            return Ok(DocumentFormat::DjVu);
        }
    } else if header.starts_with(b"FORM")
        && header.len() >= 12
        && (&header[8..12] == b"DJVU" || &header[8..12] == b"DJVM")
    {
        return Ok(DocumentFormat::DjVu);
    }

    // Microsoft Compiled HTML Help (CHM)
    if header.starts_with(b"ITSF") {
        return Ok(DocumentFormat::Chm);
    }

    // FictionBook XML (starts with <?xml or <FictionBook)
    if (header.starts_with(b"<?xml") || header.starts_with(b"<FictionBook"))
        && (header
            .windows(12)
            .any(|w| w.eq_ignore_ascii_case(b"<FictionBook"))
            || ext == "fb2")
    {
        return Ok(DocumentFormat::Fb2);
    }

    // RAR archive (CBR) - RAR4 (Rar!\x1A\x07\x00) or RAR5 (Rar!\x1A\x07\x01\x00)
    if header.starts_with(b"Rar!\x1A\x07") {
        return Ok(DocumentFormat::Comic(ComicArchiveType::Cbr));
    }

    // 7-Zip archive (CB7)
    if header.starts_with(b"7z\xBC\xAF\x27\x1C") {
        return Ok(DocumentFormat::Comic(ComicArchiveType::Cb7));
    }

    // TAR archive (CBT) - check "ustar" at offset 257
    if header.len() >= 262 && &header[257..262] == b"ustar" && (ext == "cbt" || ext == "tar") {
        return Ok(DocumentFormat::Comic(ComicArchiveType::Cbt));
    }

    // PalmDOC / MOBI / AZW3 magic header at offset 60
    if header.len() >= 68 && &header[60..68] == b"BOOKMOBI" {
        if ext == "azw3" || ext == "azw" || ext == "kf8" {
            return Ok(DocumentFormat::Azw3);
        }
        return Ok(DocumentFormat::Mobi);
    }

    // ZIP magic check (EPUB vs CBZ vs XPS vs generic zip)
    if header.starts_with(b"PK\x03\x04") {
        match ext.as_str() {
            "epub" => return Ok(DocumentFormat::Epub),
            "cbz" => return Ok(DocumentFormat::Comic(ComicArchiveType::Cbz)),
            "xps" | "oxps" => return Ok(DocumentFormat::Xps),
            "zip" => {
                // If extension is generic .zip, inspect for mimetype inside zip
                if header
                    .windows(20)
                    .any(|w| w.starts_with(b"application/epub+zip"))
                {
                    return Ok(DocumentFormat::Epub);
                }
                return Ok(DocumentFormat::Comic(ComicArchiveType::Cbz));
            }
            _ => {
                // For other extensions, check if it's EPUB mimetype
                if header
                    .windows(20)
                    .any(|w| w.starts_with(b"application/epub+zip"))
                {
                    return Ok(DocumentFormat::Epub);
                }
                // Do NOT misclassify non-document zip archives (e.g. .apk, .docx, .jar) as CBZ comics.
                // Fall through to extension check.
            }
        }
    }

    // 2. Extension-based fallback
    match ext.as_str() {
        "pdf" => Ok(DocumentFormat::Pdf),
        "xps" | "oxps" => Ok(DocumentFormat::Xps),
        "djvu" | "djv" => Ok(DocumentFormat::DjVu),
        "chm" => Ok(DocumentFormat::Chm),
        "epub" => Ok(DocumentFormat::Epub),
        "mobi" | "prc" => Ok(DocumentFormat::Mobi),
        "azw" | "azw3" | "kf8" => Ok(DocumentFormat::Azw3),
        "fb2" => Ok(DocumentFormat::Fb2),
        "md" | "markdown" | "typ" => Ok(DocumentFormat::Markdown),
        "cbz" => Ok(DocumentFormat::Comic(ComicArchiveType::Cbz)),
        "cbr" => Ok(DocumentFormat::Comic(ComicArchiveType::Cbr)),
        "cb7" => Ok(DocumentFormat::Comic(ComicArchiveType::Cb7)),
        "cbt" => Ok(DocumentFormat::Comic(ComicArchiveType::Cbt)),
        "webtoon" => Ok(DocumentFormat::Webtoon),
        _ => {
            if path.is_dir() {
                Ok(DocumentFormat::Comic(ComicArchiveType::ImageDirectory))
            } else {
                Err(anyhow!(
                    "Unsupported or unrecognized document format for path: {:?}",
                    path
                ))
            }
        }
    }
}
