//! `kiyo-gpu`: Hardware-accelerated presentation pipeline, shaders, and raster fallbacks.

#![allow(clippy::excessive_precision, clippy::chunks_exact_to_as_chunks)]

pub const SMART_DARK_SHADER: &str = include_str!("../shaders/smart_dark.wgsl");
pub const VISUAL_DIFF_SHADER: &str = include_str!("../shaders/visual_diff.wgsl");

/// Shader configuration parameters for luminance-preserving Smart Dark Mode.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DarkModeUniforms {
    pub contrast_dampening: f32,
    pub chroma_threshold: f32,
    pub invert_images: u32,
    pub paper_lightness: f32,
}

impl Default for DarkModeUniforms {
    fn default() -> Self {
        Self {
            contrast_dampening: 0.88,
            chroma_threshold: 0.12,
            invert_images: 0,
            paper_lightness: 0.10,
        }
    }
}

/// Converts linear sRGB to perceptual Oklab color space
#[inline]
pub fn srgb_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;

    let l_ = l.max(0.0).cbrt();
    let m_ = m.max(0.0).cbrt();
    let s_ = s.max(0.0).cbrt();

    (
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    )
}

/// Converts Oklab back to linear sRGB
#[inline]
pub fn oklab_to_srgb(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let l_cubed = l_ * l_ * l_;
    let m_cubed = m_ * m_ * m_;
    let s_cubed = s_ * s_ * s_;

    (
        4.0767439362 * l_cubed - 3.3077115913 * m_cubed + 0.2309699292 * s_cubed,
        -1.2684380046 * l_cubed + 2.6097574011 * m_cubed - 0.3413193965 * s_cubed,
        -0.0041960863 * l_cubed - 0.7034186147 * m_cubed + 1.7076147010 * s_cubed,
    )
}

