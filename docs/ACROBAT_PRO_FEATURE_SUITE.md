# Adobe Acrobat Pro Enterprise Feature Suite & Security Architecture

## 1. Executive Vision: Workstation-Grade Document Authoring

For over two decades, Adobe Acrobat Pro (culminating in the definitive, unbloated workflow of Acrobat X Pro) set the gold standard for enterprise document engineering. However, proprietary lock-in, cloud forced-subscriptions, invasive telemetry, and notorious memory leaks have made Acrobat an obsolete liability in modern Linux environments.

`Kiyo` systematically reconstructs and surpasses the entire Adobe Acrobat Pro feature suite in pure Rust. It delivers **Workstation-Grade Document Authoring**:
1. **Mathematical In-Place Content Editing**: Micro-surgery on PDF content streams without breaking document layout.
2. **Interactive Light-Table Page Organizer**: Drag-and-drop page manipulation, bursting, and inter-document assembly.
3. **5-Box Prepress Geometry Engine**: Strict compliance with ISO 32000-1 §14.11.2 prepress page boundaries.
4. **Bates Numbering for Legal Discovery**: Structured pagination embedded inside ISO 14289 tagged `/Artifact` structures.
5. **Multi-Stage Destructive Redaction**: True physical scrubbing of vector paths, raster pixels, glyph runs, and hidden metadata.
6. **AcroForms with Sandboxed ECMAScript**: Secure calculation scripts executed via a zero-syscall `boa_engine` runtime.
7. **PAdES Digital Signatures**: ETSI EN 319 142 compliant cryptographic verification and detached CMS signatures.

```
+---------------------------------------------------------------------------------------------------+
|                         Kiyo Acrobat Pro Engineering Feature Matrix                               |
+---------------------------------------------------------------------------------------------------+
| [In-Place Content Editing]       | [Light-Table Organizer]         | [Prepress 5-Box Geometry]    |
| - PDF Operator Stream Patching   | - Visual Thumbnail Grid Canvas  | - MediaBox, CropBox, BleedBox|
| - Font Subsetting & Glyph Match  | - Multi-Page Drag-and-Drop Move | - TrimBox, ArtBox Enclosure  |
| - Type1/CFF/TrueType Synthesis   | - Arbitrary Angle Rotation      | - ISO 32000-1 §14.11.2 Check |
+----------------------------------+---------------------------------+------------------------------+
| [Bates Legal Discovery]          | [Destructive Redaction]         | [AcroForms & Sandboxed JS]   |
| - Zero-Padded Sequence Indexing  | - Glyph Excision & Re-encoding  | - ISO 32000-1 §12.7 Widgets  |
| - Tagged PDF/UA /Artifact Embed  | - Sutherland-Hodgman Polygon Clip| - boa_engine Zero-Syscall JS |
| - Anti-Collision Page Numbers    | - XObject Raster Scrubbing      | - Event-Driven Field Engine  |
+----------------------------------+---------------------------------+------------------------------+
| [PAdES Cryptographic Signatures] | [Structural & Visual Diff]      | [PDF/A Preflight Validator]  |
| - ETSI EN 319 142 Profile Specs  | - Real-Time WGSL Diff Shader    | - PDF/A-1b, PDF/A-2u, PDF/A-3|
| - /ByteRange Disjoint Hash Proof | - Bounding Box Spatial Delta    | - Embedded Color ICC Check   |
| - RFC 3161 TSA Timestamp Tokens  | - Semantic Typography Inspector | - Unembedded Font Detection  |
+---------------------------------------------------------------------------------------------------+
```

---

## 2. In-Place Content Editing & Operator Stream Patching

Traditional PDF viewers treat pages as static rendered bitmaps. When a user clicks to edit a typographical typo, standard tools either fail completely or insert ugly, unstyled text annotations over the original page.

`Kiyo` executes mathematical surgery on raw PDF content streams using `lopdf 0.47` and `pdf-writer 0.15`:

