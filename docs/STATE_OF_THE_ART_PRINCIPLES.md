# The 14 Core Principles for Day 0 State of the Art and Novel Quality-of-Life Innovations

## 1. The 14 Architectural Principles

To definitively outclass all existing PDF readers and toolchains from Day 0, `Kiyo` is engineered around fourteen non-negotiable architectural principles:

```
+----------------------------------------------------------------------------------------------------+
|                                      The 14 Core Principles                                        |
+----------------------------------------------------------------------------------------------------+
|  1. Zero-Stutter 120Hz Loop   | Decouple UI thread from rasterization; asynchronous tile pipeline. |
|  2. Zero-Copy Memory Mapping  | Sub-millisecond open via memmap2; lazy demand-paged page loading.  |
|  3. Dual-Engine Synergy       | First-class parity between GPU Desktop (wgpu) & Headless CLI.      |
|  4. Smart Dark Mode           | Perceptual Oklab inversion that never ruins photos, charts, or art.|
|  5. Academic Portals          | Floating dual-viewports for non-disruptive citation inspection.    |
|  6. Bidirectional SyncTeX     | Sub-millisecond roundtrip between PDF point and Neovim/VS Code.    |
|  7. Atomic Live Reload        | Flicker-free memory buffer swap during Typst/LaTeX compilation.    |
|  8. Zero-C++ Neural OCR       | Embedded ocrs+rten; instant ISO 32000 sandwich PDFs (3 Tr).        |
|  9. Perceptual Visual Diff    | Real-time GPU XOR shaders highlighting typographic revisions.      |
| 10. XY-Cut Markdown Extractor | Reconstruct double-column papers and complex tables into Markdown. |
| 11. Linux Native Citizenship  | Nemo actions, instant PRIMARY copy, D-Bus theme sync, HiDPI crisp. |
| 12. Hermetic Single Binary    | Zero external shared-library crashes; runs identically everywhere. |
| 13. Kinetic Ergonomic Physics | Viscous/drag decay, Avy jump tags, Wayland gestures, 3pt hit halo. |
| 14. Prepress Color & Crypto   | LittleCMS 33^3 LUT, saddle creep, auto-probe pass, AES-256 SASL.   |
+----------------------------------------------------------------------------------------------------+
```

---

## 2. In-Depth Walkthrough of Novel Quality-of-Life Innovations

### 2.1. Principle 1: Zero-Stutter 120Hz/144Hz Rendering Loop
- **The Defect in Existing Viewers**: Poppler and MuPDF render synchronously or lock the UI thread while waiting for raster surfaces. On 120Hz or 144Hz high-refresh displays, frame pacing is erratic, dipping down to 15–25 FPS during rapid scrolling.
- **Kiyo's Solution**:
  - The UI event loop (`winit` + `wgpu`) runs at the native monitor refresh rate (120Hz / 8.33ms per frame).
  - Page rasterization is completely offloaded to the isolated multi-process worker pool (`kiyo-worker`).
  - While a high-resolution tile is rendering, the presentation pipeline displays a bilinear-upscaled preview from the lower Level of Detail (LoD) mipmap cache. The user experiences zero blank white screens and zero input stutter.

### 2.2. Principle 2: Zero-Copy Virtual Memory Mapping (`memmap2`)
- **The Defect in Existing Viewers**: Desktop applications frequently allocate large heap buffers to read PDF files, causing high RSS and long startup times on gigabyte-sized files.
- **Kiyo's Solution**:
  - Documents are memory-mapped into virtual memory via `memmap2::Mmap`.
  - The Linux kernel page cache manages physical RAM residency. A 2,000-page document opens in **< 1 millisecond**, consuming under **20 MiB RSS** until specific pages are scrolled into view.

### 2.3. Principle 3: Dual-Engine Synergy: Native GPU Desktop (GUI) & Headless Shell Automation (CLI)
- **The Defect in Existing Viewers**: Users are forced to maintain disparate tools and mental models: graphical viewers (Evince/Sioyek) cannot be scripted or integrated into headless terminal pipelines, while CLI manipulation utilities (`pdfcpu`/`pdftk`) lack visual feedback and desktop launching capabilities.
- **Kiyo's Solution**:
  - A unified binary `kiyo` providing dual-engine synergy:
    - `kiyo gui <doc.pdf>` (or simply `kiyo <doc.pdf>`): Instant launch of the high-framerate hardware-accelerated desktop viewer.
    - `kiyo merge`, `split`, `diff`, `ocr`, `extract`, `watch`, `thumbnail`: Scriptable, sub-millisecond headless CLI pipelines and Linux Mint Nemo context actions.
  - **Architectural Note on TUI Retirement**: Earlier prototypes explored a terminal UI (`kiyo-tui`). However, brittle terminal graphics protocols (Sixel, Kitty graphics, iTerm2, ANSI half-blocks) and multiplexer inconsistencies across terminal emulators compromised rendering precision and created duplicate maintenance overhead. Retiring the TUI enables 100% focused polish on the 120Hz/144Hz native GPU desktop experience while ensuring the headless CLI remains an ultra-fast, robust automation powerhouse.

