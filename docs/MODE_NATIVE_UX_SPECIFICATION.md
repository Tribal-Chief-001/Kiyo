# Mode-Native UX Specification: Ergonomic Document Interaction

## 1. The Core Philosophy: Why One Interface Cannot Fit All Documents

A single user interface paradigm cannot serve the diverse cognitive requirements of different document reading workflows:
- An **academic researcher** reading a dense IEEE or arXiv paper requires constant reference jumping, figure pinning, equation tracking, and LaTeX/Typst editor synchronization.
- A **book reader** curling up with an EPUB novel requires clean typography, customized margins, hyphenation, zero clutter, and instant dictionary lookups.
- A **manga / comic reader** requires right-to-left page turning, double-page splash spread alignment, panel-by-panel guided view, and hardware binarization shaders to clean up scanned paper textures.
- A **legal / enterprise professional** requires an authoring workstation with light-table thumbnails, destructive redaction tools, AcroForm filling, and Bates stamping.

`Kiyo` introduces **Mode-Native UX Architecture**: four dedicated, polymorphic user experience modes that adapt the keyboard grammar, mouse gestures, layout orchestration, and GPU presentation shaders to match the document's native reading context.

```
+---------------------------------------------------------------------------------------------------+
|                                  Kiyo Mode-Native Interaction Matrix                              |
+-------------------+-----------------------+-------------------------+-----------------------------+
| Mode Name         | Primary Target Formats| Core Ergonomic Features | Default Layout & Pipeline   |
+-------------------+-----------------------+-------------------------+-----------------------------+
| 1. Academic Mode  | PDF, DjVu, Typst      | - Sioyek-Style Portals  | Fixed-Layout Canvas with    |
|                   |                       | - Dual Reading Ruler    | Secondary Floating Portals  |
|                   |                       | - SyncTeX Bi-Direction  | & Visual Citation Popups    |
|                   |                       | - Hover Citation Popups |                             |
+-------------------+-----------------------+-------------------------+-----------------------------+
| 2. E-Reader Mode  | EPUB, MOBI, AZW3,     | - Fluid Typst/Taffy Flow| Paginated or Scroll Reflow  |
|                   | FB2, Markdown         | - Cosmic-Text & Hypher  | with Offline Dictionary     |
|                   |                       | - Offline Lemma Lookup  | & E-Ink Gray Simulation     |
|                   |                       | - Sepia / E-Ink Palettes|                             |
+-------------------+-----------------------+-------------------------+-----------------------------+
| 3. Comic / Manga  | CBZ, CBR, CB7, CBT,   | - RTL Manga Flow        | Dual-Page Facing Spreads,   |
|    & Webtoon Mode | Webtoon Strips        | - Smart Spread Stitching| Guided Panel-by-Panel View, |
|                   |                       | - Guided Panel View     | & Infinite Vertical Ribbon  |
|                   |                       | - Oklab Binarization    | with Seam Concealment       |
+-------------------+-----------------------+-------------------------+-----------------------------+
| 4. Workstation    | PDF, AcroForms,       | - Light-Table Organizer | Multi-Pane Dockable Studio  |
|    Mode           | Prepress Print Boxes  | - Destructive Redaction | with 5-Box Prepress Canvas, |
|                   |                       | - Bates Numbering Wizard| Toolbars, & Diff Inspector  |
|                   |                       | - AcroForm boa_engine   |                             |
+-------------------+-----------------------+-------------------------+-----------------------------+
```

---

## 2. Academic Mode: High-Cognition Research Interface

Academic papers (LaTeX, Typst, ACM, IEEE, arXiv) are inherently non-linear. Readers continually jump between body text, figures, equations, and bibliography citations. Traditional PDF readers force jarring back-and-forth scrolling that destroys working memory.

### 2.1. Sioyek-Style Dynamic Portals
A **Portal** in Kiyo is a persistent, independent secondary viewport that pins a specific figure, table, formula, or proof without losing the reader's current location in the main text stream.
- **Portal Creation (`P` key)**: When clicking any citation (e.g. `Figure 4` or `Eq. (12)`), pressing `P` opens a smooth, floating picture-in-picture portal docked to the right margin or bottom corner.
- **Bidirectional Zoom & Pan**: The portal window can be zoomed and panned independently using `Alt + Scroll`.
- **Close & Return (`Esc` or `Backspace`)**: Closes the portal instantly, preserving the exact scroll coordinate $y_{\text{doc}}$ of the main reading thread.

```
+-------------------------------------------------------------------------------+
| Main Academic Reading Viewport (Page 4)       | Pinned Portal: Figure 3       |
|                                               | (Page 11)                     |
| As shown in the benchmarking results in       | +---------------------------+ |
| Figure 3, the throughput of Kiyo scales       | |  [GPU Throughput Chart]   | |
| linearly with compute dispatch queues. We     | |  - Vulkan Pipeline: 120fps| |
| can model the memory overhead as:             | |  - CPU Software: 24fps    | |
|                                               | +---------------------------+ |
|          E = \sum_{k=1}^N \alpha_k \cdot T    | [Fig 3: Hardware Scaling]   |
|                                               |                               |
| where \alpha is the cache eviction factor...  | Pin Duration: Persistent      |
+-------------------------------------------------------------------------------+
```

