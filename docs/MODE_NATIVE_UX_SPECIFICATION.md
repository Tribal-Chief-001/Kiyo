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

---

## 6. Kinetic Scrolling Physics Engine & Detent Impulse

To eliminate the jarring, mechanical step-scrolling common to legacy Linux PDF readers, `Kiyo` incorporates a continuous kinetic physics simulation executed directly on the 120Hz/144Hz presentation loop.

```
+---------------------------------------------------------------------------------------------------+
|                              Kiyo Kinetic Scrolling Simulation Architecture                       |
+---------------------------------------------------------------------------------------------------+
|  [Input: Wheel Detent / Touch Drag] ---> [Impulse Injector: J_wheel = +/- 350 px/s]               |
|                                                     |                                             |
|                                                     v                                             |
|                     [Momentum Decay: dv/dt = -mu_visc * v - c_aero * v * |v|]                     |
|                                                     |                                             |
|                         +---------------------------+---------------------------+                 |
|                         | (Within Document Bounds)                              | (Past Bounds)   |
|                         v                                                       v                 |
|                [Linear Displacement]                                 [Overscroll Resistance]      |
|                y += v * dt                                           y_disp = y_bound +           |
|                                                                      d * tanh(Delta y / d)        |
|                                                                                 |                 |
|                                                                                 v                 |
|                                                                      [Harmonic Restitution]       |
|                                                                      zeta = 1.0, omega_n = 26     |
+---------------------------------------------------------------------------------------------------+
```

### 6.1. Viscous and Aerodynamic Momentum Decay
Once a fling or rapid scroll gesture is released, velocity decays according to combined viscous fluid friction and quadratic aerodynamic drag:
$$\frac{dv}{dt} = -\mu_{\text{visc}} v - c_{\text{aero}} v |v|$$
- $\mu_{\text{visc}} = 3.5\text{ s}^{-1}$: Provides natural, progressive deceleration at lower velocities.
- $c_{\text{aero}} = 0.0012\text{ px}^{-1}$: Clamps high-velocity flings smoothly, preventing disorienting runaway skips across thousands of pages.

### 6.2. Non-Linear Overscroll Pull Resistance (Rubber-Banding)
When the user drags past the beginning ($y < 0$) or end ($y > y_{\text{max}}$) of a document, boundary pull resistance is modulated using a hyperbolic tangent clamp:
$$y_{\text{disp}} = y_{\text{bound}} + d \cdot \tanh\left(\frac{\Delta y}{d}\right)$$
- $d = 120.0\text{ pt}$: Maximum asymptotic displacement limit.
- $\Delta y$: Raw unconstrained cursor displacement beyond the boundary.
- **Behavior**: As $\Delta y \to \infty$, $y_{\text{disp}}$ smoothly approaches $y_{\text{bound}} + d$ without abrupt hard stops.

### 6.3. Critically Damped Harmonic Restitution
Upon pointer release in an overscrolled state, the viewport snaps back to document bounds governed by a 2nd-order harmonic spring equation:
$$\frac{d^2 x}{dt^2} + 2\zeta\omega_n \frac{dx}{dt} + \omega_n^2 x = 0$$
- $\zeta = 1.0$: Damping ratio calibrated to **critical damping** (strictly zero oscillatory bounce or overshoot).
- $\omega_n = 26.0\text{ rad/s}$: Natural undamped angular frequency.
- **Settling Time**: Total restitution duration is $t_{\text{settle}} \approx \frac{4}{\omega_n} \approx 154\text{ ms}$, delivering a crisp, tactile snap-back to the page edge.

### 6.4. Mouse Wheel Detent Impulse Smoothing
Discrete notched mouse wheels generate discontinuous step events. `Kiyo` converts wheel notches into smooth kinetic impulses:
- Each detent notch imparts an instantaneous velocity impulse $J_{\text{wheel}} = \pm 350.0\text{ px/s}$.
- Multiple rapid notches accumulate additively into $v_0 = \sum J_i$, which is integrated over sub-frame intervals $\Delta t \approx 8.33\text{ ms}$, entirely eliminating stutter and micro-freezes.

---

## 7. Ergonomic Micro-Interactions, Gestures & Spatial Navigation

Document navigation in Kiyo is designed around micro-ergonomics that preserve cognitive focus.

