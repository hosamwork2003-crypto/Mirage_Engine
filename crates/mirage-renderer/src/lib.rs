use winit::window::Window;
use std::sync::Arc;

pub mod residency;
pub mod gpu_driven;

pub use gpu_driven::GpuDriven;
pub use residency::{ResidencyTracker, ResidencyState};

pub const CHUNK_SIZE: usize = 64;
pub const NUM_CHUNKS: u32 = 15625;
pub const NUM_ENTITIES: u32 = NUM_CHUNKS * CHUNK_SIZE as u32;
pub const MAX_CHUNKS: usize = NUM_CHUNKS as usize;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EntityChunk {
    pub positions: [[f32; 4]; CHUNK_SIZE],
    pub colors: [[f32; 4]; CHUNK_SIZE],
    pub velocities: [[f32; 4]; CHUNK_SIZE],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PulseUniform {
    pub origin: [f32; 4],
    pub payload: [f32; 4],
    pub mutation_data: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ChunkListHeader {
    pub count: u32,
}

#[allow(dead_code)]

pub struct MirageRenderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    pub queue: wgpu::Queue,

    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,

    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    thermal_pipeline: wgpu::ComputePipeline,

    world_buffer: wgpu::Buffer,
    indirect_buffer: wgpu::Buffer,

    active_chunks_buffer: wgpu::Buffer,

    hot_chunks_buffer: wgpu::Buffer,
    resident_chunks_buffer: wgpu::Buffer,

    camera_buffer: wgpu::Buffer,
    pulse_buffer: wgpu::Buffer,

    pub states_buffer: wgpu::Buffer,

    bind_group: wgpu::BindGroup,
    thermal_bind_group: wgpu::BindGroup,

    depth_view: wgpu::TextureView,
}

impl MirageRenderer {
    pub async fn new(window: Arc<Window>) -> Self {

        let size = window.inner_size();

        let instance = wgpu::Instance::default();

let surface =
    instance
        .create_surface(window.clone())
        .unwrap();

        let adapter =
            instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    power_preference:
                        wgpu::PowerPreference::HighPerformance,

                    compatible_surface: Some(&surface),

                    force_fallback_adapter: false,
                },
            )
            .await
            .unwrap();

let (device, queue) =
    adapter.request_device(
        &wgpu::DeviceDescriptor {

            label: Some("Mirage Device"),

            required_features:
                wgpu::Features::VERTEX_WRITABLE_STORAGE,

            required_limits:
                wgpu::Limits::default(),
        },

        None,
    ).await.unwrap();

        let surface_caps =
            surface.get_capabilities(&adapter);

        let config =
            wgpu::SurfaceConfiguration {
                usage:
                    wgpu::TextureUsages::RENDER_ATTACHMENT,

                format:
                    surface_caps.formats[0],

                width:
                    size.width,

                height:
                    size.height,

                present_mode:
                    wgpu::PresentMode::Immediate,

                alpha_mode:
                    surface_caps.alpha_modes[0],

                view_formats: vec![],

                desired_maximum_frame_latency: 2,
            };

        surface.configure(&device, &config);

        // =========================================================
        // BUFFERS
        // =========================================================

        let world_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("World Buffer"),

                    size:
                        (
                            std::mem::size_of::<EntityChunk>()
                                * NUM_CHUNKS as usize
                        ) as u64,

                    usage:
                        wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        let indirect_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("Indirect Buffer"),

                    size: 16,

                    usage:
                        wgpu::BufferUsages::INDIRECT
                            | wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        let active_chunks_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label:
                        Some("Active Chunks Buffer"),

                    size:
                        (NUM_CHUNKS * 4) as u64,

                    usage:
                        wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        let camera_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("Camera Buffer"),

                    size: 64,

                    usage:
                        wgpu::BufferUsages::UNIFORM
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        let pulse_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("Pulse Buffer"),

                    size:
                        std::mem::size_of::<PulseUniform>()
                            as u64,

                    usage:
                        wgpu::BufferUsages::UNIFORM
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        let states_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label:
                        Some("Chunk States Buffer"),

                    size:
                        (NUM_CHUNKS * 4) as u64,

                    usage:
                        wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

const CHUNK_LIST_SIZE: u64 =
    16 + (MAX_CHUNKS as u64 * 4);

        let hot_chunks_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label:
                        Some("Hot Chunks Buffer"),

                    size: CHUNK_LIST_SIZE,

                    usage:
                        wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        let resident_chunks_buffer =
            device.create_buffer(
                &wgpu::BufferDescriptor {
                    label:
                        Some("Resident Chunks Buffer"),

                    size: CHUNK_LIST_SIZE,

                    usage:
                        wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_DST,

                    mapped_at_creation: false,
                },
            );

        // =========================================================
        // MAIN BIND GROUP LAYOUT
        // =========================================================

        let bind_group_layout =
            device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label:
                        Some("Main Bind Group Layout"),

                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,

                            visibility:
                                wgpu::ShaderStages::COMPUTE
                                    | wgpu::ShaderStages::VERTEX,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: false,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 1,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: false,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 2,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: true,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 3,

                            visibility:
                                wgpu::ShaderStages::COMPUTE
                                    | wgpu::ShaderStages::VERTEX,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Uniform,

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 4,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Uniform,

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 5,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: true,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },
                    ],
                },
            );

        let bind_group =
            device.create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label:
                        Some("Main Bind Group"),

                    layout:
                        &bind_group_layout,

                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource:
                                world_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource:
                                indirect_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource:
                                active_chunks_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource:
                                camera_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource:
                                pulse_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource:
                                states_buffer
                                    .as_entire_binding(),
                        },
                    ],
                },
            );

        // =========================================================
        // THERMAL BIND GROUP
        // =========================================================

        let thermal_bind_group_layout =
            device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label:
                        Some("Thermal Layout"),

                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: true,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 1,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: false,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },

                        wgpu::BindGroupLayoutEntry {
                            binding: 2,

                            visibility:
                                wgpu::ShaderStages::COMPUTE,

                            ty:
                                wgpu::BindingType::Buffer {
                                    ty:
                                        wgpu::BufferBindingType::Storage {
                                            read_only: false,
                                        },

                                    has_dynamic_offset: false,

                                    min_binding_size: None,
                                },

                            count: None,
                        },
                    ],
                },
            );

        let thermal_bind_group =
            device.create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label:
                        Some("Thermal Bind Group"),

                    layout:
                        &thermal_bind_group_layout,

                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,

                            resource:
                                states_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 1,

                            resource:
                                hot_chunks_buffer
                                    .as_entire_binding(),
                        },

                        wgpu::BindGroupEntry {
                            binding: 2,

                            resource:
                                resident_chunks_buffer
                                    .as_entire_binding(),
                        },
                    ],
                },
            );

        // =========================================================
        // SHADERS
        // =========================================================

        let shader =
            device.create_shader_module(
                wgpu::include_wgsl!("shader.wgsl"),
            );

        let compute_shader =
            device.create_shader_module(
                wgpu::include_wgsl!("compute.wgsl"),
            );

        let thermal_shader =
            device.create_shader_module(
                wgpu::include_wgsl!(
                    "thermal_classification.wgsl"
                ),
            );

        // =========================================================
        // PIPELINE LAYOUTS
        // =========================================================

        let pipeline_layout =
            device.create_pipeline_layout(
                &wgpu::PipelineLayoutDescriptor {
                    label:
                        Some("Main Pipeline Layout"),

                    bind_group_layouts:
                        &[&bind_group_layout],

                    push_constant_ranges: &[],
                },
            );

        let thermal_pipeline_layout =
            device.create_pipeline_layout(
                &wgpu::PipelineLayoutDescriptor {
                    label:
                        Some("Thermal Pipeline Layout"),

                    bind_group_layouts:
                        &[&thermal_bind_group_layout],

                    push_constant_ranges: &[],
                },
            );

        // =========================================================
        // RENDER PIPELINE
        // =========================================================

        let render_pipeline =
            device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label:
                        Some("Render Pipeline"),

                    layout:
                        Some(&pipeline_layout),

                    vertex:
                        wgpu::VertexState {
                            module: &shader,

                            entry_point: "vs_main",

                            buffers: &[],
                        },

                    fragment:
                        Some(
                            wgpu::FragmentState {
                                module: &shader,

                                entry_point: "fs_main",

                                targets: &[Some(
                                    wgpu::ColorTargetState {
                                        format:
                                            config.format,

                                        blend:
                                            Some(
                                                wgpu::BlendState::REPLACE
                                            ),

                                        write_mask:
                                            wgpu::ColorWrites::ALL,
                                    }
                                )],
                            }
                        ),

                    primitive:
                        wgpu::PrimitiveState {
                            topology:
                                wgpu::PrimitiveTopology::TriangleList,

                            ..Default::default()
                        },

                    depth_stencil:
                        Some(
                            wgpu::DepthStencilState {
                                format:
                                    wgpu::TextureFormat::Depth32Float,

                                depth_write_enabled: true,

                                depth_compare:
                                    wgpu::CompareFunction::Less,

                                stencil:
                                    wgpu::StencilState::default(),

                                bias:
                                    wgpu::DepthBiasState::default(),
                            }
                        ),

                    multisample:
                        wgpu::MultisampleState::default(),

                    multiview: None,
                },
            );

        // =========================================================
        // COMPUTE PIPELINE
        // =========================================================

        let compute_pipeline =
            device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label:
                        Some("Compute Pipeline"),

                    layout:
                        Some(&pipeline_layout),

                    module:
                        &compute_shader,

                    entry_point:
                        "update_main",
                },
            );

        // =========================================================
        // THERMAL PIPELINE
        // =========================================================

        let thermal_pipeline =
            device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label:
                        Some("Thermal Pipeline"),

                    layout:
                        Some(&thermal_pipeline_layout),

                    module:
                        &thermal_shader,

                    entry_point:
                        "classify_main",
                },
            );

        // =========================================================
        // DEPTH TEXTURE
        // =========================================================

        let depth_view =
            device.create_texture(
                &wgpu::TextureDescriptor {
                    label:
                        Some("Depth Texture"),

                    size:
                        wgpu::Extent3d {
                            width:
                                config.width,

                            height:
                                config.height,

                            depth_or_array_layers: 1,
                        },

                    mip_level_count: 1,

                    sample_count: 1,

                    dimension:
                        wgpu::TextureDimension::D2,

                    format:
                        wgpu::TextureFormat::Depth32Float,

                    usage:
                        wgpu::TextureUsages::RENDER_ATTACHMENT,

                    view_formats: &[],
                },
            )
            .create_view(
                &wgpu::TextureViewDescriptor::default(),
            );

        // =========================================================
        // FINAL SELF
        // =========================================================

        Self {
            window,
            surface,
            device,
            queue,

            config,
            size,

            render_pipeline,
            compute_pipeline,
            thermal_pipeline,

            world_buffer,
            indirect_buffer,

            active_chunks_buffer,

            hot_chunks_buffer,
            resident_chunks_buffer,

            camera_buffer,
            pulse_buffer,

            states_buffer,

            bind_group,
            thermal_bind_group,

            depth_view,
        }
    }
        pub fn render(
        &mut self,
        active_count: u32,
    ) -> Result<(), wgpu::SurfaceError> {

        let output =
            self.surface.get_current_texture()?;

        let view =
            output.texture.create_view(
                &wgpu::TextureViewDescriptor::default(),
            );

        let mut encoder =
            self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                },
            );

        //
        // THERMAL PASS
        //
        {
            let mut pass =
                encoder.begin_compute_pass(
                    &wgpu::ComputePassDescriptor {
                        label: Some("Thermal Pass"),
                        timestamp_writes: None,
                    },
                );

            pass.set_pipeline(&self.thermal_pipeline);

            pass.set_bind_group(
                0,
                &self.thermal_bind_group,
                &[],
            );

            pass.dispatch_workgroups(NUM_CHUNKS, 1, 1);
        }

        //
        // SIMULATION PASS
        //
        {
            let mut pass =
                encoder.begin_compute_pass(
                    &wgpu::ComputePassDescriptor {
                        label: Some("Simulation Pass"),
                        timestamp_writes: None,
                    },
                );

            pass.set_pipeline(&self.compute_pipeline);

            pass.set_bind_group(
                0,
                &self.bind_group,
                &[],
            );

            pass.dispatch_workgroups(
                active_count,
                1,
                1,
            );
        }

        //
        // RENDER PASS
        //
        {
            let mut rpass =
                encoder.begin_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),

                        color_attachments: &[
                            Some(
                                wgpu::RenderPassColorAttachment {
                                    view: &view,

                                    resolve_target: None,

                                    ops: wgpu::Operations {
                                        load:
                                            wgpu::LoadOp::Clear(
                                                wgpu::Color {
                                                    r: 0.01,
                                                    g: 0.01,
                                                    b: 0.02,
                                                    a: 1.0,
                                                },
                                            ),

                                        store:
                                            wgpu::StoreOp::Store,
                                    },
                                },
                            ),
                        ],

                        depth_stencil_attachment:
                            Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &self.depth_view,

                                    depth_ops: Some(
                                        wgpu::Operations {
                                            load:
                                                wgpu::LoadOp::Clear(1.0),

                                            store:
                                                wgpu::StoreOp::Store,
                                        },
                                    ),

                                    stencil_ops: None,
                                },
                            ),

                        timestamp_writes: None,

                        occlusion_query_set: None,
                    },
                );

            rpass.set_pipeline(&self.render_pipeline);

            rpass.set_bind_group(
                0,
                &self.bind_group,
                &[],
            );

            rpass.draw_indirect(
                &self.indirect_buffer,
                0,
            );
        }

        self.queue.submit(
            std::iter::once(
                encoder.finish(),
            ),
        );

        output.present();

        Ok(())
    }

    pub fn update_states_buffer(
        &mut self,
        states: &[u32],
    ) {
        self.queue.write_buffer(
            &self.states_buffer,
            0,
            bytemuck::cast_slice(states),
        );
    }

    pub fn upload_active_chunks(
        &mut self,
        active_indices: &[u32],
    ) {
        self.queue.write_buffer(
            &self.active_chunks_buffer,
            0,
            bytemuck::cast_slice(active_indices),
        );
    }

    pub fn update_camera(
        &mut self,
        camera: CameraUniform,
    ) {
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&camera),
        );
    }

    pub fn update_pulse(
        &mut self,
        pulse: PulseUniform,
    ) {
        self.queue.write_buffer(
            &self.pulse_buffer,
            0,
            bytemuck::bytes_of(&pulse),
        );
    }

    pub fn upload_chunk_to_vram(
        &self,
        chunk_idx: u32,
        chunk_bytes: &[u8],
    ) {
        let offset =
            chunk_idx as u64
                * std::mem::size_of::<EntityChunk>() as u64;

        self.queue.write_buffer(
            &self.world_buffer,
            offset,
            chunk_bytes,
        );
    }

    pub fn reset_draw_count(
        &mut self,
    ) {
        self.queue.write_buffer(
            &self.indirect_buffer,
            0,
            bytemuck::cast_slice(
                &[
    CHUNK_SIZE as u32 * 3,
    0,
    0,
    0,
],
            ),
        );
    }
}