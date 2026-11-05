# Kiyo: System Architecture and Detailed Design

## 1. Executive Vision and System Objectives

`Kiyo` is an ultra-high-performance, modular PDF engine, viewer, and document manipulation suite written in Rust. Conceived as a sibling to `Klip` (cross-platform clipboard sync daemon) and `Kiwi` (sub-4MB network intelligence CLI), `Kiyo` solves the chronic fragmentation, sluggishness, and poor Linux integration of modern PDF tooling.

Traditional PDF viewers on Linux (Evince, Okular, Zathura, Sioyek) either:
1. Suffer from single-threaded UI rendering lockups during heavy Poppler/MuPDF rasterization.
2. Rely on naive color inversion that destroys figures, charts, and photographs in dark mode.
3. Lack unified headless CLI and terminal graphics (TUI) interfaces.
4. Have zero integrated, offline, zero-dependency neural OCR engines for scanned documents.

`Kiyo` eliminates these compromises through a 7-crate workspace architecture combining hardware-accelerated GPU raster presentation (`wgpu`), speculative asynchronous tile caching, a multi-process worker pool to circumvent C-library mutex locks, pure-Rust fallback parsers, and native Linux desktop citizenship.

```
                                  +-------------------------------------------------------+
                                  |                     User Surfaces                     |
                                  +-------------------------------------------------------+
                                  |    kiyo-gui (wgpu)   |  kiyo-tui (ratatui) | kiyo-cli |
                                  +----------------------+---------------------+----------+
                                             |                      |               |
                                             v                      v               v
+-----------------------------------------------------------------------------------------+
|                                    kiyo-render                                          |
|  - RenderBackend Trait         - Viewport Manager           - Speculative TileCache     |
|  - Coordinate Transformer      - Page Layout Orchestrator   - Priority Tile Scheduler   |
+-----------------------------------------------------------------------------------------+
       |                                      |                                  |
       v                                      v                                  v
+-----------------------+   +------------------------------------+   +--------------------+
|       kiyo-gpu        |   |         kiyo-worker (IPC)          |   |     kiyo-ocr       |
| - wgpu 24/30 Pipeline |   | - Multi-Process PDFium Worker Pool |   | - ocrs 0.13        |
| - Smart Dark Shader   |   | - Shared Memory (memfd_create)     |   | - rten 0.27        |
| - Visual Diff Shader  |   | - UDS IPC Control Plane            |   | - ISO 32000 (3 Tr) |
| - tiny-skia Fallback  |   | - Pure Rust hayro Fallback         |   | - Bounding Box Gen |
+-----------------------+   +------------------------------------+   +--------------------+
                                              |
                                              v
+-----------------------------------------------------------------------------------------+
|                                     kiyo-core                                           |
| - Zero-Copy Memory Mapping (memmap2)         - Object Cross-Reference Graph (lopdf)   |
| - Streaming PDF Writer (pdf-writer)          - Vector Path Compiler (krilla)            |
| - SyncTeX Parser & Reverse Mapper            - Structural XY-Cut Text Layout Analyzer  |
+-----------------------------------------------------------------------------------------+
```

---

## 2. Workspace Crate Topology (The 7 Crates)

The Kiyo codebase is segregated into seven specialized crates within a unified Cargo workspace. Boundaries between crates enforce strict separation of concerns, rapid compilation times, and headless reusability.

```
Kiyo/
├── Cargo.toml                  # Workspace root manifest
├── docs/                       # Architectural blueprints and engineering specifications
└── crates/
    ├── kiyo-core/              # Core PDF document model, parsing, mutation, xref, and SyncTeX
    ├── kiyo-render/            # Abstract rendering engine, tile cache, and viewport manager
    ├── kiyo-gpu/               # wgpu presentation pipeline, WGSL shaders, and softbuffer fallback
    ├── kiyo-ocr/               # Embedded neural OCR inference engine (ocrs + rten)
    ├── kiyo-gui/               # High-framerate desktop graphical viewer (winit + wgpu)
    ├── kiyo-tui/               # Terminal graphical viewer (ratatui + Kitty/Sixel/iTerm2)
    └── kiyo-cli/               # Headless CLI toolkit (merge, split, diff, ocr, extract, watch)
```

