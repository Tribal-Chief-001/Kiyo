//! `kiyo-gpu`: Hardware-accelerated presentation pipeline, shaders, and raster fallbacks.

pub const SMART_DARK_SHADER: &str = include_str!("../shaders/smart_dark.wgsl");

/// Shader configuration parameters.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
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