### 7.1. Perceptually Uniform Logarithmic Zoom
Linear zoom increments ($s_{k+1} = s_k + \Delta s$) feel sluggish at high magnification and violent at low zoom levels. Kiyo applies continuous exponential zoom:
$$s(t) = s_0 \cdot 2^{v_z t}$$
To maintain visual stability, Kiyo enforces the **Stationary Cursor Invariant**: the exact document point $\mathbf{p}_{\text{doc}}$ beneath the mouse pointer must remain stationary on screen throughout the zoom transition:
$$\mathbf{p}_{\text{doc}} = \frac{\mathbf{p}_{\text{cursor}} - \mathbf{T}(t)}{s(t)} = \text{const} \implies \mathbf{T}(t) = \mathbf{p}_{\text{cursor}} - s(t) \cdot \mathbf{p}_{\text{doc}}$$
where $\mathbf{T}(t)$ is the viewport translation vector.

### 7.2. 2.5D Paper Curl vs. Minimal Slide & Accessibility Sync
- **2.5D Cylindrical Paper Curl**: In E-Reader and Comic modes, turning a page triggers a realistic 2.5D vertex shader simulating cylindrical paper deformation, complete with ambient occlusion shadow cast on the underlying sheet.
- **Reduced Motion Compliance**: Before initiating animations, Kiyo queries the FreeDesktop portal or D-Bus property:
  `org.freedesktop.appearance.prefers-reduced-motion`
  If set to `1` (or in remote SSH/X11 sessions), all 2.5D curl and slide transitions are instantaneously disabled in favor of zero-latency atomic buffer swaps.

### 7.3. Selection Loupe with Measurement Crosshairs
Holding `Z` or `Alt + Left Click Drag` activates the **Selection Loupe**:
- A floating circular magnifying lens rendering a $3.0\times$ magnified view of the underlying canvas.
- Overlays precision prepress crosshairs with sub-pixel PDF point dimensions ($w \times h$), font metrics, and live RGB/CMYK pixel readouts.

### 7.4. Hop/Avy Character-Filtered Spatial Jump Tags
Modeled after Emacs Avy and Vim Sneak, Kiyo enables mouse-free navigation across visible pages:
1. The user presses `f` followed by a target character (e.g. `f t`).
2. Kiyo identifies all words on screen starting with `'t'` and overlays a distinct, high-contrast 2-letter mnemonic badge (e.g. `[aa]`, `[ab]`, `[ac]`).
3. Typing the 2-letter badge instantly jumps the cursor and reading ruler to that exact word in **< 1 ms**, eliminating tedious mouse targeting.

### 7.5. Wayland Continuous Pointer & Touchpad Gestures
Leveraging the `zwp_pointer_gestures_v1` protocol, Kiyo handles raw touchpad hardware streams:
- Continuous 2-finger pinch-to-zoom with direct sub-millimeter finger tracking.
- Continuous 2-finger rotation gestures for rotating blueprints and schematics.
- 3-finger horizontal swipe for instantaneous document history traversal (Back / Forward).

### 7.6. Haptic Micro-Clicks
On supported Linux touchpads (e.g. ThinkPad haptic pads, Apple Magic Trackpad under Linux) or via subtle system audio ticks, Kiyo emits micro-haptic clicks when the viewport snaps to 100% zoom, page boundaries, or 90° rotation detents.

---

## 8. Intelligent Whitespace Auto-Cropping & Margin Suppression

Scanned PDFs and print-ready books frequently contain wide outer paper margins designed for physical binding. On laptop and tablet screens, these margins waste up to 40% of visible display area.

```
+---------------------------------------------------------------------------------------------------+
|                        Kiyo Whitespace Auto-Cropping & Header Heuristic                           |
+---------------------------------------------------------------------------------------------------+
|  [Raw Page Scan / PDF Vector Canvas]                                                              |
|  +---------------------------------------------------------------------------------------------+  |
|  | [Wide Outer Margin]                                                                         |  |
|  |            +---------------------------------------------------------------+                |  |
|  |            | Running Header: "Chapter 3: Quantum Chromodynamics" (Page 42) | <--- Detected  |  |
|  |            +---------------------------------------------------------------+      by Hash   |  |
|  |            |                                                               |                |  |
|  |            | Main Typography Block (Meaningful Content)                    |                |  |
|  |            | - Text runs, equations, figures...                            |                |  |
|  |            |                                                               |                |  |
|  |            +---------------------------------------------------------------+                |  |
|  |            | Running Footer: "Confidential Draft - Internal Use Only"       | <--- Detected  |  |
|  |            +---------------------------------------------------------------+                |  |
|  | [Wide Bottom Margin]                                                                        |  |
|  +---------------------------------------------------------------------------------------------+  |
|                                                  |                                                |
|                                                  v                                                |
|  [Aggressive Margin Crop]: Header/Footer Suppressed, Body Content Scaled to 100% Viewport Width   |
+---------------------------------------------------------------------------------------------------+
```

