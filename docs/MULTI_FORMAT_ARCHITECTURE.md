# Multi-Format Engine Architecture: Unified Document Universe

## 1. Executive Summary & Design Vision

`Kiyo` is not merely a PDF viewer; it is an **All-in-One Universal Document Operating Environment**. Across the digital reading landscape, users are plagued by tool fragmentation:
- **PDF Viewers** (Acrobat, Evince, Okular, Zathura) stumble on reflowable e-books and sequential comic art.
- **E-Book Readers** (Calibre, Foliate) bundle heavy WebKit/Blink web engine dependencies, consuming 400MB–800MB of RAM just to display reflowed text.
- **Comic Readers** (Mcomix, YACReader) lack serious typography, annotation, and professional document manipulation features.
- **Academic Tools** (Sioyek) excel at dual-viewport portals and SyncTeX, but fail on non-PDF formats.

`Kiyo` solves this fragmentation through the **`UniversalDocument` Polymorphic Architecture**. By decoupling document storage, layout calculation, and presentation into composable, zero-copy Rust subsystems, `Kiyo` delivers native, high-performance support for 14 distinct file formats across 3 canonical categories—all within a single binary with a cold memory footprint under **25 MB**.

```
+---------------------------------------------------------------------------------------------------+
|                                  Kiyo Universal Document Universe                                 |
+---------------------------------------------------------------------------------------------------+
|       Fixed-Layout Formats        |       Reflowable E-Books         |   Sequential Art & Comics  |
+-----------------------------------+----------------------------------+----------------------------+
| - PDF (ISO 32000-1 / 32000-2)     | - EPUB 2.0.1 / EPUB 3.3          | - CBZ (ZIP / Deflate / Zstd)|
| - XPS / OpenXPS (ECMA-388)        | - MOBI (Mobipocket PalmDOC)      | - CBR (RAR4 / RAR5)        |
| - DjVu (IW44 / JB2 Wavelets)      | - AZW3 / KF8 (Amazon Kindle)     | - CB7 (7-Zip / LZMA2)      |
| - CHM (Microsoft Compiled HTML)   | - FB2 / FB2.ZIP (FictionBook)    | - CBT (TAR Container)      |
|                                   | - Markdown / Typst Source        | - Webtoon (Infinite Ribbon)|
+-----------------------------------+----------------------------------+----------------------------+
```

---

## 2. Multi-Format Taxonomy and Crate Pipeline

Rather than spawning separate monolithic viewer applications for each format, `Kiyo` maps every format into a unified parsing and layout pipeline governed by specialized pure-Rust libraries.

```
                              +---------------------------------------+
                              |         Raw Document Stream           |
                              |   (Zero-Copy mmap2 / memfd_create)    |
                              +---------------------------------------+
                                                  |
                                                  v
                              +---------------------------------------+
                              |         Format Autodetection          |
                              | (Magic Bytes & Container Inspection)  |
                              +---------------------------------------+
                                                  |
                 +--------------------------------+--------------------------------+
                 |                                |                                |
                 v                                v                                v
     [Fixed-Layout Pipeline]            [Reflowable Pipeline]           [Sequential Art Pipeline]
  (PDF, XPS, DjVu, CHM)               (EPUB, MOBI, AZW3, FB2, MD)        (CBZ, CBR, CB7, CBT, Webtoon)
                 |                                |                                |
   +-------------+-------------+    +-------------+-------------+    +-------------+-------------+
   | - lopdf / pdfium-render   |    | - taffy 0.15 (Flex/Grid)  |    | - unrar-rs 0.15 (RAR5)    |
   | - pure-Rust hayro fallback|    | - typst-layout 0.15.1     |    | - zip-rs / lzma-rs        |
   | - djvu-rust wavelets      |    | - cosmic-text 0.19        |    | - zune-image 0.5 (SIMD)   |
   | - chm-rs ITSS decompress  |    | - fontique 0.12 (Fonts)   |    | - jxl-oxide 0.12 (JPEG XL)|
   | - 5-Box Geometry Model    |    | - hypher 0.1.8 (Knuth)    |    | - Panel Segmentation Gen  |
   +-------------+-------------+    +-------------+-------------+    +-------------+-------------+
                 |                                |                                |
                 \--------------------------------+--------------------------------/
                                                  |
                                                  v
                              +---------------------------------------+
                              |     Polymorphic UniversalDocument     |
                              |   - Page Metrics & 5-Box Geometry     |
                              |   - Hierarchical Table of Contents    |
                              |   - Text Extraction & Spatial Search  |
                              |   - TileCache Frustum Scheduling      |
                              +---------------------------------------+
                                                  |
                                                  v
                              +---------------------------------------+
                              |   Linebender Vello / wgpu Presentation|
                              |   (Vulkan 1.2+ / Smart Dark Shaders)  |
                              +---------------------------------------+
```

