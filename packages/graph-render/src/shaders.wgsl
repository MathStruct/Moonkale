struct Camera {
    center: vec2<f32>,
    scale: f32,
    mode: f32,          // 0 = 2D, 1 = 3D (Milestone 8)
    viewport: vec2<f32>,
    dist: f32,          // 3D: orbit distance, for size attenuation
    _pad: f32,
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

fn px_to_ndc(px: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(px.x / (camera.viewport.x * 0.5), -px.y / (camera.viewport.y * 0.5));
}

// A world point → (ndc.xy, depth 0..1, clip w) in either mode.
fn project(p: vec3<f32>) -> vec4<f32> {
    if (camera.mode > 0.5) {
        let c = camera.view_proj * vec4<f32>(p, 1.0);
        let w = max(c.w, 1e-4);
        return vec4<f32>(c.x / w, c.y / w, clamp(c.z / w, 0.0, 1.0), w);
    }
    let px = (p.xy - camera.center) * camera.scale;
    return vec4<f32>(px_to_ndc(px), 0.5, 1.0);
}

// Fog for 3D: farther than the target fades toward the background.
fn fog(w: f32) -> f32 {
    if (camera.mode < 0.5) { return 0.0; }
    return clamp((w - camera.dist) / (camera.dist * 1.5), 0.0, 0.75);
}

// ---------------- nodes: instanced SDF circles ----------------
struct NodeInst {
    @location(0) pos: vec3<f32>,
    @location(1) radius: f32,
    @location(2) color: vec4<f32>,
};
struct NodeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) fog: f32,
};

@vertex
fn node_vs(@builtin(vertex_index) vi: u32, inst: NodeInst) -> NodeOut {
    var corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    let c = corners[vi];
    let pr = project(inst.pos);
    // Radius in pixels, never smaller than 2px when zoomed far out; in 3D
    // nearer nodes are bigger.
    var r: f32;
    if (camera.mode > 0.5) {
        r = max(inst.radius * clamp(camera.dist / pr.w, 0.2, 4.0), 2.0) + 1.0;
    } else {
        r = max(inst.radius * camera.scale, 2.0) + 1.0;
    }
    let off = vec2<f32>(c.x * r / (camera.viewport.x * 0.5), c.y * r / (camera.viewport.y * 0.5));
    var out: NodeOut;
    out.clip = vec4<f32>(pr.x + off.x, pr.y + off.y, pr.z, 1.0);
    out.uv = c;
    out.color = inst.color;
    out.fog = fog(pr.w);
    return out;
}

@fragment
fn node_fs(in: NodeOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    let edge = fwidth(d) * 1.5;
    let alpha = 1.0 - smoothstep(1.0 - edge, 1.0, d);
    if (alpha < 0.02) { discard; }
    // slightly darker rim
    let rim = smoothstep(0.7, 1.0, d) * 0.35;
    let bg = vec3<f32>(0.047, 0.055, 0.075);
    let rgb = mix(in.color.rgb * (1.0 - rim), bg, in.fog);
    return vec4<f32>(rgb, in.color.a * alpha);
}

// ---------------- edges: instanced quads along a segment ----------------
struct EdgeInst {
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) color: vec4<f32>,
};
struct EdgeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) fog: f32,
};

@vertex
fn edge_vs(@builtin(vertex_index) vi: u32, inst: EdgeInst) -> EdgeOut {
    let pa = project(inst.a);
    let pb = project(inst.b);
    // Screen-space normal for a constant 1.2px width.
    let sa = vec2<f32>(pa.x * camera.viewport.x * 0.5, -pa.y * camera.viewport.y * 0.5);
    let sb = vec2<f32>(pb.x * camera.viewport.x * 0.5, -pb.y * camera.viewport.y * 0.5);
    let dir = normalize(sb - sa + vec2(1e-4, 0.0));
    let n = vec2<f32>(-dir.y, dir.x) * 0.6;
    var p: vec2<f32>;
    var depth: f32;
    var w: f32;
    switch vi {
        case 0u: { p = sa + n; depth = pa.z; w = pa.w; }
        case 1u: { p = sa - n; depth = pa.z; w = pa.w; }
        case 2u: { p = sb + n; depth = pb.z; w = pb.w; }
        default: { p = sb - n; depth = pb.z; w = pb.w; }
    }
    var out: EdgeOut;
    out.clip = vec4<f32>(px_to_ndc(p), depth, 1.0);
    out.color = inst.color;
    out.fog = fog(w);
    return out;
}

@fragment
fn edge_fs(in: EdgeOut) -> @location(0) vec4<f32> {
    let bg = vec3<f32>(0.047, 0.055, 0.075);
    return vec4<f32>(mix(in.color.rgb, bg, in.fog), in.color.a);
}