### 2.4. Principle 4: Luminance-Preserving Smart Dark Mode
- **The Defect in Existing Viewers**: Naive negative color inversion ($C' = 1.0 - C$) turns high-resolution microscope scans, circuit diagrams, portraits, and multi-color charts into bizarre inverted negatives.
- **Kiyo's Solution**:
  - Custom WGSL GPU shader converting sRGB to perceptual Oklab color space.
  - Inverts luminance of white backgrounds ($L \approx 1.0 \to 0.10$ charcoal) while preserving the chroma ($C = \sqrt{a^2 + b^2}$) of diagrams and photographs. Reading technical papers in bed or dark environments is natural and easy on the eyes.

### 2.5. Principle 5: Sioyek-Grade Academic Portals
- **The Defect in Existing Viewers**: In a 30-page academic paper, clicking a citation link `[14]` jumps the entire viewport to the bibliography at Page 28. The reader loses their place in the theorem, requiring multiple jump-back keystrokes.
- **Kiyo's Solution**:
  - Pressing `p` or clicking a cross-reference opens an **Academic Portal**: a non-disruptive secondary floating viewport previewing the target figure, equation, or bibliography entry.
  - The primary document viewport remains anchored at the reading position.
  - Portals can be pinned (`P`), resized, or closed with `Esc`.

```
+-----------------------------------------------------------------------------------------+
| Primary Viewport (Page 4)                             | Floating Portal Viewport        |
|                                                       | (Anchored to Page 28, Ref [14]) |
| ...as demonstrated by Turing et al. [14],             +---------------------------------+
| the halting problem remains undecidable for           | [14] A. M. Turing, "On          |
| universal computing machines...                       | Computable Numbers...", Proc.   |
|                                                       | London Math. Soc., 1936.        |
|                                                       +---------------------------------+
|                                                                                         |
+-----------------------------------------------------------------------------------------+
```

### 2.6. Principle 6: Bidirectional SyncTeX with Neovim & VS Code
- **Forward Synchronization**: When compiling LaTeX or Typst in Neovim/VS Code, invoking `kiyo --synctex-forward <line>:<col>:<source.tex>` smoothly animates the viewport to the exact rendered PDF coordinate, highlighting the corresponding paragraph with a subtle pulsed bounding box.
- **Reverse Synchronization**: Holding `Ctrl` + left-clicking any word or mathematical symbol in `kiyo-gui` triggers an RPC call or terminal command opening Neovim/VS Code at the exact source file line and column:
  ```bash
  nvim --server /tmp/nvim.sock --remote-send "<Esc>:edit +<line> <file><CR>"
  ```

### 2.7. Principle 7: Atomic Flicker-Free Live Compiler Reload
- **The Defect in Existing Viewers**: While running `typst watch` or `latexmk`, external viewers either lock the file (preventing compiler writes) or flash a jarring white screen on reload, resetting zoom and scroll offsets.
- **Kiyo's Solution**:
  - `kiyo-gui` watches the document descriptor using inotify via the `notify` crate.
  - On file modification, it maps the new revision in memory in the background, renders the visible frustum tiles into a secondary staging buffer, and executes an **atomic GPU texture swap**.
  - **Zero White Flash**: The user sees text update instantly in place as they type in their editor.

### 2.8. Principle 8: Zero-C++ Embedded Neural OCR
- **The Defect in Existing Viewers**: Scanning documents or processing non-searchable PDFs requires installing Tesseract, Python, and shell wrappers like `ocrmypdf`.
- **Kiyo's Solution**:
  - `kiyo-ocr` embeds pure-Rust neural models (`ocrs` + `rten`) directly in the binary.
  - Executes real-time text detection (DBNet) and text recognition (CRNN) with AVX2/AVX-512 acceleration.
  - Injects ISO 32000-1 compliant invisible text operators (`3 Tr`), transforming flat scanned bitmaps into fully selectable, searchable, copy-pasteable PDFs in seconds.

### 2.9. Principle 9: Perceptual Visual XOR Revision Diffing
- **The Defect in Existing Viewers**: Comparing two revisions of a contract, legal filing, or engineering schematic requires tedious manual side-by-side reading.
- **Kiyo's Solution**:
  - The `kiyo diff file_v1.pdf file_v2.pdf` command aligns corresponding pages and renders them into GPU textures.
  - The Visual XOR Diff shader highlights deleted clauses/lines in vivid red and inserted additions in vivid green, while rendering unchanged typography in muted monochrome.

### 2.10. Principle 10: XY-Cut Layout Analysis & Markdown Extraction
- **The Defect in Existing Viewers**: Copying text from a two-column academic paper in traditional viewers copies across the columns, producing garbled sentences that mix left and right columns.
- **Kiyo's Solution**:
  - `kiyo-core` implements recursive **XY-Cut spatial decomposition**:
    1. Projects glyph horizontal and vertical bounding histograms.
    2. Identifies vertical white space gutters separating columns.
    3. Traverses blocks in topological reading order.
  - Command `kiyo extract --markdown` outputs clean, human-readable Markdown with reconstructed headers, paragraphs, and tables.

### 2.11. Principle 11: Linux Mint & Desktop Citizenship
- Seamless integration with Linux Mint Cinnamon, Nemo file manager context actions, X11 `PRIMARY` selection ownership, Wayland `wp_fractional_scale_v1` crisp rendering, and real-time D-Bus theme tracking.

### 2.12. Principle 12: Hermetic Single-Binary Portability
- Zero runtime dynamic C++ shared library dependency crashes. Builds cleanly with `cargo build --release` and ships as a unified standalone binary or Flatpak/AppImage running reliably across all Linux distributions.

### 2.13. Principle 13: Kinetic Physics, Micro-Interactions & Ergonomics
- **The Defect in Existing Viewers**: Legacy document viewers move viewports through coarse, discontinuous jumps that cause disorientation. Hyperlink targeting is unforgiving, mouse-free jumping across words is nonexistent, and desktop tiling window managers mangle secondary preview windows.
- **Kiyo's Solution**:
  - **Kinetic Physics**: Viscous friction and aerodynamic quadratic drag ($\frac{dv}{dt} = -\mu_{\text{visc}} v - c_{\text{aero}} v |v|$) combined with critically damped harmonic restitution ($\zeta = 1.0, \omega_n = 26.0\text{ rad/s}$) snap viewports smoothly to boundaries in ~154ms with zero overshoot.
  - **Hop/Avy Spatial Navigation**: Pressing `f <char>` overlays two-character mnemonics over matching text, enabling instantaneous keyboard jumping across visible pages without mouse interaction.
  - **Wayland Continuous Gestures & 3pt Hit Halo**: Continuous pinch-to-zoom (`zwp_pointer_gestures_v1`) paired with an ergonomic 3-point Euclidean margin expansion ($d((x,y), B) \le 3\text{ pt}$) eliminates targeting frustration on high-DPI displays.
  - **Subsurface Overlay Isolation**: Academic portals and loupe magnifiers are mapped as Wayland `wl_subsurface` and `xdg_popup` surfaces, ensuring they never trigger unwanted tiling in Sway or Hyprland.

### 2.14. Principle 14: Prepress Color Fidelity, Mechanical Imposition & Cryptographic Hardening
- **The Defect in Existing Viewers**: Desktop open-source readers discard CMYK press color accuracy, lack print imposition tooling with paper creep compensation, prompt users with spurious password modals for empty-password files, and ignore ISO 32000 permissions bitmasks.
- **Kiyo's Solution**:
  - **LittleCMS 2.16 3D LUT**: Converts device CMYK press profiles (FOGRA51, GRACoL) into calibrated RGB via a $33 \times 33 \times 33$ 3D LUT texture in GPU VRAM, backed by an out-of-gamut zebra warning shader evaluating CIEDE2000 ($\Delta E_{00}$).
  - **Booklet Saddle-Stitch Imposition & Creep**: Imposes signatures with the exact mechanical creep formula $\Delta_{\text{creep}}(k) = k \cdot t$, shifting inner pages inward to guarantee uniform outer margins after guillotine face trimming. Emits full prepress printer marks (trim, bleed, fold, CMYK color bars).
  - **Hardened Cryptography & Strict `/P` Table 22**: Transparent empty-user-password auto-probing, strict enforcement of ISO 32000 Table 22 `/P` bitmasks (Bits 3, 4, 5, 6, 9, 10, 11, 12), and AES-256 Rev 5/6 decryption with SASLprep Unicode normalization.