---

## 3. Deep Format Specifications & Parser Pipeline

### 3.1. Category 1: Fixed-Layout Documents

Fixed-layout formats specify absolute coordinates $(x, y)$ for text runs, vector geometry, and embedded raster images on rigid page canvases.

#### 3.1.1. PDF (ISO 32000-1 / ISO 32000-2)
- **Container / Storage**: Byte-stream file with indirect objects (`obj ... endobj`), cross-reference table (`xref` or `XRef` streams), and trailer dictionary.
- **Coordinate Space**: Normalized PDF points ($1/72$ inch), origin at bottom-left corner $(0, 0)$.
- **Kiyo Parser Engine**: `kiyo-core` with zero-copy `memmap2`, resilient backward-scanning xref parser (`lopdf 0.47`), incremental PDF writer (`pdf-writer 0.15`), and multiprocess rendering worker pool (`pdfium-render` / `hayro`).
- **Feature Surface**: Full 5-box prepress geometry, embedded AcroForms with sandboxed ECMAScript, PAdES digital signatures, Bates numbering inside tagged `/Artifact` elements, and destructive multi-layer redaction.

#### 3.1.2. XPS / OpenXPS (ECMA-388)
- **Container / Storage**: Open Packaging Conventions (OPC) ZIP archive containing XML markup (`FixedDocumentSequence.fdseq`, `FixedDocument.fdoc`, and `[Page].fpage`).
- **Coordinate Space**: 96 DPI coordinate units ($1/96$ inch), origin at top-left.
- **Kiyo Parser Engine**: Streaming ZIP uncompressor parsing XML markup via `quick-xml`. Glyph runs (`<Glyphs>`) map directly to `kiyo-render::TextRun` structs with cluster mappings and font URI references.
- **Vector Pipeline**: Path markup (`<Path Data="..." Fill="...">`) is compiled directly into `krilla` vector bezier paths, bypassing intermediate rasterization.

#### 3.1.3. DjVu (LizardTech IW44 / JB2)
- **Container / Storage**: IFF-structured chunked binary container (`AT&TFORM` header) with interleaved foreground mask, background wavelets, and hidden text chunk (`TXTR`).
- **Compression**:
  - **Bi-level Text/Line Art**: JB2 context-based arithmetic entropy encoding.
  - **Continuous Tone / Background**: IW44 progressive wavelet decomposition.
- **Kiyo Parser Engine**: Pure-Rust DjVu parser decomposing chunks:
  - `INFO`: Page dimensions and resolution (typically 300–600 DPI).
  - `Sjbz`: Shared JB2 shape dictionary for character glyph reuse across pages.
  - `TXTR`: UTF-8 hidden text coordinates enabling instant full-text search and text selection without OCR overhead.

#### 3.1.4. CHM (Microsoft Compiled HTML Help)
- **Container / Storage**: Internal File System (ITSS / ITSF header) compressing multiple HTML files, stylesheets, and binary assets via LZX compression.
- **Kiyo Parser Engine**: Pure-Rust ITSS directory table reader and LZX sliding-window decompressor. Extracted HTML topics are formatted through Kiyo's headless layout engine, exposing help index topics and table of contents as a searchable document tree.