### 2.1. `kiyo-core`
- **Purpose**: Low-level document representation, object streaming, cross-reference (xref) reconstruction, vector graphics generation, and text layout analysis.
- **Key Dependencies**: `memmap2`, `lopdf`, `pdf-writer`, `krilla`, `rayon`, `parking_lot`.
- **Key Responsibilities**:
  - Memory-mapping PDF files via `memmap2::Mmap` for sub-millisecond document loading regardless of file size (100MB+ documents map in < 1ms).
  - Parsing corrupted xref tables with repair heuristics.
  - Exposing an immutable document DOM with copy-on-write (CoW) page tree manipulation.
  - Bidirectional SyncTeX line/column $\leftrightarrow$ PDF coordinate translation.
  - XY-cut recursive spatial decomposition of glyph runs for reading-order extraction and table reconstruction.

### 2.2. `kiyo-render`
- **Purpose**: Viewport-aware rendering abstraction, coordinate space mapping, and predictive speculative tile caching.
- **Key Dependencies**: `crossbeam-channel`, `image`, `bytemuck`, `kiyo-core`.
- **Key Responsibilities**:
  - Defining the fundamental `RenderBackend` trait.
  - Managing continuous document layouts (single-page, double-page facing, continuous vertical scroll, horizontal flow).
  - High-precision coordinate transformations:
    $$\begin{pmatrix} x_{\text{screen}} \\ y_{\text{screen}} \\ 1 \end{pmatrix} = \begin{pmatrix} s_x & 0 & t_x \\ 0 & -s_y & t_y \\ 0 & 0 & 1 \end{pmatrix} \begin{pmatrix} x_{\text{pdf}} \\ y_{\text{pdf}} \\ 1 \end{pmatrix}$$
  - Tile-based rendering architecture dividing pages into $512 \times 512$ px tiles with mipmapped Level of Detail (LoD) downsampling.
  - Speculative prefetching: predicting scroll velocity ($v_y = \frac{\Delta y}{\Delta t}$) and pre-rendering tiles in the trajectory vector before they enter the visible frustum.

### 2.3. `kiyo-gpu`
- **Purpose**: Hardware-accelerated presentation pipeline, custom WGSL compute/fragment shaders, texture streaming, and software raster fallback.
- **Key Dependencies**: `wgpu`, `winit`, `softbuffer`, `tiny-skia`, `bytemuck`.
- **Key Responsibilities**:
  - 3-tier presentation pipeline:
    1. **Primary**: Vulkan 1.2+ backend on Linux (Wayland / X11).
    2. **Secondary**: OpenGL ES 3.0 / EGL fallback for legacy GPUs and virtualized environments (QEMU/KVM).
    3. **Tertiary**: CPU rasterization via `tiny-skia` and direct frame presentation via `softbuffer`.
  - Custom WGSL Fragment Shaders:
    - **Luminance-Preserving Smart Dark Mode**: Real-time Oklab color-space inversion dampening pure whites to charcoal, preserving chrominance of images and vector syntax colors.
    - **Visual XOR Difference Shader**: Real-time structural and typographical delta visualization between two PDF revisions (Revision A in cyan, Revision B in magenta, overlaps in neutral gray).
  - Ring buffer texture streaming: Zero-copy upload of worker-rendered tiles to GPU memory via pinned staging buffers.

### 2.4. `kiyo-ocr`
- **Purpose**: Embedded, 100% offline, pure-Rust neural OCR inference and searchable PDF generation.
- **Key Dependencies**: `ocrs`, `rten`, `rten-tensor`, `image`, `kiyo-core`.
- **Key Responsibilities**:
  - Offline text detection via DBNet (Real-time Scene Text Detection with Differentiable Binarization).
  - Offline text recognition via CRNN (Convolutional Recurrent Neural Network with CTC loss).
  - Generating ISO 32000-1 compliant "Sandwich PDFs":
    - Injecting invisible text operators using Text Rendering Mode 3 (`3 Tr`).
    - Exact spatial bounding box alignment matching detected text coordinates.
    - Preserving original scanned bitmaps while enabling native text selection, search, and copy-paste.

### 2.5. `kiyo-gui`
- **Purpose**: Ultra-responsive desktop graphical application.
- **Key Dependencies**: `kiyo-core`, `kiyo-render`, `kiyo-gpu`, `winit`, `ashpd`, `x11rb`, `zbus`, `notify`.
- **Key Responsibilities**:
  - High-framerate event loop delivering stutter-free 120Hz/144Hz scrolling.
  - Non-intrusive floating secondary viewports ("Portals") for simultaneous inspection of mathematical equations, footnotes, and bibliography references.
  - Wayland `wp_fractional_scale_v1` integration for pixel-perfect HiDPI text.
  - Instant X11 `PRIMARY` selection population on text highlight.
  - System D-Bus theme detection (`org.freedesktop.appearance.color-scheme`).
  - Atomic live reload integration: listening to file modification events via `notify` and hot-swapping document buffers without resetting scroll or zoom state.