```
               [Raw Content Stream Stream Object (FlateDecode)]
                                      |
                                      v
                       [Content Operator Tokenizer]
             (q, Q, cm, BT, ET, Tf, Td, TD, Tm, T*, Tj, TJ, re, f)
                                      |
                                      v
                       [Spatial Glyph Run Reconstructor]
                 - Resolves font resource dictionary (/Font << /F1 >>)
                 - Matches glyph bounding boxes against user click (x, y)
                 - Computes horizontal displacement delta (dx)
                                      |
                                      v
                       [Font Subset Matcher & Injector]
                 - Inspects font descriptor (/FontDescriptor)
                 - Synthesizes matching CFF/TrueType subset glyph metrics
                                      |
                                      v
                       [Content Stream Operator Rewriter]
                 - Replaces target (Tj) string or (TJ) kerning array
                 - Rebalances matrix transformations (Tm)
                 - Emits updated Flate-compressed stream
```

### 2.1. The Operator Patching Algorithm
When editing text, `Kiyo` parses the `BT ... ET` (Begin Text / End Text) operator block:
```postscript
BT
/F1 12.00 Tf
1.00 0.00 0.00 1.00 72.00 712.00 Tm
0.00 0.00 0.00 rg
[(Hel) -20 (lo) 30 ( Wor) -10 (ld)] TJ
ET
```
If the user modifies `"World"` to `"Universe"`:
1. `Kiyo` checks if all glyphs in `"Universe"` are present in `/F1`'s `/Widths` array and `/ToUnicode` CMap table.
2. If glyphs are missing, `Kiyo` injects a supplementary font subset resource into the page `/Resources` dictionary.
3. The displacement matrix is recalculated:
   $$\Delta x = \sum_{i=1}^{n} \left( w_i - \frac{k_i}{1000} \right) \cdot \frac{T_{\text{fs}}}{1000}$$
4. Downstream operators within the same text block are shifted by $\Delta x$ to prevent overlapping adjacent words.

---

## 3. The 5-Box Prepress Geometry Model

In professional publishing, prepress printing, and legal compliance, a single "page size" does not exist. ISO 32000-1 §14.11.2 defines five distinct nested bounding boxes:

```
+---------------------------------------------------------------------------------------+
|  MediaBox (Physical Paper Medium - e.g. 612 x 792 pt)                                 |
|                                                                                       |
|      +-------------------------------------------------------------------------+      |
|      |  BleedBox (Production Printing with 3mm Bleed Margin)                    |      |
|      |                                                                         |      |
|      |      +-----------------------------------------------------------+      |      |
|      |      |  TrimBox (Intended Cut Dimensions of Finished Page)       |      |      |
|      |      |                                                           |      |      |
|      |      |      +---------------------------------------------+      |      |      |
|      |      |      |  ArtBox (Extent of Page Meaningful Content) |      |      |      |
|      |      |      |                                             |      |      |      |
|      |      |      |   [Actual Artwork, Typography, Figures]     |      |      |      |
|      |      |      |                                             |      |      |      |
|      |      |      +---------------------------------------------+      |      |      |
|      |      +-----------------------------------------------------------+      |      |
|      +-------------------------------------------------------------------------+      |
|                                                                                       |
|      CropBox (Visible Display Frustum Clipping - Default: equal to MediaBox)          |
+---------------------------------------------------------------------------------------+
```

### 3.1. Mathematical Enclosure Invariants
`Kiyo` enforces the strict mathematical hierarchy defined by ISO 32000-1:
$$\text{ArtBox} \subseteq \text{TrimBox} \subseteq \text{BleedBox} \subseteq \text{MediaBox}$$
$$\text{CropBox} \subseteq \text{MediaBox}$$

If any box is omitted in the PDF dictionary, the standard inheritance rules apply:
- If `CropBox` is missing $\implies \text{CropBox} = \text{MediaBox}$.
- If `BleedBox` is missing $\implies \text{BleedBox} = \text{CropBox}$.
- If `TrimBox` is missing $\implies \text{TrimBox} = \text{CropBox}$.
- If `ArtBox` is missing $\implies \text{ArtBox} = \text{CropBox}$.

In `kiyo-core`, the `PageMetrics` struct calculates and validates all 5 boxes:
```rust
pub struct PageMetrics {
    pub width: f32,
    pub height: f32,
    pub rotation_degrees: u16,
    pub dpi: f32,
    pub media_box: (f32, f32, f32, f32),
    pub crop_box: Option<(f32, f32, f32, f32)>,
    pub bleed_box: Option<(f32, f32, f32, f32)>,
    pub trim_box: Option<(f32, f32, f32, f32)>,
    pub art_box: Option<(f32, f32, f32, f32)>,
}
```

