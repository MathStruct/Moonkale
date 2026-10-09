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
// Two instance buffers (spec 031, stage 0): slot 0 = geometry (pos, uploaded
// while the layout runs), slot 1 = appearance (colour + radius + the selected
// ring, uploaded on a graph swap, a hover or a selection).
struct NodeInst {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
    @location(3) selected: f32,
};
struct NodeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) fog: f32,
    @location(3) selected: f32,
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
    out.selected = inst.selected;
    return out;
}

@fragment
fn node_fs(in: NodeOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    let edge = fwidth(d) * 1.5;
    let alpha = 1.0 - smoothstep(1.0 - edge, 1.0, d);
    if (alpha < 0.02) { discard; }
    // A selected node wears a light ring; an unselected one a darker rim.
    let rim = smoothstep(0.7, 1.0, d) * 0.35;
    let ring = mix(in.color.rgb * (1.0 - rim), vec3<f32>(1.0), rim * 0.9);
    let bg = vec3<f32>(0.047, 0.055, 0.075);
    let rgb = select(mix(in.color.rgb * (1.0 - rim), bg, in.fog),
                     mix(ring, bg, in.fog),
                     in.selected > 0.5);
    return vec4<f32>(rgb, in.color.a * alpha);
}

// ---------------- segments: instanced quads with width and dash ----------------
struct EdgeInst {
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) width: f32,
    @location(4) dash: f32,
    @location(5) phase: f32,   // arc length at the segment's start (world)
};
struct EdgeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) fog: f32,
    @location(2) coord: f32,    // distance along the stroke, screen pixels
    @location(3) dash: f32,
};

@vertex
fn edge_vs(@builtin(vertex_index) vi: u32, inst: EdgeInst) -> EdgeOut {
    let pa = project(inst.a);
    let pb = project(inst.b);
    // Screen-space normal for the instance's width (default 1.2px).
    let sa = vec2<f32>(pa.x * camera.viewport.x * 0.5, -pa.y * camera.viewport.y * 0.5);
    let sb = vec2<f32>(pb.x * camera.viewport.x * 0.5, -pb.y * camera.viewport.y * 0.5);
    let seg = sb - sa;
    let dir = normalize(seg + vec2(1e-4, 0.0));
    let n = vec2<f32>(-dir.y, dir.x) * max(inst.width, 0.5) * 0.5;
    var p: vec2<f32>;
    var depth: f32;
    var w: f32;
    var t: f32;
    switch vi {
        case 0u: { p = sa + n; depth = pa.z; w = pa.w; t = 0.0; }
        case 1u: { p = sa - n; depth = pa.z; w = pa.w; t = 0.0; }
        case 2u: { p = sb + n; depth = pb.z; w = pb.w; t = 1.0; }
        default: { p = sb - n; depth = pb.z; w = pb.w; t = 1.0; }
    }
    var out: EdgeOut;
    out.clip = vec4<f32>(px_to_ndc(p), depth, 1.0);
    out.color = inst.color;
    out.fog = fog(w);
    // The dash continues across a curve's sub-segments: each carries the
    // arc length at its start. The phase is world; the length is screen —
    // the camera scale converts (2D; in 3D it is an approximation).
    out.coord = t * length(seg) + inst.phase * camera.scale;
    out.dash = inst.dash;
    return out;
}

@fragment
fn edge_fs(in: EdgeOut) -> @location(0) vec4<f32> {
    if (in.dash > 0.5) {
        // 50 % duty cycle along the stroke, in screen pixels.
        if (fract(in.coord / in.dash) < 0.5) { discard; }
    }
    let bg = vec3<f32>(0.047, 0.055, 0.075);
    return vec4<f32>(mix(in.color.rgb, bg, in.fog), in.color.a);
}

// ---------------- arrowheads: rotated triangle billboards ----------------
struct ArrowInst {
    @location(0) pos: vec3<f32>,
    @location(1) angle: f32,
    @location(2) color: vec4<f32>,
    @location(3) size: f32,
};
struct ArrowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,    // rotated: +x is the pointing direction
    @location(1) color: vec4<f32>,
    @location(2) fog: f32,
};

@vertex
fn arrow_vs(@builtin(vertex_index) vi: u32, inst: ArrowInst) -> ArrowOut {
    var corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    let c = corners[vi];
    let pr = project(inst.pos);
    var s: f32;
    if (camera.mode > 0.5) {
        s = max(inst.size * clamp(camera.dist / pr.w, 0.2, 4.0), 3.0) + 1.0;
    } else {
        s = max(inst.size * camera.scale, 3.0) + 1.0;
    }
    // Rotate the billboard so its +x is the pointing direction. The angle
    // is measured in the world plane (y down) while NDC y is up, so the
    // sine flips — without it every head points screen-right. The shape
    // keeps the *unrotated* corner as uv: the quad rotates, the triangle
    // reads in it, and the apex lands along the direction.
    let sn = -sin(inst.angle);
    let cs = cos(inst.angle);
    let rc = vec2<f32>(c.x * cs - c.y * sn, c.x * sn + c.y * cs);
    let off = vec2<f32>(rc.x * s / (camera.viewport.x * 0.5), rc.y * s / (camera.viewport.y * 0.5));
    var out: ArrowOut;
    out.clip = vec4<f32>(pr.x + off.x, pr.y + off.y, pr.z, 1.0);
    out.uv = c;
    out.color = inst.color;
    out.fog = fog(pr.w);
    return out;
}

@fragment
fn arrow_fs(in: ArrowOut) -> @location(0) vec4<f32> {
    // A triangle pointing along +x: apex at (0.5, 0), base at x = -0.5.
    let soft = max(fwidth(in.uv.x), fwidth(in.uv.y)) * 1.5;
    let ax = smoothstep(0.5 + soft, 0.5 - soft, in.uv.x) * smoothstep(-0.5 - soft, -0.5 + soft, in.uv.x);
    let wy = 0.45 * (0.5 - in.uv.x);
    let ay = smoothstep(wy + soft, wy - soft, abs(in.uv.y));
    let alpha = ax * ay;
    if (alpha < 0.02) { discard; }
    let bg = vec3<f32>(0.047, 0.055, 0.075);
    return vec4<f32>(mix(in.color.rgb, bg, in.fog), in.color.a * alpha);
}
