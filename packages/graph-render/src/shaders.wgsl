struct Camera {
    center: vec2<f32>,
    scale: f32,
    mode: f32,          // 0 = 2D, 1 = 3D (Milestone 8)
    viewport: vec2<f32>,
    dist: f32,          // 3D: orbit distance, for size attenuation
    time: f32,          // seconds since the view was created — the pulses' clock (§2)
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
    @location(4) kind: f32,      // 0 = node, 1 = port (spec 031 §6: port LOD)
};
struct NodeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) fog: f32,
    @location(3) selected: f32,
    @location(4) kind: f32,
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
    // Port LOD (spec 031 §6): once a port's projected size falls under a
    // couple of pixels it stops drawing — the scale lives here, not in the
    // fragment. kind becomes 2 = a suppressed port. kind 3 (a hyperedge
    // junction) is structural and always draws.
    if (inst.kind == 1.0 && inst.radius * camera.scale < 2.5) {
        out.kind = 2.0;
    } else {
        out.kind = inst.kind;
    }
    return out;
}

@fragment
fn node_fs(in: NodeOut) -> @location(0) vec4<f32> {
    if (in.kind > 1.5 && in.kind < 2.5) { discard; } // a port under the pixel threshold
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
    @location(6) pulse: f32,   // world units per second, 0 = none (§2)
    @location(7) edge_len: f32, // the whole stroke's world length
};
struct EdgeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) fog: f32,
    @location(2) coord: f32,    // distance along the stroke, screen pixels
    @location(3) dash: f32,
    @location(4) seglen: f32,   // this segment's screen length (LOD, §6)
    @location(5) wcoord: f32,   // distance along the stroke, world units
    @location(6) pulse: f32,    // world units per second, 0 = none (§2)
    @location(7) pulse_pos: f32, // the glow's world position — the clock lives
                                // in the vertex (the fragment cannot read the
                                // camera uniform; it is vertex-visibility)
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
    out.seglen = length(seg);
    out.wcoord = inst.phase + t * length(inst.b.xyz - inst.a.xyz);
    out.pulse = inst.pulse;
    // The glow's position is the clock's over the stroke's length —
    // constant across the segment, so the fragment interpolates it
    // unchanged.
    out.pulse_pos = 0.0;
    if (inst.pulse > 0.0) {
        out.pulse_pos = fract(camera.time * inst.pulse / max(inst.edge_len, 1e-3)) * inst.edge_len;
    }
    return out;
}

@fragment
fn edge_fs(in: EdgeOut) -> @location(0) vec4<f32> {
    // Dash LOD (spec 031 §6): a dash pattern needs several dash lengths
    // to read. On a segment shorter than that the gaps would eat most of
    // the stroke — it draws solid instead.
    if (in.dash > 0.5 && in.seglen > 6.0) {
        // 50 % duty cycle along the stroke, in screen pixels.
        if (fract(in.coord / in.dash) < 0.5) { discard; }
    }
    var rgb = in.color.rgb;
    var alpha = in.color.a;
    // A pulse travels the whole stroke in world units (spec 031 §2): the
    // position arrives from the vertex, the distance is interpolated per
    // fragment, so the glow crosses a curve's sub-segments as one
    // continuous line. Barely visible animations stop (§6): a segment
    // under 8 screen px does not glow.
    if (in.pulse > 0.0 && in.seglen > 8.0) {
        let d = abs(in.wcoord - in.pulse_pos);
        let glow = exp(-(d * d) / 128.0);  // sigma = 8 world units
        rgb = mix(rgb, vec3<f32>(1.0), glow * 0.85);
        alpha = clamp(alpha + glow * 0.6, 0.0, 1.0);
    }
    let bg = vec3<f32>(0.047, 0.055, 0.075);
    return vec4<f32>(mix(rgb, bg, in.fog), alpha);
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

// ---------------- group hulls: rounded-rect SDF, drawn first ----------------
// Spec 031 §4: a group tints the region its members occupy. One billboard
// per hull, two instance slots like every pipeline (geometry on pos_rev,
// tint on attr_rev); drawn before the edges and nodes so it sits behind.
struct HullInst {
    @location(0) center: vec3<f32>,
    @location(1) half: vec2<f32>,   // world, padding included
    @location(2) color: vec4<f32>,
    @location(3) corner: f32,       // world
    @location(4) highlight: f32,    // 1 = a clique's rim (spec 031 §2)
};
struct HullOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,     // ±1 across the rect
    @location(1) color: vec4<f32>,
    @location(2) extent: vec2<f32>, // half-extents in screen pixels
    @location(3) corner: f32,       // corner radius in screen pixels
    @location(4) highlight: f32,
};

