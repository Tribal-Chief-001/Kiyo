// Smart Dark Mode WGSL Shader
// Perceptual Oklab luminance inversion preserving chromatic images and figures

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;

struct DarkModeUniforms {
    contrast_dampening: f32, // Default 0.88
    chroma_threshold: f32,   // Default 0.12
    invert_images: u32,      // 0 = preserve images, 1 = invert all
    paper_lightness: f32,    // Default 0.10 (charcoal)
};

@group(1) @binding(0) var<uniform> uniforms: DarkModeUniforms;

// Converts linear sRGB to perceptual Oklab color space
fn srgb_to_oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;

    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);

    return vec3<f32>(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_
    );
}

// Converts Oklab back to linear sRGB
fn oklab_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let l_ = c.x + 0.3963377774 * c.y + 0.2158037573 * c.z;
    let m_ = c.x - 0.1055613458 * c.y - 0.0638541728 * c.z;
    let s_ = c.x - 0.0894841775 * c.y - 1.2914855480 * c.z;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    return vec3<f32>(
        4.0767439362 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let raw_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let oklab = srgb_to_oklab(raw_color.rgb);
    let chroma = sqrt(oklab.y * oklab.y + oklab.z * oklab.z);

    // If pixel is near-achromatic (paper background or body text)
    if (chroma < uniforms.chroma_threshold || uniforms.invert_images == 1u) {
        // Invert luminance: high white paper (L ~ 1.0) maps to charcoal background
        let target_l = uniforms.paper_lightness + (1.0 - oklab.x) * uniforms.contrast_dampening;
        let modified_oklab = vec3<f32>(target_l, oklab.y * 0.5, oklab.z * 0.5);
        let corrected_srgb = clamp(oklab_to_srgb(modified_oklab), vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(corrected_srgb, raw_color.a);
    }

    // High chroma element (color chart, figure, photograph): preserve original hue
    return raw_color;
}