---

## 4. Bates Numbering & Legal Discovery Indexing

During high-stakes legal litigation and government discovery proceedings, millions of pages must be sequentially stamped with unique identifiers (e.g. `PLAINTIFF-000142-CONFIDENTIAL`).

### 4.1. The Tagged PDF / ISO 14289 (PDF/UA) Compliance Trap
Naive stamping tools simply draw text strings onto the page canvas. This creates severe compliance violations:
1. **Search Pollution**: Text searches for numbers (e.g. searching for a currency value `142`) hit thousands of irrelevant Bates stamps.
2. **Screen Reader Failure**: Accessibility software reading tagged PDFs announces the Bates number mid-sentence or as part of the document body.
3. **OCR Misattribution**: Optical character recognition models confuse Bates numbers with page headers or footnote citations.

### 4.2. Kiyo's `/Artifact` Tagged Stamping Engine
`Kiyo` stamps Bates numbers exclusively inside ISO 14289 Tagged PDF `/Artifact` structures using PDF Marked Content operators:

```postscript
/Artifact << /Type /Pagination /Subtype /Header >> BDC
q
0.00 0.00 0.00 rg
BT
/F_Bates 9.00 Tf
450.00 36.00 Td
(DOJ-DISCOVERY-004812) Tj
ET
Q
EMC
```

**Architectural Benefits**:
- `BDC` (Begin Marked Content) and `EMC` (End Marked Content) delineate the Bates stamp as non-content pagination artifact.
- Search indexing engines and screen readers completely ignore the artifact during normal text flow.
- The `BatesConfig` builder in `kiyo-core` provides collision-free page counting, custom prefixes/suffixes, and micro-aligned corner placement:
  $$\text{Stamp}(i) = \text{Prefix} + \text{format!("{:0width$}", \text{Start} + i)} + \text{Suffix}$$

---

## 5. Multi-Stage Destructive Redaction Mechanics

In government, medical (HIPAA), and legal operations, redacting sensitive information (passwords, Social Security numbers, trade secrets) is a life-or-death security requirement.

### 5.1. The Fatal Vulnerability of "Annotation Redaction"
Over 80% of security leaks in government documents occur because software developers merely place a **black rectangle annotation** (`/Subtype /Square` or `/Annot`) over sensitive text.
- The underlying text strings, glyph matrices, and vector paths remain 100% intact in the content stream.
- Anyone can highlight the black box, copy the text to clipboard, or inspect the file via `pdftotext` to recover the classified data immediately.

### 5.2. Kiyo's 4-Stage Destructive Scrubbing Pipeline
`Kiyo` executes irreversible physical excision across four distinct layers:

```
[User Selects Redaction Bounding Box R = (min_x, min_y, max_x, max_y)]
                               |
                               +---------------------------------------+
                               |                                       |
                               v                                       v
               [Stage 1: Glyph Stream Excision]        [Stage 2: Vector Path Clipping]
               - Decompose (Tj) and (TJ) runs          - Sutherland-Hodgman Polygon Clip
               - Calculate glyph bbox for each char    - Vector segments inside R purged
               - If glyph bbox intersects R:           - Endpoints reconnected to boundary
                 * Character code scrubbed             - Bezier control points recalculated
                 * Text matrix Tm adjusted             
                               |                                       |
                               +---------------------------------------+
                               |
                               v
               [Stage 3: Raster XObject Pixel Scrubbing]
               - Locate all /Subtype /Image XObjects overlapping R
               - Transform R from PDF point space to raster pixel space:
                 * (px, py) = T_image^-1 * (x_pdf, y_pdf)
               - Overwrite intersecting pixel buffer with solid color (0x00)
               - Re-encode bitmap via FlateDecode / DCTDecode
                               |
                               v
               [Stage 4: Metadata & History Sanitization]
               - Purge /Author, /Title, /Subject in /Info dict
               - Sanitize XMP /Metadata XML stream
               - Purge /PieceInfo (private Illustrator/Corel data)
               - Prune tagged /StructTreeRoot nodes pointing to redacted content
               - Linearize and rebuild xref table from scratch
                 (Purges incremental revision history so previous versions cannot be recovered)
```