@vertex
fn hull_vs(@builtin(vertex_index) vi: u32, inst: HullInst) -> HullOut {
    var corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    let c = corners[vi];
    let pr = project(inst.center);
    // Screen half-extents shrink with the zoom; the corner keeps a 3 px
    // minimum so the shape stays rounded, clamped to never exceed the
    // box (a pill at most).
    let ew = max(inst.half * camera.scale, vec2<f32>(1.0, 1.0));
    let cr = clamp(inst.corner * camera.scale, 0.0, min(ew.x, ew.y));
    let corner_px = max(cr, min(3.0, min(ew.x, ew.y)));
    let off = vec2<f32>(c.x * ew.x / (camera.viewport.x * 0.5),
                       c.y * ew.y / (camera.viewport.y * 0.5));
    var out: HullOut;
    out.clip = vec4<f32>(pr.x + off.x, pr.y + off.y, pr.z, 1.0);
    out.uv = c;
    out.color = inst.color;
    out.extent = ew;
    out.corner = corner_px;
    out.highlight = inst.highlight;
    return out;
}

@fragment
fn hull_fs(in: HullOut) -> @location(0) vec4<f32> {
    // Signed distance to the rounded rect, in screen pixels.
    let q = abs(in.uv * in.extent) - (in.extent - vec2<f32>(in.corner, in.corner));
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - in.corner;
    let edge = fwidth(d) * 1.5;
    let cov = 1.0 - smoothstep(-edge, edge, d);
    if (cov < 0.02) { discard; }
    // LOD (spec 031 §6): a hull a few pixels across is a smudge, not a
    // region — it fades out between 6 and 14 px of its smaller half-extent.
    let lod = smoothstep(6.0, 14.0, min(in.extent.x, in.extent.y));
    if (lod < 0.02) { discard; }
    // The fill tints; a thin band of the group's colour traces the rim —
    // twice as strong for a highlighted clique (spec 031 §2).
    let rim_w = fwidth(d) * 1.5 + 1.5;
    let inner = 1.0 - smoothstep(-edge, edge, d + rim_w);
    let rim_a = clamp(cov - inner, 0.0, 1.0) * select(0.45, 0.9, in.highlight > 0.5) * lod;
    return vec4<f32>(in.color.rgb, max(cov * in.color.a * lod, rim_a));
}

// ---------------- simplicial faces: instanced filled triangles ----------------
// Spec 031 §2: a 2-simplex is a filled triangle over three nodes (a face
// over more is fanned into triangles); drawn behind the edges and nodes.
// Not a group hull, not a hyperedge junction — a third class.
struct SimpInst {
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) c: vec3<f32>,
    @location(3) color: vec4<f32>,
};
struct SimpOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) min_edge: f32,   // the triangle's smallest edge, screen px
};

@vertex
fn simplex_vs(@builtin(vertex_index) vi: u32, inst: SimpInst) -> SimpOut {
    let pa = project(inst.a);
    let pb = project(inst.b);
    let pc = project(inst.c);
    var corner = array<vec4<f32>, 3>(pa, pb, pc);
    let sx = camera.viewport * 0.5;
    let sa = vec2<f32>(pa.x * sx.x, -pa.y * sx.y);
    let sb = vec2<f32>(pb.x * sx.x, -pb.y * sx.y);
    let sc = vec2<f32>(pc.x * sx.x, -pc.y * sx.y);
    let e0 = distance(sa, sb);
    let e1 = distance(sb, sc);
    let e2 = distance(sc, sa);
    var out: SimpOut;
    out.clip = corner[vi];
    out.color = inst.color;
    out.min_edge = min(e0, min(e1, e2));
    return out;
}

@fragment
fn simplex_fs(in: SimpOut) -> @location(0) vec4<f32> {
    // LOD (spec 031 §6), same ladder as the hulls: a face a few pixels
    // across fades out instead of smudging.
    let lod = smoothstep(6.0, 14.0, in.min_edge);
    if (lod < 0.02) { discard; }
    return vec4<f32>(in.color.rgb, in.color.a * lod);
}
