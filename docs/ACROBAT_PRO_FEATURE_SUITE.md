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
8. **ICC Prepress Color Proofing & 3D LUT**: LittleCMS 2.16 with GPU-accelerated 3D LUT texture pipeline and zebra gamut warnings.
9. **Mechanical Booklet & N-Up Imposition**: Saddle-stitch folding mathematics with paper creep compensation and full printer marks.
10. **Hardened Cryptography & Permissions**: ISO 32000-2 AES-256 Rev 5/6 with SASLprep and strict `/P` bitmask enforcement.

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
| [PAdES Cryptographic Signatures] | [Structural & Visual Diff]      | [Prepress Color & Imposition]|
| - ETSI EN 319 142 Profile Specs  | - Real-Time WGSL Diff Shader    | - LittleCMS 2.16 33^3 3D LUT |
| - /ByteRange Disjoint Hash Proof | - Bounding Box Spatial Delta    | - Saddle-Stitch Creep Delta  |
| - RFC 3161 TSA Timestamp Tokens  | - Semantic Typography Inspector | - Out-of-Gamut Zebra Shaders |
+----------------------------------+---------------------------------+------------------------------+
| [Hardened Cryptography & /P]     | [PDF/A Preflight Validator]     | [Automated Document Salvage] |
| - AES-256 Rev 5/6 (SASLprep)     | - PDF/A-1b, PDF/A-2u, PDF/A-3   | - 4-Stage Corrupt Xref Scan  |
| - Empty User Pass Auto-Probe     | - Embedded Color ICC Check      | - /ObjStm Decompression      |
| - Strict Table 22 /P Bitmask     | - Unembedded Font Detection     | - Hybrid Stream Offset Parse |
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

---

## 9. Prepress ICC Color Management & LittleCMS 2.16 3D LUT Engine

In commercial offset printing (CMYK) and digital prepress, exact color fidelity is critical. A design composed in Adobe RGB, sRGB, or Display P3 must be accurately proofed against physical press profiles such as **ISO Coated v2 (ECI)**, **GRACoL 2006 Coated 1**, or **Japan Color 2001 Coated** before plate imaging.

```
+---------------------------------------------------------------------------------------------------+
|                        Kiyo LittleCMS 2.16 GPU 3D LUT Color Pipeline                              |
+---------------------------------------------------------------------------------------------------+
| [Embedded ICC / DeviceCMYK] ---> [LittleCMS 2.16 Engine] ---> [33x33x33 3D LUT Texture in VRAM]   |
|                                         |                                  |                      |
|                                         v                                  v                      |
|                              [Rendering Intent Selection]         [wgpu Trilinear Sampler]        |
|                              - Perceptual (Intent 0)                       |                      |
|                              - Relative Colorimetric + BPC (Intent 1)      v                      |
|                              - Saturation (Intent 2)             [WGSL Post-Process Shader]       |
|                              - Absolute Colorimetric (Intent 3)            |                      |
|                                                                            +---> Out-of-Gamut     |
|                                                                                  Zebra Overlay    |
+---------------------------------------------------------------------------------------------------+
```

### 9.1. The 4 ISO Prepress Rendering Intents
Kiyo supports the four standardized ICC profile rendering intents defined in ISO 15076-1:
1. **Perceptual (Intent 0)**: Compresses the entire source color gamut into the destination press gamut while preserving proportional visual distances between hues. Ideal for continuous-tone photography and complex art.
2. **Relative Colorimetric with Black Point Compensation (BPC) (Intent 1)**: Maps in-gamut colors with mathematical precision ($\Delta E_{00} \approx 0$) while clipping out-of-gamut shades to the closest boundary. Applies ISO 18619 BPC to prevent crushed shadow details in rich blacks.
3. **Saturation (Intent 2)**: Stretches out-of-gamut vectors to maximum device saturation, sacrificing hue accuracy for maximum punch. Tailored for corporate pie charts and vector graphics.
4. **Absolute Colorimetric (Intent 3)**: Preserves absolute chromaticity values without adjusting for the white point of the output medium, simulating the exact yellowish or bluish cast of physical unprinted paper substrates.

### 9.2. Zero-Latency GPU 3D LUT Pipeline
Invoking CPU-based color conversion on every pixel of a $4K$ rendered page causes severe frame drops ($> 60\text{ ms}$ latency). `Kiyo` solves this via hardware-accelerated 3D Lookup Tables (CLUT):
1. **Host-Side Generation**: On document open or profile change, `kiyo-gpu` invokes LittleCMS 2.16 (`lcms2 6.0`) to compute a $33 \times 33 \times 33$ RGB/CMYK transformation lattice ($35,937$ 32-bit float nodes).
2. **GPU Texture Upload**: The 3D table is uploaded to VRAM as a `wgpu::TextureFormat::Rgba8Unorm` or `Rgba16Float` 3D texture (`TextureViewDimension::D3`).
3. **Trilinear Hardware Interpolation**: In the presentation fragment shader, input pixel colors act as normalized 3D coordinates $(u, v, w) \in [0.0, 1.0]^3$. The GPU hardware texture unit performs trilinear volumetric filtering in sub-nanosecond time:
   ```wgsl
   @group(1) @binding(0) var lut_texture: texture_3d<f32>;
   @group(1) @binding(1) var lut_sampler: sampler;

   fn apply_icc_lut(color: vec3<f32>) -> vec3<f32> {
       return textureSample(lut_texture, lut_sampler, color).rgb;
   }
   ```

