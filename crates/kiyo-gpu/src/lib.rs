//! `kiyo-gpu`: Hardware-accelerated presentation pipeline, shaders, and raster fallbacks.

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
}