### 2.2. Dual Reading Ruler
Reading dense, multi-column academic text for hours causes eye strain and involuntary line skips.
- Kiyo renders a customizable horizontal **Reading Ruler** driven directly by a GPU fragment shader.
- **Ruler Thickness & Tint**: Configurable from 1 to 4 lines of text height with adjustable background dimming (dimming unread lines by 30%–60%).
- **Follow Mouse / Keyboard**: The ruler snaps smoothly to the current line based on cursor position or keyboard movement (`j` / `k`).

### 2.3. Bidirectional SyncTeX Integration
Kiyo bridges the gap between document authoring in LaTeX/Typst and PDF review:
- **Forward Synchronization**: When compiling LaTeX or Typst in an external editor (VS Code, Neovim, Emacs), the editor sends a D-Bus or UDS signal to Kiyo:
  ```bash
  kiyo-sync --file paper.pdf --line 142 --col 12
  ```
  Kiyo instantly jumps to the corresponding page coordinate $(x, y)$ and highlights the target sentence with a soft cyan pulsing beacon.
- **Inverse Synchronization (`Ctrl + Click`)**: Clicking any word in the PDF triggers an IPC message back to the editor, immediately opening the source `.tex` or `.typ` file at the exact line and column.

### 2.4. Hover Citation & Bibliography Previews
Hovering the mouse cursor over a bracketed citation reference (e.g. `[14]` or `Knuth et al., 1984`) queries the internal PDF outline and annotation link graph:
- A non-intrusive tooltip appears in **< 10 ms**, displaying the full bibtex entry, authors, title, DOI, and publication year.
- Clicking the DOI automatically opens the paper in the default system browser or retrieves its PDF via URL.

---

## 3. E-Reader Mode: Distraction-Free Typography & Immersion

Reading narrative fiction, non-fiction books, and essays requires an environment free from technical chrome, optimized for typographical excellence and reading comfort.

### 3.1. Fluid Typography Engine (Taffy + cosmic-text + hypher)
- **Zero Fixed Page Coordinates**: E-books reflow gracefully according to viewport geometry, window resizing, and display orientation (portrait vs landscape).
- **HarfBuzz Text Shaping**: Advanced OpenType feature execution (kerning pairs, discretionary ligatures, small caps, old-style figures).
- **Hyphenation Quality**: `hypher 0.1.8` applies Frank Liang's hyphenation algorithm, balancing paragraph rag and eliminating distracting whitespace rivers in justified text blocks.

### 3.2. Personalized Reading Geometry
Users have instant, real-time control over typographical layout without reload latency:
- **Margins**: Narrow (5%), Standard (12%), Wide (20%), or custom pixel padding.
- **Line Spacing (Leading)**: Continuously variable from $1.2\times$ to $2.2\times$.
- **Font Selection**: Rapid switching between system serif, sans-serif, and dyslexic fonts (e.g. Linux Libertine, EB Garamond, Inter, OpenDyslexic).
- **Multi-Column Flow**: On wide monitors ($> 1440$px), automatically splits text into elegant two-column spreads reminiscent of a physical hardcover book.

### 3.3. Offline Lemma & StarDict Dictionary Lookup
Selecting any word and pressing `D` (or double-clicking) opens an instant dictionary definition modal:
- **100% Offline**: Zero network requests or tracking.
- **Stemming & Lemmatization**: Automatically stems inflected forms (e.g. `"ran"` $\to$ `"run"`, `"hypotheses"` $\to$ `"hypothesis"`).
- **Extensible Dictionary Engines**: Supports standard StarDict (`.idx`/`.dict`), Dictd, and embedded SQLite wordbases with phonetic pronunciation (IPA), etymology, and synonyms.

### 3.4. Reading Palettes & E-Ink Simulation
```
+---------------------------------------------------------------------------------------------------+
| Color Scheme       | Background Hex | Text Hex   | Target Ambient Environment                     |
+--------------------+----------------+------------+------------------------------------------------+
| Paper White        | #FAF8F5        | #1F2328    | Well-lit daytime reading                       |
| Classic Sepia      | #F4ECD8        | #433422    | Warm indoor lighting, reduces blue light       |
| Solarized Dark     | #002B36        | #93A1A1    | Low fatigue evening reading                    |
| OLED Pitch Black   | #000000        | #D0D0D0    | Night reading, maximizes battery on OLED       |
| E-Ink Monochrome   | #E5E5E5        | #111111    | 16-level grayscale with Floyd-Steinberg dither |
+--------------------+----------------+------------+------------------------------------------------+
```