### 8.1. Running Header / Footer Suppression Heuristic
Naive white-box cropping fails on academic papers and books because running headers (e.g. `"Journal of Applied Physics"`) and page numbers prevent vertical margin trimming:
1. `Kiyo` samples glyph bounds across $K = 5$ consecutive pages.
2. Identifies text strings residing within the top and bottom 10% vertical margins.
3. If strings exhibit identical font descriptors, vertical positions $y$, or sequential page numbering syntax (`format!("{n}")`), they are classified as **Peripheral Running Metadata**.
4. The auto-cropper crops past the running header directly to the primary body text block, automatically scaling readable text size by **up to 45%** on smaller screens.

---

## 9. Synchronized Dual-Document Comparison & GPU Bezier Ribbons

Comparing two revisions of an engineering document or legal contract requires instant visual correlation across structural changes.

### 9.1. Three Synchronization Modes
1. **Lock-Step Page Lock**: Viewports for Document A and Document B advance in exact lockstep ($P_B = P_A$).
2. **Proportional Continuous Scrolling**: For documents with disparate page counts (e.g. an expanded 120-page revision vs an 80-page draft), scroll offset is proportionally normalized:
   $$y_B = y_A \cdot \frac{H_B}{H_A}$$
   where $H_A, H_B$ represent total document heights.
3. **Semantic Heading Alignment**: Viewport tracking locks to matching Table of Contents anchors and headings, maintaining alignment even when whole chapters have been inserted or removed.

### 9.2. GPU Translucent Bezier Connector Ribbons
In side-by-side comparison mode, `kiyo-gpu` renders dynamic cubic Bezier ribbons directly in the viewport margin linking corresponding text blocks between Revision A and Revision B:
$$\mathbf{B}(t) = (1-t)^3 \mathbf{P}_0 + 3(1-t)^2 t \mathbf{P}_1 + 3(1-t) t^2 \mathbf{P}_2 + t^3 \mathbf{P}_3$$
- Ribbons are rendered using a translucent WGSL gradient shader (Cyan for Document A anchor $\to$ Magenta for Document B target).
- Visually guides the human eye directly to shifted, re-wrapped, or modified clauses across side-by-side viewports.

---

## 10. Reading Health, Visual Attention Heatmaps & Crash-Safe Session Journaling

### 10.1. The 20-20-20 Eye Strain Prevention Rule
To combat asthenopia during marathon reading sessions, Kiyo incorporates an opt-in ergonomic health daemon:
- Every 20 minutes of active reading, Kiyo gently pulses the viewport luminance down by 15% for 20 seconds.
- Displays an unobtrusive micro-badge in the corner: `"Take a 20-second break: Look at an object 20 feet (6m) away"`.

### 10.2. Visual Reading Heatmap & Dwell-Time Accumulator
Kiyo maintains a spatial dwell-time accumulator in memory:
- Records reader fixation time per paragraph and chapter.
- Overlays a subtle, toggleable visual heatmap in the document scrollbar: hot regions (red/amber) indicate dense sections that required deep re-reading; cool regions (cyan/gray) highlight skimmed or unread material.
- 100% private: stored strictly locally in ephemeral session memory.

### 10.3. Crash-Safe Atomic Session Journaling
To ensure users never lose their place, open portals, or pending form edits during system reboots or power interruptions:
- Kiyo writes session state to a Write-Ahead Log (`.kiyo-session.wal`).
- Serialized using `postcard` (compact binary format) and committed to disk via `atomic-write` + POSIX `rename`.
- On cold launch, Kiyo restores exact viewport coordinates, active portals, bookmarks, and draft AcroForm field entries in **< 3 ms**.