### 2.6. `kiyo-tui`
- **Purpose**: Full-featured terminal PDF viewer operating directly inside modern terminal emulators.
- **Key Dependencies**: `ratatui`, `crossterm`, `ratatui-image`, `kiyo-core`, `kiyo-render`.
- **Key Responsibilities**:
  - Multi-protocol graphics rendering:
    - **Kitty Graphics Protocol**: High-bandwidth inline raster display.
    - **Sixel Graphics**: Universal legacy terminal graphics (Foot, WezTerm, XTerm).
    - **iTerm2 Protocol**: Inline image escape codes.
    - **ANSI Half-Block (`▀`/`▄`) + 24-bit Truecolor**: Graceful fallback on minimal terminals.
  - Modal Vim navigation model (`j`/`k`, `d`/`u`, `gg`/`G`, mark jumps, `/` instant regex search).

### 2.7. `kiyo-cli`
- **Purpose**: High-throughput scriptable CLI binary for automated document workflows.
- **Key Dependencies**: `clap`, `serde`, `serde_json`, `kiyo-core`, `kiyo-ocr`, `rayon`.
- **Key Responsibilities**:
  - Headless document manipulation subcommands:
    - `kiyo merge`: Zero-copy catalog merging and outline splicing.
    - `kiyo split`: Range extraction, chapter segmentation, and page bursting.
    - `kiyo diff`: Typographical and structural document delta engine.
    - `kiyo ocr`: Batch searchable PDF generation.
    - `kiyo extract`: High-fidelity Markdown, plain text, and embedded asset extraction.
    - `kiyo watch`: Compiler daemon mode for real-time Typst/LaTeX feedback loops.

---

## 3. The Multi-Process Worker Pool (Bypassing PDFium's Global Mutex)

### 3.1. The Concurrency Bottleneck in C-Based PDF Engines
The Google PDFium library (`pdfium-render` crate) is the industry standard for PDF rasterization fidelity. However, PDFium contains extensive internal global state and non-reentrant font rendering locks. In a standard multi-threaded Rust process:
- All calls to `FPDF_RenderPageBitmap` must be serialized across a global process-wide mutex.
- Multi-threaded rasterization across a 16-core CPU yields negligible speedup due to catastrophic lock contention.
- If PDFium encounters a malformed font or corrupted raster stream that triggers a `SIGSEGV` or abort, the entire viewer process crashes.

### 3.2. Architecture of `kiyo-worker`
`Kiyo` solves this via a isolated multi-process worker pool architecture. Rather than running PDFium in worker threads, `Kiyo` spawns dedicated worker child processes (`kiyo-worker`).

```
+-----------------------------------------------------------------------------------------+
|                                    Kiyo Main Process                                    |
|                                                                                         |
|   +-----------------------+                    +------------------------------------+   |
|   |  Render Coordinator   |                    |           TileCache Table          |   |
|   +-----------------------+                    +------------------------------------+   |
|               |                                                  ^                      |
|               | (Dispatch Task via UDS)                          | (Tile Available)     |
|               v                                                  |                      |
|   +---------------------------------------------------------------------------------+   |
|   |                Unix Domain Socket Control Channels (Non-blocking)               |   |
+---+---------------------------------------------------------------------------------+---+
               |                                                  |
        +------+------+                                    +------+------+
        | UDS Frame   |                                    | UDS Frame   |
        v             v                                    v             v
+-------------------------------+                  +-------------------------------+
|     Worker Process 1          |                  |     Worker Process N          |
|  - Isolated PDFium Instance   |                  |  - Isolated PDFium Instance   |
|  - Independent Memory Space   |                  |  - Independent Memory Space   |
|  - Zero Mutex Contention      |                  |  - Zero Mutex Contention      |
+-------------------------------+                  +-------------------------------+
        |                                                  |
        | (Renders directly into SHM)                      | (Renders directly into SHM)
        v                                                  v
+-----------------------------------------------------------------------------------------+
|                  Shared Memory Segment (memfd_create / POSIX shm)                       |
|   [ Slot 0: 512x512 RGBA8 ]   [ Slot 1: 512x512 RGBA8 ]   [ Slot N: 512x512 RGBA8 ]     |
+-----------------------------------------------------------------------------------------+
        |
        | (Main process maps SHM directly into GPU staging buffer)
        v
+-----------------------------------------------------------------------------------------+
|                                  wgpu Texture Upload                                    |
+-----------------------------------------------------------------------------------------+
```