---

## 4. Comic / Manga / Webtoon Mode: Visual Sequential Art

Sequential graphic art has distinct conventions that break conventional document viewers.

### 4.1. RTL (Right-to-Left) Manga Progression Flow
Japanese Manga is authorially drawn to be read from right to left.
- In **Manga Mode**, page progression is inverted:
  - Pressing `Right Arrow` or `Space` moves to the **next** page (turning leftward).
  - Pressing `Left Arrow` moves to the **previous** page.
- In double-page view, the right page represents page $N$, and the left page represents page $N+1$.

### 4.2. Automatic Dual-Page Spread Detection & Stitching
Comic artists regularly compose panoramic 2-page splash illustrations across facing pages.
- **Aspect Ratio Heuristic**: When two sequential single pages have aspect ratios of approximately $1:1.4$ (portrait) and identical pixel heights, Kiyo pairs them into a single seamless spread.
- **Smart Offset Adjustment (`Shift + O`)**: Allows shifting the spread pairing alignment by one page (vital when the first page is an isolated cover illustration).

### 4.3. Guided Panel-by-Panel View
On laptops and smaller displays, viewing a full comic page forces text in speech bubbles to be illegibly small.
- **Automated Panel Segmentation**: `Kiyo` scans image luminance gradients to detect black gutter panel dividing lines:
  ```rust
  pub struct ComicPanel {
      pub panel_index: usize,
      pub bounds: (f32, f32, f32, f32),
      pub reading_order: usize,
  }
  ```
- **Guided Navigation**: Pressing `Space` smoothly animates the camera to zoom and frame Panel 1, then Panel 2, then Panel 3, respecting RTL/LTR reading order, before advancing to the next page.

### 4.4. Hardware-Accelerated Image Enhancement Shaders
Scanned paper manga and vintage comics suffer from yellowed pulp paper, bleed-through, and printer halftoning moiré artifacts. `Kiyo` executes real-time cleanup via custom WGSL shaders:
1. **Oklab Binarization Shader**:
   - Converts murky gray and yellowed paper backgrounds to pure, pristine white while deepening ink lines to pure black, boosting contrast by up to 300%.
2. **De-Screening (Moiré Filter)**:
   - Bilateral spatial filter suppressing periodic printer dot-matrix halftoning patterns without softening sharp ink outlines.
3. **Contrast & Dynamic Gamma**:
   - Real-time keyboard-controlled gamma expansion (`+` / `-` keys) without modifying disk image files.

---

## 5. Workstation Mode: Enterprise Document Studio

Workstation Mode transforms Kiyo into a full Adobe Acrobat Pro replacement for document mutation, legal discovery, and publishing review:

```
+---------------------------------------------------------------------------------------------------+
| Menu: File  Edit  View  Document  Forms  Tools  Signatures  Help                                  |
+---------------------------------------------------------------------------------------------------+
| [Tools]: [Organize] [Edit Text] [Redact] [Bates] [AcroForm] [Diff] [Sign] [Preflight]            |
+--------------------+-----------------------------------------------------+------------------------+
| Page Thumbnails    | Main 5-Box Prepress Canvas                          | Properties & Inspector |
| +----------------+ |                                                     | +--------------------+ |
| | [Page 1] [90°] | |   CropBox: (0, 0, 595, 842)                         | | Document Title:    | |
| +----------------+ |   MediaBox: (0, 0, 595, 842)                        | | "Annual Audit"     | |
| +----------------+ |   TrimBox: (18, 18, 577, 824)                       | | Security: PAdES    | |
| | [Page 2]       | |                                                     | | Bates: CONFID-0001 | |
| +----------------+ |   [ Interactive Destructive Redaction Rect ]        | +--------------------+ |
| +----------------+ |   (Black scrub box: text & pixels excised)          | Form Field Tree        |
| | [Page 3]       | |                                                     | - txtFirstName (/Tx)   |
| +----------------+ |   Bates Stamp: [ACROBAT-CONFIDENTIAL-000042]        | - btnSubmit (/Btn)     |
+--------------------+-----------------------------------------------------+------------------------+
```

### 5.1. The Workstation Tool Palette
- **Light-Table Organizer**: Reorder pages via drag-and-drop, rotate pages by 90° increments, delete pages, insert blank pages, or split into chapters.
- **In-Place Content Editor**: Direct cursor editing of typography strings and operator stream patching.
- **Destructive Redaction Workbench**: Visual selection of redaction rectangles, automatic pattern matching (credit card numbers, emails, SSNs), and irreversible physical scrubbing.
- **Bates Numbering Wizard**: Prefix, suffix, zero-padding, position selection, and tagged `/Artifact` emission.
- **AcroForm Builder & Tester**: Create and test form fields, attached ECMAScript validation scripts, and preview calculated totals.
- **Visual & Structural Diff Viewer**: Cyan/Magenta revision overlay with pixel sensitivity threshold and text delta lists.
