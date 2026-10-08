//! wgpu: surface on a `<canvas>`, two instanced pipelines (edges, nodes).
//! Written against wgpu 30.

use crate::camera::Camera;
use crate::graph::Graph;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Node geometry per instance — the part that changes with every layout
/// step and drag (spec 031, stage 0: uploaded on `pos_rev` only).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NodePos {
    pos: [f32; 3],
}

/// Node appearance per instance — colour and radius change on a graph swap
/// or a hover only (uploaded on `attr_rev`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NodeAttr {
    color: [f32; 4],
    radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EdgePos {
    a: [f32; 3],
    b: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EdgeAttr {
    color: [f32; 4],
}

/// One primitive class's persistent instance buffers. Geometry and
/// appearance are separate so a running layout (positions every frame,
/// colours never) uploads half the bytes, and nothing is allocated per
/// frame: the buffers grow to the next power of two of the instance count
/// and force one full upload when they do.
struct InstBufs {
    pos: wgpu::Buffer,
    attr: wgpu::Buffer,
    cap: u64,
    pos_rev: u64,
    attr_rev: u64,
}

impl InstBufs {
    fn new(device: &wgpu::Device, pos_stride: u64, attr_stride: u64) -> Self {
        let mk = |stride: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("graph instances"),
                size: stride,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            pos: mk(pos_stride),
            attr: mk(attr_stride),
            cap: 0,
            pos_rev: u64::MAX,
            attr_rev: u64::MAX,
        }
    }

    /// Grow to hold `count` instances; recreating forces both halves to be
    /// uploaded once (the revisions reset to `u64::MAX`).
    fn ensure(&mut self, device: &wgpu::Device, count: u64, pos_stride: u64, attr_stride: u64) {
        if count <= self.cap {
            return;
        }
        let cap = count.max(1).next_power_of_two();
        let mk = |stride: u64, label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: cap * stride,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        self.pos = mk(pos_stride, "graph instance positions");
        self.attr = mk(attr_stride, "graph instance appearance");
        self.cap = cap;
        self.pos_rev = u64::MAX;
        self.attr_rev = u64::MAX;
    }
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    node_pipe: wgpu::RenderPipeline,
    edge_pipe: wgpu::RenderPipeline,
    node_bufs: InstBufs,
    edge_bufs: InstBufs,
    /// Depth buffer (3D mode draws nodes over edges by depth; in 2D every
    /// depth is 0.5 and order wins).
    depth: wgpu::TextureView,
    pub backend: String,
    /// The background (spec 030: the theme's `--mk-graph-bg`), sRGB 0..1.
    pub clear: [f64; 3],
}

fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("graph depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Renderer {
    pub async fn new(
        canvas: web_sys::HtmlCanvasElement,
        width: u32,
        height: u32,
        gl_only: bool,
    ) -> Result<Self, String> {
        // WebGPU where the browser really has it, WebGL2 otherwise (or only).
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = if gl_only {
            wgpu::Backends::GL
        } else {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        };
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no adapter: {e}"))?;
        let backend = format!("{:?}", adapter.get_info().backend).to_lowercase();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("moonkale-graph"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("device: {e}"))?;

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface is not supported by the adapter")?;
        // Our colours are the sRGB values the CSS uses; an sRGB surface would
        // re-encode them and wash the scene out (grey background on Chromium's
        // WebGL and the Android WebView — P-090). Prefer a non-sRGB format.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = f;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);
        let format = config.format;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("graph shaders"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::cast_slice(&Camera::default().uniform()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&camera_layout)],
            ..Default::default()
        });
        let targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let make =
            |name: &str, vs: &str, fs: &str, buffers: &[Option<wgpu::VertexBufferLayout>]| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(name),
                    layout: Some(&pipe_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(vs),
                        compilation_options: Default::default(),
                        buffers,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &targets,
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth24Plus,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(wgpu::CompareFunction::LessEqual),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
        fn inst<'a>(
            stride: u64,
            attrs: &'a [wgpu::VertexAttribute],
        ) -> wgpu::VertexBufferLayout<'a> {
            wgpu::VertexBufferLayout {
                array_stride: stride,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: attrs,
            }
        }
        // Two slots per pipeline (spec 031, stage 0): slot 0 carries the
        // per-instance geometry (uploaded while the layout runs), slot 1 the
        // appearance (uploaded on a graph swap or a hover). The location
        // numbering keeps the P-068 lesson: explicit offsets, because the
        // attribute macro would pack them and drop the red channel again.
        let node_pos_layout = inst(
            std::mem::size_of::<NodePos>() as u64,
            &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        );
        let node_attr_layout = inst(
            std::mem::size_of::<NodeAttr>() as u64,
            &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 16,
                    shader_location: 2,
                },
            ],
        );
        let edge_pos_layout = inst(
            std::mem::size_of::<EdgePos>() as u64,
            &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
            ],
        );
        let edge_attr_layout = inst(
            std::mem::size_of::<EdgeAttr>() as u64,
            &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 2,
            }],
        );
        let node_pipe = make(
            "nodes",
            "node_vs",
            "node_fs",
            &[Some(node_pos_layout), Some(node_attr_layout)],
        );
        let edge_pipe = make(
            "edges",
            "edge_vs",
            "edge_fs",
            &[Some(edge_pos_layout), Some(edge_attr_layout)],
        );
        let depth = depth_view(&device, config.width, config.height);
        let node_bufs = InstBufs::new(
            &device,
            std::mem::size_of::<NodePos>() as u64,
            std::mem::size_of::<NodeAttr>() as u64,
        );
        let edge_bufs = InstBufs::new(
            &device,
            std::mem::size_of::<EdgePos>() as u64,
            std::mem::size_of::<EdgeAttr>() as u64,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            camera_buf,
            camera_bind,
            node_pipe,
            edge_pipe,
            node_bufs,
            edge_bufs,
            depth,
            backend,
            // #0c0e13, the dark theme's until `set_theme` says otherwise.
            clear: [0.047, 0.055, 0.075],
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = depth_view(&self.device, width, height);
    }

    /// Upload instance data — only the half whose revision moved; a layout
    /// step moves `pos_rev`, a hover or a graph swap moves both (spec 031,
    /// stage 0) — and draw one frame.
    pub fn draw(
        &mut self,
        graph: &Graph,
        camera: &Camera,
        hovered: Option<usize>,
        pos_rev: u64,
        attr_rev: u64,
    ) {
        let node_stride = std::mem::size_of::<NodePos>() as u64;
        let node_attr_stride = std::mem::size_of::<NodeAttr>() as u64;
        let edge_stride = std::mem::size_of::<EdgePos>() as u64;
        let edge_attr_stride = std::mem::size_of::<EdgeAttr>() as u64;
        self.node_bufs.ensure(
            &self.device,
            graph.nodes.len() as u64,
            node_stride,
            node_attr_stride,
        );
        self.edge_bufs.ensure(
            &self.device,
            graph.edges.len() as u64,
            edge_stride,
            edge_attr_stride,
        );

        if pos_rev != self.node_bufs.pos_rev {
            let pos: Vec<NodePos> = graph
                .nodes
                .iter()
                .map(|n| NodePos {
                    pos: [n.x, n.y, n.z],
                })
                .collect();
            self.queue
                .write_buffer(&self.node_bufs.pos, 0, bytemuck::cast_slice(&pos));
            self.node_bufs.pos_rev = pos_rev;
        }
        if attr_rev != self.node_bufs.attr_rev {
            let attr: Vec<NodeAttr> = graph
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let h = Some(i) == hovered;
                    NodeAttr {
                        color: if h { [1.0, 1.0, 1.0, 1.0] } else { n.color },
                        radius: if h { n.radius * 1.4 } else { n.radius },
                    }
                })
                .collect();
            self.queue
                .write_buffer(&self.node_bufs.attr, 0, bytemuck::cast_slice(&attr));
            self.node_bufs.attr_rev = attr_rev;
        }
        if pos_rev != self.edge_bufs.pos_rev {
            let pos: Vec<EdgePos> = graph
                .edges
                .iter()
                .map(|e| EdgePos {
                    a: [graph.nodes[e.a].x, graph.nodes[e.a].y, graph.nodes[e.a].z],
                    b: [graph.nodes[e.b].x, graph.nodes[e.b].y, graph.nodes[e.b].z],
                })
                .collect();
            self.queue
                .write_buffer(&self.edge_bufs.pos, 0, bytemuck::cast_slice(&pos));
            self.edge_bufs.pos_rev = pos_rev;
        }
        if attr_rev != self.edge_bufs.attr_rev {
            let attr: Vec<EdgeAttr> = graph
                .edges
                .iter()
                .map(|e| EdgeAttr { color: e.color })
                .collect();
            self.queue
                .write_buffer(&self.edge_bufs.attr, 0, bytemuck::cast_slice(&attr));
            self.edge_bufs.attr_rev = attr_rev;
        }
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&camera.uniform()));

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("graph"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.clear[0],
                            g: self.clear[1],
                            b: self.clear[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.camera_bind, &[]);
            if !graph.edges.is_empty() {
                pass.set_pipeline(&self.edge_pipe);
                pass.set_vertex_buffer(
                    0,
                    self.edge_bufs
                        .pos
                        .slice(..graph.edges.len() as u64 * edge_stride),
                );
                pass.set_vertex_buffer(
                    1,
                    self.edge_bufs
                        .attr
                        .slice(..graph.edges.len() as u64 * edge_attr_stride),
                );
                pass.draw(0..4, 0..graph.edges.len() as u32);
            }
            if !graph.nodes.is_empty() {
                pass.set_pipeline(&self.node_pipe);
                pass.set_vertex_buffer(
                    0,
                    self.node_bufs
                        .pos
                        .slice(..graph.nodes.len() as u64 * node_stride),
                );
                pass.set_vertex_buffer(
                    1,
                    self.node_bufs
                        .attr
                        .slice(..graph.nodes.len() as u64 * node_attr_stride),
                );
                pass.draw(0..4, 0..graph.nodes.len() as u32);
            }
        }
        self.queue.submit(Some(enc.finish()));
        self.queue.present(frame);
    }
}