### 9.3. Out-of-Gamut Zebra Warning Shader
To prevent costly print press disasters where vibrant monitor colors clip to muddy inks, `Kiyo` provides an interactive Out-of-Gamut Warning shader.
- Evaluates the CIEDE2000 color difference:
  $$\Delta E_{00} = \sqrt{\left(\frac{\Delta L'}{k_L S_L}\right)^2 + \left(\frac{\Delta C'}{k_C S_C}\right)^2 + \left(\frac{\Delta H'}{k_H S_H}\right)^2 + R_T \frac{\Delta C'}{k_C S_C}\frac{\Delta H'}{k_H S_H}}$$
- If $\Delta E_{00} > 2.0$ (exceeding Just Noticeable Difference against target press gamut), the shader generates a dynamic diagonal high-contrast zebra pattern using screen-space coordinates:
  ```wgsl
  let screen_diag = in.position.x + in.position.y;
  let is_stripe = fract(screen_diag / 14.0) > 0.5;
  if (delta_e00 > 2.0 && is_stripe) {
      final_color = vec4<f32>(1.0, 0.78, 0.05, 1.0); // Prepress cautionary amber
  }
  ```

---

## 10. N-Up Sheet Imposition, Booklet Saddle-Stitch Engine & Printer Marks

Commercial print workflows require arranging multiple logical document pages onto larger parent printing sheets (imposition) so that once printed, folded, bound, and trimmed, pages appear in correct reading order.

```
+---------------------------------------------------------------------------------------------------+
|                            4-Page Saddle-Stitch Booklet Imposition Layout                         |
+---------------------------------------------------------------------------------------------------+
|  [Sheet s, Front Face]                           |  [Sheet s, Back Face]                          |
|  +-----------------------+---------------------+ |  +---------------------+---------------------+  |
|  | Page M - 2s           | Page 2s + 1         | |  | Page 2s + 2         | Page M - 2s - 1     |  |
|  | (Back Cover / Left)   | (Front Cover/ Right)| |  | (Inside Left Page)  | (Inside Right Page) |  |
|  +-----------------------+---------------------+ |  +---------------------+---------------------+  |
|             ^                       ^            |             ^                     ^            |
|             |                       |            |             |                     |            |
|             +---- Mechanical Creep -+            |             +-- Mechanical Creep -+            |
|                   Delta_creep(k) = k * t         |                 Delta_creep(k) = k * t         |
+---------------------------------------------------------------------------------------------------+
```

### 10.1. Mechanical Booklet Saddle-Stitch Imposition
Saddle-stitching folds nested sheets along a central spine and staples through the crease.
1. **Padding to Multiple of 4**: If total document page count is $N$, Kiyo appends blank pages until $M = 4 \cdot \lceil N / 4 \rceil$.
2. **Sheet Signature Calculation**: Total physical sheets $S = M / 4$.
3. **Face Page Placement**: For each sheet $s \in [0, S - 1]$:
   - **Front Face**: Left Page $= M - 2s$, Right Page $= 2s + 1$.
   - **Back Face**: Left Page $= 2s + 2$, Right Page $= M - 2s - 1$.

### 10.2. Corrected Mechanical Paper Creep Formula
Because paper has physical thickness $t$ (caliper), inner folded sheets are pushed outward beyond the edge of outer sheets. When the booklet face is trimmed flat with a guillotine cutter, inner pages lose their outer margins unless compensated during imposition:
$$\Delta_{\text{creep}}(k) = k \cdot t$$
- $k \in [0, S - 1]$ is the zero-indexed sheet layer index, where $k = 0$ is the outermost sheet (cover) and $k = S - 1$ is the innermost centerfold sheet.
- $t$ is the single-sheet paper thickness in PDF points ($1\text{ mm} \approx 2.83465\text{ pt}$; typical 80 gsm bond paper $t \approx 0.10\text{ mm} \approx 0.2835\text{ pt}$).
- **Spine Displacement**: For sheet $k$, inner pages are shifted inward toward the binding fold by $\Delta_{\text{creep}}(k)$. This ensures that after guillotine face trimming, outer margins are strictly uniform across all signatures.

### 10.3. N-Up Sheet Imposition Engine
Kiyo supports arbitrary matrix layouts: $2 \times 1$ (2-up), $2 \times 2$ (4-up), $4 \times 2$ (8-up), and $4 \times 4$ (16-up).
- **Aspect-Ratio Preserving Cell Fitting**: Calculates optimal rotation ($0^\circ$ or $90^\circ$) to maximize paper utilization.
- **Configurable Gutters & Bleeds**: Preserves 3mm bleed overlap between adjoining cells.
- **Order Traversal**: Horizontal LTR, Horizontal RTL, Vertical, or Boustrophedon (snake order).

### 10.4. Complete Prepress Printer Marks Engine
Kiyo embeds vector printer marks directly into the imposition stream outside the `TrimBox`:
1. **Trim Ticks**: 0.25 pt hairline marks at the corners of the `TrimBox` indicating exact cutting positions.
2. **Bleed Ticks**: Boundary corner ticks indicating the 3mm `BleedBox` envelope.
3. **Fold Marks**: Centered dashed lines on parent sheet borders indicating the spine folding axis.
4. **CMYK Density Wedges / Color Bars**: Standard 100%, 75%, 50%, and 25% tint swatches for Cyan, Magenta, Yellow, and Key (Black), plus RGB overprint blocks and neutral gray balance targets for press densitometer calibration.
5. **Registration Targets**: Precision bulls-eye crosshairs with sub-point registration alignment circles.
6. **Sheet Slug Information**: Stamped document filename, timestamp, sheet number ($s / S$), side indicator (Front/Back), and separation plate name.

---

## 11. Document Security, Cryptography & Permissions Enforcement

ISO 32000-1 and ISO 32000-2 define sophisticated cryptographic standard security handlers. Kiyo implements a zero-compromise security engine.

### 11.1. Empty User Password Auto-Probing
In enterprise, banking, and government distributions, millions of PDF files are encrypted with an Owner Password (to restrict printing or text copying) while setting the User Password to the empty string `""`.
- Naive PDF viewers display an alarming modal asking for a password, confounding users.
- `Kiyo` automatically performs **Empty User Password Auto-Probing**:
  1. On document parse, if `/Encrypt` dictionary is present, Kiyo tests decrypting the file encryption key using an empty password `""`.
  2. If authentication succeeds, the document opens instantaneously in restricted-view mode without user disruption.
  3. If auto-probing fails, Kiyo securely prompts for the user/owner decryption password.

### 11.2. Strict `/P` Permissions Bitmask Enforcement (ISO 32000 Table 22)
The `/P` integer in the `/Encrypt` dictionary is a 32-bit signed integer bitmask defining client operation privileges:

```
+---------------------------------------------------------------------------------------------------+
|                         ISO 32000-1 Table 22 /P Permissions Bitmask Table                         |
+-----+------------+-------------------------------------+------------------------------------------+
| Bit | Hex Value  | Standard ISO Privilege Meaning      | Kiyo Strict Enforcement Action           |
+-----+------------+-------------------------------------+------------------------------------------+
| 3   | 0x00000004 | Print document (low-res draft 150DPI)| If 0: Disable print spooling via ashpd   |
| 4   | 0x00000008 | Modify contents of document         | If 0: Lock content stream patching & edit|
| 5   | 0x00000010 | Copy / extract text and graphics    | If 0: Block X11 PRIMARY & clipboard copy |
| 6   | 0x00000020 | Add / modify text annotations & forms| If 0: Disable comment & sticky note tools|
| 9   | 0x00000100 | Fill in interactive AcroForm fields | If 0: Make form widgets read-only        |
| 10  | 0x00000200 | Extract text/graphics (Accessibility)| If 0: Permit screen-reader TTS only if 1 |
| 11  | 0x00000400 | Assemble document (insert/rotate/del)| If 0: Lock Light-Table reordering & split|
| 12  | 0x00000800 | High-resolution printing permitted  | If 0: Cap print rasterizer at 150 DPI    |
+-----+------------+-------------------------------------+------------------------------------------+
```
*Note: Bits 1–2, 7–8, and 13–32 are reserved by ISO 32000 and must be set to 1.*

### 11.3. Cryptographic Hardening: AES-256 Rev 5 & Rev 6 (SASLprep)
Modern PDF 2.0 documents require AES-256 encryption using Security Revision 5 (Adobe Extension Level 3) or Revision 6 (ISO 32000-2):
1. **SASLprep Password Normalization (RFC 4013)**:
   - Passwords undergo Unicode stringprep: non-ASCII spaces are mapped to standard ASCII space (`U+0020`), commonly prohibited symbols and private-use characters are rejected, and characters are canonicalized using Unicode Normalization Form KC (NFKC).
   - Strict bidirectional check prevents spoofing across RTL/LTR character boundaries.
2. **64-Round Key Derivation Loop**:
   - Computes an iterative salted hash combining password bytes, user salt, and vector data using SHA-256, SHA-384, or SHA-512 in a 64-round loop with AES-128 ECBCrypto feedback.
   - Derives the 256-bit File Encryption Key (FEK).
3. **AES-256-CBC Stream Cipher**:
   - Indirect object streams and string literals are decrypted using AES-256-CBC with a cryptographically random 16-byte initialization vector (IV) prepended to the ciphertext.
   - Validates the `/Perms` ciphertext string to guarantee that unauthorized users cannot tamper with permissions bits without rendering the document unreadable.