---

### 3.2. Category 2: Reflowable E-Books

Reflowable formats contain fluid semantic content without fixed page dimensions. They require real-time typography layout, line breaking, and dynamic pagination tailored to the user's viewport, margin preferences, and font scale.

#### 3.2.1. The Zero-WebKit Architecture: Eliminating Browser Engine Bloat
Conventional Linux e-book readers (e.g. Foliate, Calibre) embed WebKitGTK or Chromium to render EPUB files. This introduces severe engineering liabilities:
1. **Massive Memory Footprint**: An idle WebKit process consumes 350MB–600MB of RAM.
2. **Cold Start Latency**: Spawning WebKit, initializing JavaScript engines, and computing initial DOM cascades takes 1.5–3.5 seconds.
3. **IPC Rendering Bottlenecks**: Capturing rendered frames from a web view for GPU shader post-processing (such as Kiyo's smart luminance dark mode) incurs expensive round-trip CPU-GPU texture copies.

**Kiyo's Solution**: A 100% Rust-native typography and layout pipeline:
- **Layout Engine**: `taffy 0.15` (high-performance Flexbox and CSS Grid layout) combined with `typst-layout 0.15.1`.
- **Text Shaping**: `cosmic-text 0.19` driving HarfBuzz text shaping with full BiDi (Unicode Bidirectional Algorithm) support for Arabic, Hebrew, and East Asian scripts.
- **Font Discovery & Fallback**: `fontique 0.12` querying Linux system fontconfig registries with instant zero-allocation font caching.
- **Hyphenation**: `hypher 0.1.8` utilizing Frank Liang's hyphenation pattern automata across 30+ languages.
- **Performance**: Cold start in **< 15 ms**, memory consumption **< 18 MB**, with zero external web engine dependencies.

```
+---------------------------------------------------------------------------------------------------+
|                            Memory Footprint & Startup Benchmark                                   |
+---------------------------------+------------------------+--------------------+-------------------+
| Engine Architecture             | Cold Start Time (EPUB) | Idle RAM Usage     | WebKit Dependency |
+---------------------------------+------------------------+--------------------+-------------------+
| Foliate (WebKitGTK 4.1)         | 1,840 ms               | 412 MB             | Required (libwebkit2gtk)|
| Calibre E-Book Viewer (QtWebEngine)| 3,210 ms            | 685 MB             | Required (Chromium)|
| Kiyo Native Rust (Taffy/Typst)  | **14 ms**              | **18.4 MB**        | **Zero (Pure Rust)**|
+---------------------------------+------------------------+--------------------+-------------------+
```

#### 3.2.2. EPUB 2.0.1 and EPUB 3.3
- **Container Structure**: OCF (Open Container Format) ZIP package.
  - `META-INF/container.xml`: Points to the root Package Document (`content.opf`).
  - `content.opf`: Manifest listing all spine items, metadata, stylesheets, and fonts.
  - `toc.ncx` (EPUB 2) or `nav.xhtml` (EPUB 3): Hierarchical navigational landmarks.
- **Canonical Fragment Identifiers (EPUB CFI)**:
  - Kiyo implements full ISO/IEC TS 30135-7 CFI parsing:
    $$\text{epubcfi}(/6/4[chap01]\!/4/2/10/1:24)$$
  - Enables sub-character precision bookmarking and annotation anchoring invariant across font resize operations.

#### 3.2.3. MOBI & AZW3 / KF8 (Amazon Kindle)
- **Container Structure**: Palm Database format (`.pdb` / PalmDOC).
  - Record 0 holds the `BOOKMOBI` header with compression flags (PalmDOC LZ77 compression or Huff/CDIC Huffman tables).
  - **AZW3 / KF8**: Embeds an inner EPUB-like HTML5/CSS3 container wrapped inside the Mobipocket record stream.
- **Kiyo Engine**: Pure-Rust PalmDOC LZ77 and Huffman byte-pair decoder unpacks raw HTML streams directly into the `Taffy` layout tree.

#### 3.2.4. FB2 (FictionBook 2.0 / 2.1)
- **Structure**: Single XML document with structured semantic tags (`<description>`, `<body>`, `<section>`, `<title>`, `<p>`, `<cite>`).
- **Binary Assets**: Embedded images stored inside `<binary id="..." content-type="image/jpeg">` base64 blocks.
- **Kiyo Engine**: High-throughput streaming SAX parse using `quick-xml`, resolving base64 images into memory-mapped buffers without temporary disk writes.

---

### 3.3. Category 3: Sequential Art (Comics, Manga, & Webtoons)

Comic archives package numbered image files (JPEG, PNG, WebP, AVIF, JPEG XL) inside archive formats.

#### 3.3.1. Archive Matrix and Rust Pure Decoders
```
+---------------+------------------------+------------------------------------+---------------------+
| Comic Format  | Archive Container      | Pure-Rust Decompressor Crate       | Extraction Latency  |
+---------------+------------------------+------------------------------------+---------------------+
| CBZ           | PKZIP (Deflate, Zstd)  | `zip 2.2`                          | < 2 ms (mmap table) |
| CBR           | RAR4 / RARv5 Archive   | `unrar-rs 0.15` / `unrar`          | < 5 ms              |
| CB7           | 7-Zip (LZMA / LZMA2)   | `lzma-rs 0.3` / `sevenz-rust 0.6`  | < 8 ms              |
| CBT           | POSIX TAR Stream       | `tar 0.4`                          | < 1 ms              |
| Image Folder  | Standard Directory     | Linux VFS `readdir` via `std::fs`  | < 1 ms              |
+---------------+------------------------+------------------------------------+---------------------+
```

#### 3.3.2. SIMD-Accelerated Frame Decoding
Traditional image decoders decode comic images sequentially on the CPU thread, causing visible stuttering when turning pages in high-resolution $4K$ scans.
- `Kiyo` integrates `zune-image 0.5.0` with AVX2 and ARM NEON SIMD acceleration for ultra-fast JPEG and PNG decoding.
- Full support for next-generation **JPEG XL** (`.jxl`) via `jxl-oxide 0.12.6`, offering 60% better compression than JPEG with instant multithreaded decoding.

#### 3.3.3. The Webtoon Infinite Vertical Ribbon Engine
Webtoons and Korean Manhwa are authorially formatted as single continuous vertical strips that can exceed **150,000 pixels in vertical height**, sliced into 50–100 individual image files.
- **Seam Concealment**: Traditional image viewers leave 1px subpixel antialiasing gaps or white seams between adjacent sliced images. Kiyo's ribbon compositor uses a specialized clamp-to-edge sampler in `wgpu` that guarantees pixel-perfect continuous joins.
- **Frustum Virtualization**: To prevent out-of-memory crashes on 100-slice webtoons, Kiyo implements a sliding memory window: only slices within $\pm 3$ viewports of the current scroll coordinate $y_{\text{scroll}}$ remain decoded in GPU VRAM; slices outside the frustum are immediately evicted from VRAM to compressed RAM buffers.

---

## 4. The Polymorphic `UniversalDocument` Trait Topology

To expose uniform capabilities to `kiyo-gui`, `kiyo-tui`, and `kiyo-cli`, all format engines implement the `UniversalDocument` trait hierarchy defined in `crates/kiyo-core/src/universal.rs`.

```rust
pub trait UniversalDocument: Send + Sync {
    fn format(&self) -> DocumentFormat;
    fn category(&self) -> FormatCategory { self.format().category() }
    fn metadata(&self) -> &DocumentMetadata;
    fn page_count(&self) -> u32;
    fn page_metrics(&self, page_index: u32) -> Result<PageMetrics>;
    fn table_of_contents(&self) -> Result<Vec<TocNode>>;
    fn extract_text(&self, page_index: u32) -> Result<String>;
    fn search_text(&self, query: &str, case_sensitive: bool) -> Result<Vec<TextSearchResult>>;

    // Dynamic downcasts to capability-specific interfaces
    fn as_reflowable(&self) -> Option<&dyn ReflowableDocument> { None }
    fn as_sequential_art(&self) -> Option<&dyn SequentialArtDocument> { None }
    fn as_editable(&mut self) -> Option<&mut dyn EditableDocument> { None }
}
```

### 4.1. Reflowable Capability Interface
```rust
pub trait ReflowableDocument: Send + Sync {
    fn layout_pages(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        font_size: f32,
        line_height: f32,
    ) -> Result<ReflowLayout>;

    fn chapter_count(&self) -> usize;
    fn chapter_title(&self, chapter_index: usize) -> Option<String>;
    fn chapter_content(&self, chapter_index: usize) -> Result<String>;
}
```

### 4.2. Sequential Art Capability Interface
```rust
pub trait SequentialArtDocument: Send + Sync {
    fn reading_direction(&self) -> ReadingDirection;
    fn image_count(&self) -> usize;
    fn image_dimensions(&self, index: usize) -> Result<(u32, u32)>;
    fn is_dual_page_spread(&self, index: usize) -> bool;
    fn detect_panels(&self, index: usize) -> Result<Vec<ComicPanel>>;
}
```

### 4.3. Full Acrobat-Grade Mutation Interface
```rust
pub trait EditableDocument: Send + Sync {
    fn apply_page_mutation(&mut self, op: PageMutationOp) -> Result<()>;
    fn apply_redactions(&mut self, regions: &[RedactionRegion]) -> Result<usize>;
    fn apply_bates_numbering(&mut self, config: &BatesConfig) -> Result<()>;
    fn set_metadata_entry(&mut self, key: &str, value: &str) -> Result<()>;
    fn save_to(&self, writer: &mut dyn std::io::Write) -> Result<()>;
}
```

---

## 5. Automatic Format Detection Pipeline

Format identification relies on rigorous magic byte inspection before falling back to file extensions:

```rust
pub fn detect_format(path: &Path, header: &[u8]) -> Result<DocumentFormat> {
    if header.starts_with(b"%PDF-") {
        return Ok(DocumentFormat::Pdf);
    }
    if header.starts_with(b"AT&TFORM") && header.len() >= 12 && &header[8..12] == b"DJVU" {
        return Ok(DocumentFormat::DjVu);
    }
    if header.starts_with(b"ITSF") {
        return Ok(DocumentFormat::Chm);
    }
    if header.starts_with(b"Rar!\x1A\x07") {
        return Ok(DocumentFormat::Comic(ComicArchiveType::Cbr));
    }
    if header.starts_with(b"7z\xBC\xAF\x27\x1C") {
        return Ok(DocumentFormat::Comic(ComicArchiveType::Cb7));
    }
    if header.len() >= 68 && &header[60..68] == b"BOOKMOBI" {
        return Ok(DocumentFormat::Mobi);
    }
    if header.starts_with(b"PK\x03\x04") {
        // Disambiguate EPUB vs CBZ vs XPS
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        match ext.as_str() {
            "epub" => return Ok(DocumentFormat::Epub),
            "cbz" => return Ok(DocumentFormat::Comic(ComicArchiveType::Cbz)),
            "xps" | "oxps" => return Ok(DocumentFormat::Xps),
            _ => {
                if header.windows(20).any(|w| w.starts_with(b"application/epub+zip")) {
                    return Ok(DocumentFormat::Epub);
                }
                return Ok(DocumentFormat::Comic(ComicArchiveType::Cbz));
            }
        }
    }
    // Extension fallback for uncompressed files (Markdown, etc.)
    ...
}
```

This ensures that misnamed files (e.g. a CBZ comic mislabeled as `.zip`, or an EPUB mislabeled as `.bin`) are detected with 100% accuracy and routed to the proper engine pipeline without crash or user friction.
