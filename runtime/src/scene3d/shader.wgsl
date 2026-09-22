struct Camera {
    view_proj: mat4x4<f32>,
};
struct GpuCurve {
    p0: vec2<f32>,
    control: vec2<f32>,
    p1: vec2<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> glyph_curves: array<GpuCurve>;

struct In {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) color: vec4<f32>,
    // (curve_offset, curve_count, 0, 0) — this object's slice of the shared `glyph_curves` buffer.
    @location(7) text_curves: vec4<f32>,
    // This object's shaped text, in em-space: (min.x, min.y, max.x, max.y).
    @location(8) text_bounds: vec4<f32>,
};

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) panel_uv: vec2<f32>,
    @location(2) @interpolate(flat) text_curves: vec4<f32>,
    @location(3) @interpolate(flat) text_bounds: vec4<f32>,
    @location(4) @interpolate(flat) has_surface: u32,
    @location(5) @interpolate(flat) face_aspect: f32,
};

@vertex
fn vs_main(input: In) -> Out {
    let model = mat4x4<f32>(input.model_0, input.model_1, input.model_2, input.model_3);
    let world_normal = normalize((model * vec4<f32>(input.normal, 0.0)).xyz);
    let light = normalize(vec3<f32>(0.4, 0.8, 0.6));
    let shade = 0.3 + 0.7 * max(dot(world_normal, light), 0.0);
    // Raw, unwarped face UV — fs_main fits the actual shaped text into it below, preserving its
    // own aspect ratio instead of stretching it to the face's.
    let uv = vec2<f32>(input.position.x + 0.5, 0.5 - input.position.y);
    var out: Out;
    out.position = camera.view_proj * model * vec4<f32>(input.position, 1.0);
    out.color = vec4<f32>(input.color.rgb * shade, input.color.a);
    out.panel_uv = uv;
    out.face_aspect = length(input.model_0.xyz) / length(input.model_1.xyz);
    out.text_curves = input.text_curves;
    out.text_bounds = input.text_bounds;
    out.has_surface = select(0u, 1u, input.normal.z > 0.5 && input.text_curves.y > 0.5);
    return out;
}

// Signed contribution of root `t` to the nonzero winding number at `p`: does this curve cross
// the horizontal ray from `p` to +x at a valid t in [0,1)? `curve_winding` solves for `t`; this
// just evaluates and signs one root, shared by the linear- and quadratic-root callers.
fn curve_crossing(curve: GpuCurve, p: vec2<f32>, t: f32) -> f32 {
    if t < 0.0 || t >= 1.0 {
        return 0.0;
    }
    let mt = 1.0 - t;
    let x = mt * mt * curve.p0.x + 2.0 * mt * t * curve.control.x + t * t * curve.p1.x;
    if x <= p.x {
        return 0.0;
    }
    let dy = 2.0 * mt * (curve.control.y - curve.p0.y) + 2.0 * t * (curve.p1.y - curve.control.y);
    return select(-1.0, 1.0, dy > 0.0);
}

// A degenerate (line) curve has a == 0 exactly — its control point is its own midpoint — so this
// solves linearly rather than dividing by zero in the quadratic formula.
fn curve_winding(curve: GpuCurve, p: vec2<f32>) -> f32 {
    let y0 = curve.p0.y - p.y;
    let yc = curve.control.y - p.y;
    let y1 = curve.p1.y - p.y;
    let a = y0 - 2.0 * yc + y1;
    let b = 2.0 * (yc - y0);
    if abs(a) < 1e-6 {
        if abs(b) < 1e-6 {
            return 0.0;
        }
        return curve_crossing(curve, p, -y0 / b);
    }
    let disc = b * b - 4.0 * a * y0;
    if disc < 0.0 {
        return 0.0;
    }
    let sq = sqrt(disc);
    return curve_crossing(curve, p, (-b - sq) / (2.0 * a))
        + curve_crossing(curve, p, (-b + sq) / (2.0 * a));
}

// Nonzero-winding coverage of this object's text at `glyph_uv` (its own em-space coordinates, not
// the panel's), reading only its slice — `glyph_curves[offset..offset+count]` — of the shared
// buffer. Hard-edged; `fs_main` supersamples this for anti-aliasing.
fn glyph_coverage(glyph_uv: vec2<f32>, offset: u32, count: u32) -> f32 {
    var winding = 0.0;
    for (var i = 0u; i < count; i++) {
        winding += curve_winding(glyph_curves[offset + i], glyph_uv);
    }
    return select(0.0, 1.0, abs(winding) > 0.5);
}

@fragment
fn fs_main(input: Out) -> @location(0) vec4<f32> {
    // Computed before the early-return branch below: fwidth needs uniform control flow across a
    // pixel quad, and panel_uv (unlike glyph_uv below) is always defined, branch or not.
    let panel_delta = fwidth(input.panel_uv);
    if input.has_surface == 0u {
        return input.color;
    }
    // A little breathing room around the shaped text.
    let pad = (input.text_bounds.zw - input.text_bounds.xy) * 0.1;
    let glyph_min = input.text_bounds.xy - pad;
    let glyph_max = input.text_bounds.zw + pad;
    let span = max(glyph_max - glyph_min, vec2<f32>(1e-4));
    // Fit the text into the face preserving ITS OWN aspect ratio (no stretch) — working in
    // "physical" units where the face spans [0, face_aspect] x [0, 1] keeps x and y comparable.
    let face_size = vec2<f32>(input.face_aspect, 1.0);
    let scale = min(face_size.x * 0.92 / span.x, face_size.y * 0.92 / span.y);
    let drawn = span * scale;
    let origin = (face_size - drawn) * 0.5;
    let local = (input.panel_uv * face_size - origin) / scale;
    let glyph_uv = vec2<f32>(glyph_min.x + local.x, glyph_max.y - local.y);
    let offset = u32(input.text_curves.x);
    let count = u32(input.text_curves.y);
    // One pixel's footprint in glyph space. glyph_coverage is a hard 0/1 test, so a single sample
    // aliases; average a 4x4 grid of sub-pixel samples for a fractional edge instead. (4 samples
    // only gives 5 visible coverage levels — a visible staircase; 16 reads as smooth.)
    let px = panel_delta * face_size / scale;
    var coverage = 0.0;
    for (var oy = 0; oy < 4; oy++) {
        for (var ox = 0; ox < 4; ox++) {
            let sample_offset = (vec2<f32>(f32(ox), f32(oy)) + 0.5) / 4.0 - 0.5;
            coverage += glyph_coverage(glyph_uv + sample_offset * px, offset, count);
        }
    }
    coverage /= 16.0;
    return vec4<f32>(mix(input.color.rgb, vec3<f32>(1.0), coverage), input.color.a);
}
