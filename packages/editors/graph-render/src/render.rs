//! wgpu: surface on a `<canvas>`, two instanced pipelines (edges, nodes).
//! Written against wgpu 30.

use crate::camera::Camera;
use crate::graph::Graph;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NodeInst {
    pos: [f32; 3],
    radius: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EdgeInst {
    a: [f32; 3],
    b: [f32; 3],
    color: [f32; 4],
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
    /// Depth buffer (3D mode draws nodes over edges by depth; in 2D every
    /// depth is 0.5 and order wins).
    depth: wgpu::TextureView,
    pub backend: String,
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
            |name: &str, vs: &str, fs: &str, stride: u64, attrs: &[wgpu::VertexAttribute]| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(name),
                    layout: Some(&pipe_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(vs),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: stride,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: attrs,
                        })],
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
        let node_pipe = make(
            "nodes",
            "node_vs",
            "node_fs",
            std::mem::size_of::<NodeInst>() as u64,
            // Explicit offsets: `NodeInst` pads `radius` to 16 bytes before
            // `color`; `vertex_attr_array!` would pack the colour at 12 and
            // the shader would read (pad, r, g, b) — every node lost its red
            // channel (P-068).
            &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 12,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
            ],
        );
        let edge_pipe = make(
            "edges",
            "edge_vs",
            "edge_fs",
            std::mem::size_of::<EdgeInst>() as u64,
            &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
        );
        let depth = depth_view(&device, config.width, config.height);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            camera_buf,
            camera_bind,
            node_pipe,
            edge_pipe,
            depth,
            backend,
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

    /// Upload instance data and draw one frame.
    pub fn draw(&mut self, graph: &Graph, camera: &Camera, hovered: Option<usize>) {
        let nodes: Vec<NodeInst> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| NodeInst {
                pos: [n.x, n.y, n.z],
                radius: if Some(i) == hovered {
                    n.radius * 1.4
                } else {
                    n.radius
                },
                color: if Some(i) == hovered {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    n.color
                },
            })
            .collect();
        let edges: Vec<EdgeInst> = graph
            .edges
            .iter()
            .map(|e| EdgeInst {
                a: [graph.nodes[e.a].x, graph.nodes[e.a].y, graph.nodes[e.a].z],
                b: [graph.nodes[e.b].x, graph.nodes[e.b].y, graph.nodes[e.b].z],
                color: e.color,
            })
            .collect();
        let node_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("nodes"),
                contents: bytemuck::cast_slice(&nodes),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let edge_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("edges"),
                contents: bytemuck::cast_slice(&edges),
                usage: wgpu::BufferUsages::VERTEX,
            });
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
                            r: 0.047,
                            g: 0.055,
                            b: 0.075,
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
            if !edges.is_empty() {
                pass.set_pipeline(&self.edge_pipe);
                pass.set_vertex_buffer(0, edge_buf.slice(..));
                pass.draw(0..4, 0..edges.len() as u32);
            }
            if !nodes.is_empty() {
                pass.set_pipeline(&self.node_pipe);
                pass.set_vertex_buffer(0, node_buf.slice(..));
                pass.draw(0..4, 0..nodes.len() as u32);
            }
        }
        self.queue.submit(Some(enc.finish()));
        self.queue.present(frame);
    }
}