### 3.3. IPC Protocol and Shared Memory Mechanism
1. **Shared Memory Allocation**:
   - The main process creates an anonymous shared memory descriptor using `memfd_create(2)` on Linux with `MFD_CLOEXEC` and `MFD_ALLOW_SEALING`.
   - The buffer is partitioned into fixed-size tile slots ($512 \times 512 \times 4\text{ bytes} = 1\text{ MiB}$ per slot).
   - The file descriptor is passed to worker processes over Unix Domain Sockets using `SCM_RIGHTS` ancillary control messages (`sendmsg(2)`).
2. **Task Dispatch Protocol**:
   - A task packet is 32 bytes binary (zero JSON/serde overhead):
     ```rust
     #[repr(C)]
     #[derive(Copy, Clone, Debug)]
     pub struct RenderTaskMsg {
         pub task_id: u64,
         pub page_index: u32,
         pub tile_x: u16,
         pub tile_y: u16,
         pub tile_w: u16,
         pub tile_h: u16,
         pub zoom_level_fixed: u32, // Fixed-point 16.16 zoom factor
         pub shm_slot_index: u32,
         pub flags: u32,            // Dark mode hints, text render flags
     }
     ```
3. **Execution**:
   - The worker process receives the message, opens the shared memory slot, renders the specified tile directly into the shared memory slice without memory copying, and writes a 16-byte completion response back to the socket.
4. **Crash Isolation**:
   - If a corrupted PDF page causes an engine panic or segfault in a worker, only that single worker dies. The main process detects `EPOLLHUP` on the worker socket, restarts the child process immediately, marks the tile as corrupted/failed, and continues operating without interrupting the user.

---

## 4. The Pure-Rust Fallback Engine (`hayro`)

While PDFium offers complete Adobe Acrobat rendering compatibility, it introduces a dynamic C++ dependency. For ultra-lightweight environments, musl-static targets, embedded systems, or security-sensitive hermetic sandboxes, `Kiyo` provides an integrated pure-Rust rendering pipeline:

```rust
pub enum ActiveEngine {
    /// Multi-process hardware-isolated PDFium pool (Maximum spec fidelity)
    PdfiumPool,
    /// Pure Rust vector parser and software rasterizer (Zero C++ dependency)
    HayroNative,
}
```

### 4.1. The `hayro` Raster Architecture
1. **Stream Decompression**: Native Rust implementations of FlateDecode (`flate2`), RunLengthDecode, CCITTFaxDecode, and DCTDecode (`image::jpeg`).
2. **Font Interpretation**: Embedded glyph extraction supporting TrueType, OpenType, and Type 1 fonts via `owned-ttf-parser` and custom CFF (Compact Font Format) decoders.
3. **Vector Path Pipeline**: PDF vector operators (`m`, `l`, `c`, `v`, `y`, `re`, `h`, `W*`) are transformed directly into `tiny-skia` path descriptors.
4. **Color Management**: Direct conversion of DeviceRGB, DeviceCMYK, DeviceGray, and ICC-based color spaces to sRGB using pure-Rust transfer functions.

---

## 5. Actor Channel Topologies and Concurrency Topology

Kiyo's runtime is governed by an asynchronous Actor pattern using bounded channels (`crossbeam-channel` for low-latency thread-to-thread communication, `tokio::sync::mpsc` for async I/O).

```
                      +-----------------------------+
                      |      OS Event Loop          |
                      |   (winit / crossterm)       |
                      +-----------------------------+
                                     |
                                     | InputEvent (Scroll, Zoom, Key, Resize)
                                     v
                      +-----------------------------+
                      |   Document Viewport Actor   | <-----------+
                      +-----------------------------+             |
                                     |                            |
                         ViewportState (Visible Frustum)          | TileReady Event
                                     v                            |
                      +-----------------------------+             |
                      |    Tile Scheduler Actor     |             |
                      +-----------------------------+             |
                         |                       |                |
     Cache Hit (Immediate)                       | Cache Miss     |
                         v                       v                |
        +--------------------------+    +-----------------------+ |
        | GPU Queue Upload Pass    |    | Worker Pool Dispatcher| |
        +--------------------------+    +-----------------------+ |
                                                   |              |
                                          Render Request          |
                                                   v              |
                                        +-----------------------+ |
                                        | kiyo-worker (Process) |-+
                                        +-----------------------+
```

### 5.1. Backpressure and Frustum Cancellation
A common defect in PDF viewers is "scroll backlog": when a user rapidly scrolls across 50 pages, the rendering queue piles up hundreds of stale page rendering requests. By the time the user stops at Page 50, the CPU is still burning cycles rendering Pages 2 through 49.

