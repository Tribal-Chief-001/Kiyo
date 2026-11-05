// Visual XOR Difference WGSL Shader
// Highlights deleted content in red and inserted content in green

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@group(0) @binding(0) var t_revision_a: texture_2d<f32>;
@group(0) @binding(1) var t_revision_b: texture_2d<f32>;
@group(0) @binding(2) var s_sampler: sampler;

@fragment
fn fs_diff(in: VertexOutput) -> @location(0) vec4<f32> {
    let a = textureSample(t_revision_a, s_sampler, in.tex_coords).rgb;
    let b = textureSample(t_revision_b, s_sampler, in.tex_coords).rgb;

    // Luminance computation (ITU-R BT.601)
    let lum_a = dot(a, vec3<f32>(0.299, 0.587, 0.114));
    let lum_b = dot(b, vec3<f32>(0.299, 0.587, 0.114));

    let delta = lum_a - lum_b;

    // Element removed in Revision B (present in A, absent in B)
    if (delta > 0.08) {
        return vec4<f32>(1.0, 0.1, 0.2, 1.0); // Vivid Red
    }
    // Element inserted in Revision B (absent in A, present in B)
    if (delta < -0.08) {
        return vec4<f32>(0.1, 0.9, 0.4, 1.0); // Vivid Green
    }

    // Identical content: render as subdued background
    let bg = lum_a * 0.4 + 0.3;
    return vec4<f32>(bg, bg, bg, 1.0);
}