---

## 6. AcroForms & Sandboxed ECMAScript Runtime

Interactive PDF forms (AcroForms per ISO 32000-1 §12.7) contain dynamic fields:
- Text inputs (`/Tx`)
- Checkboxes and radio buttons (`/Btn`)
- Dropdown combo-boxes and list boxes (`/Ch`)
- Digital signature fields (`/Sig`)

Many forms embed JavaScript to calculate totals (e.g. `Subtotal + Tax = Total`), validate dates, or show/hide sections based on user input.

### 6.1. The Zero-Syscall Security Sandbox (`boa_engine`)
Acrobat's built-in JavaScript engine has historically been the #1 attack vector for PDF zero-day remote code execution exploits (CVEs).

`Kiyo` eliminates this vulnerability by executing all AcroForm scripts inside `boa_engine` (pure-Rust ECMAScript runtime):
1. **Zero OS System Calls**: The JavaScript engine is completely isolated from the operating system. It has no access to files (`std::fs`), networking (`std::net`), environment variables, or process spawning (`std::process`).
2. **Deterministic Memory Limits**: The execution context is capped at 16 MB of heap memory with a strict instruction count limit (timeout after 50ms) to prevent infinite loop denial-of-service attacks.
3. **ISO 32000 JavaScript API Surface**: `Kiyo` exposes only the standard Acrobat PDF DOM bindings:
   - `this.getField("fieldName")`
   - `event.value`, `event.rc`, `event.target`
   - `AFSimple_Calculate("SUM", new Array("Price1", "Price2"))`
   - `AFNumber_Format(2, 0, 0, 0, "$", true)`

---

## 7. PAdES Digital Signatures (ETSI EN 319 142)

`Kiyo` provides enterprise-grade cryptographic verification and signing adhering to the European Telecommunications Standards Institute (ETSI) **PAdES** (PDF Advanced Electronic Signatures) standard.

### 7.1. Byte-Range Disjoint Hash Verification
A PDF signature dictionary (`/Subtype /adbe.pkcs7.detached` or `/ETSI.CAdES.detached`) cannot sign the entire file naively, because the signature itself is embedded inside the file.

ISO 32000-1 solves this via the `/ByteRange` array:
```postscript
/ByteRange [ 0 10240 18432 40960 ]
/Contents <3082046a06092a864886f70d010702a082045b...>
```

```
+---------------------------------------------------------------------------------------+
|  [Range 1: Byte 0 to 10240]  |  /Contents <HEX SIG>  |  [Range 2: Byte 18432 to End]  |
+------------------------------+-----------------------+--------------------------------+
\______________________________/                       \________________________________/
               \                                                       /
                \--------------------------+--------------------------/
                                           |
                                           v
                             [Cryptographic Hash (SHA-256)]
                                           |
                                           v
                         [CMS / PKCS#7 Signature Verification]
```

`Kiyo` verifies:
1. That Range 1 and Range 2 cover 100% of the document bytes excluding only the `/Contents` hex string.
2. The SHA-256 / SHA-384 hash of the concatenated byte ranges matches the message digest inside the CMS structure.
3. The X.509 certificate chain resolves to a trusted root CA.
4. If an RFC 3161 Time-Stamp Authority (TSA) token is present, its timestamp is cryptographically verified against the TSA certificate.
5. **Incremental Modification Integrity**: Kiyo inspects any revisions appended after the signature offset to ensure they contain only permissible non-visual changes (such as subsequent form fills or annotation notes).

---

## 8. Light-Table Page Organizer Architecture

The Light-Table Canvas in `kiyo-gui` provides seamless structural document reorganization:
- **Visual Grid Presentation**: High-speed thumbnail caching using GPU mipmaps.
- **Drag-and-Drop Reordering**: Multi-selection reordering mutating the PDF `/Pages` tree with $O(1)$ node splicing.
- **Arbitrary Angle Rotation**: Mutates the `/Rotate` integer attribute (0°, 90°, 180°, 270°) without expensive raster re-encoding.
- **Burst & Split Operations**: Page extraction compiling individual pages into independent, self-contained PDF binaries with pruned resource dictionaries.
- **Inter-Document Merging**: Seamless drag-and-drop of pages between two open document windows.