`Kiyo` implements **Epoch-Based Frustum Cancellation**:
- Every viewport change increments a 64-bit `FrustumEpoch`.
- Tile rendering requests carry the `FrustumEpoch` at time of dispatch.
- When a worker prepares to render a tile, it verifies if the tile's coordinates still intersect the latest visible frustum (including speculative lookahead buffer).
- If the tile has scrolled out of bounds, the worker instantly drops the task with an `Evicted` status, wasting zero milliseconds rasterizing dead pixels.

---

## 6. Concrete Trait Interfaces

Below are the normative Rust interfaces defined in `kiyo-render` and `kiyo-core`.

```rust
// crates/kiyo-render/src/backend.rs

use std::fmt::Debug;
use std::sync::Arc;
use anyhow::Result;

/// Normalized PDF coordinate point (Points: 1/72 inch, origin at bottom-left).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PdfPoint {
    pub x: f32,
    pub y: f32,
}

/// Normalized PDF bounding box rectangle.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PdfRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

/// Physical screen viewport pixel coordinates (Origin at top-left).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub page_index: u32,
    pub level_of_detail: u8, // Mipmap level (0 = 100%, 1 = 50%, etc.)
    pub tile_col: u32,
    pub tile_row: u32,
}

/// Raw RGBA8 raster tile buffer.
pub struct TileBuffer {
    pub key: TileKey,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: Vec<u8>,
}

/// Text glyph layout extracted from document streams.
#[derive(Clone, Debug)]
pub struct TextRun {
    pub text: String,
    pub rect: PdfRect,
    pub font_name: String,
    pub font_size: f32,
}

/// Search match occurrence.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub page_index: u32,
    pub bounding_boxes: Vec<PdfRect>,
    pub matched_text: String,
    pub context_snippet: String,
}

/// The core abstract rendering backend interface.
pub trait RenderBackend: Send + Sync {
    /// Returns total page count.
    fn page_count(&self) -> u32;

    /// Retrieves dimensional page metrics (in PDF points).
    fn page_dimensions(&self, page_index: u32) -> Result<(f32, f32)>;

    /// Synchronously or asynchronously renders a single tile into memory.
    fn render_tile(
        &self,
        key: TileKey,
        zoom_factor: f32,
    ) -> Result<TileBuffer>;

    /// Extracts structural text runs for text selection and clipboard operations.
    fn extract_text_runs(&self, page_index: u32) -> Result<Vec<TextRun>>;

    /// Executes SIMD-accelerated text search across pages.
    fn search(
        &self,
        query: &str,
        case_sensitive: bool,
    ) -> Result<Vec<SearchResult>>;
}
```

```rust
// crates/kiyo-gpu/src/pipeline.rs

use wgpu::*;

/// High performance GPU rendering pipeline managing shaders and textures.
pub struct GpuPresentationPipeline {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'static>,
    pub surface_config: SurfaceConfiguration,
    pub smart_dark_pipeline: RenderPipeline,
    pub visual_diff_pipeline: RenderPipeline,
    pub tile_bind_group_layout: BindGroupLayout,
    pub sampler: Sampler,
}

impl GpuPresentationPipeline {
    /// Uploads a CPU TileBuffer directly into an existing or newly allocated wgpu::Texture.
    pub fn upload_tile(&self, tile: &kiyo_render::TileBuffer) -> TextureView {
        let size = Extent3d {
            width: tile.width,
            height: tile.height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("KiyoPageTileTexture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &tile.pixels,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(tile.stride),
                rows_per_image: Some(tile.height),
            },
            size,
        );

        texture.create_view(&TextureViewDescriptor::default())
    }
}
```

---

## 7. Memory Budget and Resource Lifecycles

| Subsystem | Allocation Strategy | Hard Limit | Eviction Policy |
| :--- | :--- | :--- | :--- |
| **PDF File Storage** | Virtual memory mapped via `memmap2` | Unlimited (VirtMem) | Managed by OS kernel page cache |
| **Tile Cache (RAM)** | Pinned RGBA8 byte arrays in heap | 256 MiB | Least Recently Used (LRU) with frustum-distance weighting |
| **GPU Texture VRAM** | `wgpu::Texture` 2D array / atlases | 512 MiB | LRU; unreferenced tiles destroyed on frame completion |
| **Shared Memory IPC** | `memfd_create` ring buffers | 64 MiB ($64 \times 1\text{ MiB}$ slots) | Reused circularly on task completion |
| **OCR Weights** | Static mmap of ONNX model files | ~45 MiB | Kept loaded in read-only mapped pages |
| **Total Base RSS** | Dynamic memory footprint | **< 30 MiB** (idle) | Sub-15 MiB in minimal CLI/TUI modes |