/// In-place perceptual Smart Dark Mode application on RGBA byte slice.
pub fn apply_smart_dark_cpu(pixels: &mut [u8], uniforms: DarkModeUniforms) {
    for chunk in pixels.chunks_exact_mut(4) {
        let r = chunk[0] as f32 / 255.0;
        let g = chunk[1] as f32 / 255.0;
        let b = chunk[2] as f32 / 255.0;

        let (l, a, b_val) = srgb_to_oklab(r, g, b);
        let chroma = (a * a + b_val * b_val).sqrt();

        if chroma < uniforms.chroma_threshold || uniforms.invert_images == 1 {
            let target_l = uniforms.paper_lightness + (1.0 - l) * uniforms.contrast_dampening;
            let (cr, cg, cb) = oklab_to_srgb(target_l, a * 0.5, b_val * 0.5);
            chunk[0] = (cr.clamp(0.0, 1.0) * 255.0).round() as u8;
            chunk[1] = (cg.clamp(0.0, 1.0) * 255.0).round() as u8;
            chunk[2] = (cb.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// Renders a visual XOR difference overlay comparing two RGBA image revisions.
/// Highlights deleted content in red and inserted content in green.
pub fn render_visual_diff(
    img_a: &[u8],
    img_b: &[u8],
    width: u32,
    height: u32,
    threshold: f32,
) -> anyhow::Result<Vec<u8>> {
    let expected_len = (width as usize) * (height as usize) * 4;
    anyhow::ensure!(
        img_a.len() >= expected_len && img_b.len() >= expected_len,
        "Image buffers too small for {}x{} RGBA buffer (got a={}, b={}, expected={})",
        width,
        height,
        img_a.len(),
        img_b.len(),
        expected_len
    );

    let mut out = vec![0u8; expected_len];
    for i in 0..(width as usize * height as usize) {
        let idx = i * 4;
        let a_r = img_a[idx] as f32 / 255.0;
        let a_g = img_a[idx + 1] as f32 / 255.0;
        let a_b = img_a[idx + 2] as f32 / 255.0;

        let b_r = img_b[idx] as f32 / 255.0;
        let b_g = img_b[idx + 1] as f32 / 255.0;
        let b_b = img_b[idx + 2] as f32 / 255.0;

        let lum_a = 0.299 * a_r + 0.587 * a_g + 0.114 * a_b;
        let lum_b = 0.299 * b_r + 0.587 * b_g + 0.114 * b_b;
        let delta = lum_a - lum_b;

        if delta > threshold {
            // Deleted in Revision B: Vivid Red (255, 26, 51)
            out[idx] = 255;
            out[idx + 1] = 26;
            out[idx + 2] = 51;
            out[idx + 3] = 255;
        } else if delta < -threshold {
            // Inserted in Revision B: Vivid Green (26, 230, 102)
            out[idx] = 26;
            out[idx + 1] = 230;
            out[idx + 2] = 102;
            out[idx + 3] = 255;
        } else {
            // Identical: Subdued background
            let bg = lum_a * 0.4 + 0.3;
            let bg_byte = (bg.clamp(0.0, 1.0) * 255.0).round() as u8;
            out[idx] = bg_byte;
            out[idx + 1] = bg_byte;
            out[idx + 2] = bg_byte;
            out[idx + 3] = 255;
        }
    }

    Ok(out)
}

/// Generates a PDF containing the visual difference bitmap as a full-page image.
pub fn generate_diff_pdf(
    diff_pixels: &[u8],
    width: u32,
    height: u32,
    output_path: &std::path::Path,
) -> anyhow::Result<()> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};

    let mut pdf = Pdf::new();

    let page_tree_id = Ref::new(1);
    let page_id = Ref::new(2);
    let image_id = Ref::new(3);
    let content_id = Ref::new(4);
    let catalog_id = Ref::new(5);

    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id).kids([page_id]).count(1);

    let mut page = pdf.page(page_id);
    page.media_box(Rect::new(0.0, 0.0, width as f32, height as f32));
    page.parent(page_tree_id);
    page.contents(content_id);

    let mut resources = page.resources();
    resources.x_objects().pair(Name(b"DiffImg"), image_id);
    resources.finish();
    page.finish();

    // Convert RGBA to RGB for PDF image XObject
    let pixel_count = (width * height) as usize;
    let mut rgb_data = Vec::with_capacity(pixel_count * 3);
    for chunk in diff_pixels.chunks_exact(4).take(pixel_count) {
        rgb_data.push(chunk[0]);
        rgb_data.push(chunk[1]);
        rgb_data.push(chunk[2]);
    }

    let mut image = pdf.image_xobject(image_id, &rgb_data);
    image.width(width as i32);
    image.height(height as i32);
    image.color_space().device_rgb();
    image.bits_per_component(8);
    image.finish();

    let mut content = Content::new();
    content.save_state();
    content.transform([width as f32, 0.0, 0.0, height as f32, 0.0, 0.0]);
    content.x_object(Name(b"DiffImg"));
    content.restore_state();

    pdf.stream(content_id, &content.finish());

    let bytes = pdf.finish();
    std::fs::write(output_path, bytes)?;
    Ok(())
}

/// High performance GPU rendering pipeline managing shaders and textures.
pub struct GpuPresentationPipeline {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub smart_dark_module: wgpu::ShaderModule,
    pub visual_diff_module: wgpu::ShaderModule,
    pub tile_bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}

impl GpuPresentationPipeline {
    /// Creates a presentation pipeline from existing WGPU device and queue.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let smart_dark_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SmartDarkShader"),
            source: wgpu::ShaderSource::Wgsl(SMART_DARK_SHADER.into()),
        });

        let visual_diff_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("VisualDiffShader"),
            source: wgpu::ShaderSource::Wgsl(VISUAL_DIFF_SHADER.into()),
        });

        let tile_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("TileBindGroupLayout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("TileSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            device,
            queue,
            smart_dark_module,
            visual_diff_module,
            tile_bind_group_layout,
            sampler,
        }
    }

    /// Uploads a CPU TileBuffer directly into a newly allocated wgpu::Texture.
    pub fn upload_tile(&self, tile: &kiyo_render::TileBuffer) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: tile.width,
            height: tile.height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("KiyoPageTileTexture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tile.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tile.stride),
                rows_per_image: Some(tile.height),
            },
            size,
        );

        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_pod_size_and_alignment() {
        assert_eq!(std::mem::size_of::<DarkModeUniforms>(), 16);
        assert_eq!(std::mem::align_of::<DarkModeUniforms>(), 4);

        let default_uniforms = DarkModeUniforms::default();
        let bytes = bytemuck::bytes_of(&default_uniforms);
        assert_eq!(bytes.len(), 16);

        let roundtrip: DarkModeUniforms = *bytemuck::from_bytes(bytes);
        assert_eq!(roundtrip, default_uniforms);
    }

    #[test]
    fn test_smart_dark_wgsl_syntax_and_validation() {
        let module = wgpu::naga::front::wgsl::parse_str(SMART_DARK_SHADER)
            .expect("Smart dark mode WGSL shader must parse cleanly without syntax errors");

        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        let _info = validator
            .validate(&module)
            .expect("Smart dark mode WGSL shader must pass Naga semantic validation");
    }

    #[test]
    fn test_visual_diff_wgsl_syntax_and_validation() {
        let module = wgpu::naga::front::wgsl::parse_str(VISUAL_DIFF_SHADER)
            .expect("Visual diff WGSL shader must parse cleanly without syntax errors");

        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        let _info = validator
            .validate(&module)
            .expect("Visual diff WGSL shader must pass Naga semantic validation");
    }

    #[test]
    fn test_apply_smart_dark_cpu_inversion() {
        // Pixel 0: White paper [255, 255, 255, 255] (low chroma, high luminance)
        // Pixel 1: Black ink [0, 0, 0, 255] (low chroma, low luminance)
        // Pixel 2: Saturated Red [255, 0, 0, 255] (high chroma, should NOT invert luminance)
        let mut pixels = vec![255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255];

        let uniforms = DarkModeUniforms::default();
        apply_smart_dark_cpu(&mut pixels, uniforms);

        // White paper should invert to dark charcoal (< 80)
        assert!(
            pixels[0] < 80,
            "White paper R must be dark, got {}",
            pixels[0]
        );
        assert!(
            pixels[1] < 80,
            "White paper G must be dark, got {}",
            pixels[1]
        );
        assert!(
            pixels[2] < 80,
            "White paper B must be dark, got {}",
            pixels[2]
        );

        // Black text should invert to bright/light text (> 180)
        assert!(
            pixels[4] > 180,
            "Black text R must be bright, got {}",
            pixels[4]
        );
        assert!(
            pixels[5] > 180,
            "Black text G must be bright, got {}",
            pixels[5]
        );
        assert!(
            pixels[6] > 180,
            "Black text B must be bright, got {}",
            pixels[6]
        );

        // Saturated red should preserve high red component
        assert!(
            pixels[8] > 200,
            "Saturated red must preserve hue, got R={}",
            pixels[8]
        );
        assert!(
            pixels[9] < 100,
            "Saturated red must preserve low green, got G={}",
            pixels[9]
        );
    }

    #[test]
    fn test_render_visual_diff_logic() {
        // 2x1 pixels:
        // Pixel 0: revision A is white [255, 255, 255, 255], revision B is black [0, 0, 0, 255] (deleted in B -> red)
        // Pixel 1: revision A is black [0, 0, 0, 255], revision B is white [255, 255, 255, 255] (inserted in B -> green)
        let img_a = vec![255, 255, 255, 255, 0, 0, 0, 255];
        let img_b = vec![0, 0, 0, 255, 255, 255, 255, 255];

        let diff = render_visual_diff(&img_a, &img_b, 2, 1, 0.08).expect("Diff rendering succeeds");
        assert_eq!(diff.len(), 8);

        // Pixel 0: deleted in B -> Vivid Red [255, 26, 51, 255]
        assert_eq!(diff[0], 255);
        assert_eq!(diff[1], 26);
        assert_eq!(diff[2], 51);

        // Pixel 1: inserted in B -> Vivid Green [26, 230, 102, 255]
        assert_eq!(diff[4], 26);
        assert_eq!(diff[5], 230);
        assert_eq!(diff[6], 102);

        // Test identical pixels -> subdued background
        let img_c = vec![128, 128, 128, 255];
        let diff_identical =
            render_visual_diff(&img_c, &img_c, 1, 1, 0.08).expect("Identical diff succeeds");
        assert_eq!(diff_identical.len(), 4);
        assert_eq!(diff_identical[0], diff_identical[1]);
        assert_eq!(diff_identical[1], diff_identical[2]);
    }

    #[test]
    fn test_generate_diff_pdf_output() {
        let temp_dir = std::env::temp_dir();
        let pdf_path = temp_dir.join("test_diff_out.pdf");

        let diff_pixels = vec![255, 26, 51, 255, 26, 230, 102, 255];
        generate_diff_pdf(&diff_pixels, 2, 1, &pdf_path).expect("PDF diff generation succeeds");

        assert!(pdf_path.exists());
        let data = std::fs::read(&pdf_path).expect("Read generated PDF");
        assert!(data.starts_with(b"%PDF-"));
        let _ = std::fs::remove_file(pdf_path);
    }
}
