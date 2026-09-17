struct Camera {
    center: vec2<f32>,
    scale: f32,
    _pad: f32,
    viewport: vec2<f32>,
    _pad2: vec2<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

fn world_to_clip(p: vec2<f32>) -> vec2<f32> {
    let px = (p - camera.center) * camera.scale;              // pixels from viewport centre
    return vec2<f32>(px.x / (camera.viewport.x * 0.5), -px.y / (camera.viewport.y * 0.5));
}

// ---------------- nodes: instanced SDF circles ----------------
struct NodeInst {
    @location(0) pos: vec2<f32>,
    @location(1) radius: f32,
    @location(2) color: vec4<f32>,
};
struct NodeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn node_vs(@builtin(vertex_index) vi: u32, inst: NodeInst) -> NodeOut {
    var corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    let c = corners[vi];
    // Radius in pixels, never smaller than 2px when zoomed far out.
    let r = max(inst.radius * camera.scale, 2.0) + 1.0;
    let centre = (inst.pos - camera.center) * camera.scale;
    let px = centre + c * r;
    var out: NodeOut;
    out.clip = vec4<f32>(px.x / (camera.viewport.x * 0.5), -px.y / (camera.viewport.y * 0.5), 0.0, 1.0);
    out.uv = c;
    out.color = inst.color;
    return out;
}

@fragment
fn node_fs(in: NodeOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    let edge = fwidth(d) * 1.5;
    let alpha = 1.0 - smoothstep(1.0 - edge, 1.0, d);
    // slightly darker rim
    let rim = smoothstep(0.7, 1.0, d) * 0.35;
    return vec4<f32>(in.color.rgb * (1.0 - rim), in.color.a * alpha);
}

// ---------------- edges: instanced quads along a segment ----------------
struct EdgeInst {
    @location(0) a: vec2<f32>,
    @location(1) b: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct EdgeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn edge_vs(@builtin(vertex_index) vi: u32, inst: EdgeInst) -> EdgeOut {
    let a = (inst.a - camera.center) * camera.scale;
    let b = (inst.b - camera.center) * camera.scale;
    let dir = normalize(b - a + vec2(1e-4, 0.0));
    let n = vec2<f32>(-dir.y, dir.x) * 0.6;   // half width in px
    var p: vec2<f32>;
    switch vi {
        case 0u: { p = a + n; }
        case 1u: { p = a - n; }
        case 2u: { p = b + n; }
        default: { p = b - n; }
    }
    var out: EdgeOut;
    out.clip = vec4<f32>(p.x / (camera.viewport.x * 0.5), -p.y / (camera.viewport.y * 0.5), 0.0, 1.0);
    out.color = inst.color;
    return out;
}

@fragment
fn edge_fs(in: EdgeOut) -> @location(0) vec4<f32> {
    return in.color;
}
