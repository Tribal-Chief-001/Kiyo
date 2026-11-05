// Smart Dark Mode WGSL Shader
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;

struct DarkModeUniforms {
    contrast_dampening: f32,
    chroma_threshold: f32,
    invert_images: u32,
    paper_lightness: f32,
};

@group(1) @binding(0) var<uniform> uniforms: DarkModeUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let raw = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    return raw;
}
