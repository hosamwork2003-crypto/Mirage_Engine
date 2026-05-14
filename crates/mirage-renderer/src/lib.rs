use winit::window::Window;
use wgpu::util::DeviceExt;
use std::sync::Arc;

pub const CHUNK_SIZE: usize = 64;
pub const NUM_CHUNKS: u32 = 1563; 
pub const NUM_ENTITIES: u32 = NUM_CHUNKS * CHUNK_SIZE as u32;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EntityChunk {
    pub positions: [[f32; 4]; CHUNK_SIZE],
    pub colors: [[f32; 4]; CHUNK_SIZE],
    pub velocities: [[f32; 4]; CHUNK_SIZE],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpawnRule { pub entity_count: u32, pub seed: u32, pub spread: f32, pub speed: f32 }

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform { pub view_proj: [[f32; 4]; 4] }

pub struct MirageRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub window: Arc<Window>,
    spawn_pipeline: wgpu::ComputePipeline,
    update_pipeline: wgpu::ComputePipeline,
    render_pipeline: wgpu::RenderPipeline,
    compute_bind_group: wgpu::BindGroup,
    render_bind_group: wgpu::BindGroup,
    pub indirect_buffer: wgpu::Buffer, 
    pub scene_buffer: wgpu::Buffer, 
    pub rule_buffer: wgpu::Buffer,
    pub active_chunks_buffer: wgpu::Buffer,
    pub camera_buffer: wgpu::Buffer,
    depth_view: wgpu::TextureView,
}

impl MirageRenderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false }).await.unwrap();
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor { label: Some("Mirage Device"), required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::default() }, None).await.unwrap();

        let config = surface.get_default_config(&adapter, size.width, size.height).unwrap();
        surface.configure(&device, &config);

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth"), size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let scene_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (NUM_CHUNKS as usize * std::mem::size_of::<EntityChunk>()) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let indirect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::bytes_of(&[3u32, 0u32, 0u32, 0u32]), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST });
        let rule_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let active_chunks_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (NUM_CHUNKS as usize * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let compute_shader = device.create_shader_module(wgpu::include_wgsl!("compute.wgsl"));

        // 🛠️ الإصلاح: بناء الـ Layout لربط الـ Pipeline بشكل صحيح
        let render_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }
            ], label: None
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Layout"), bind_group_layouts: &[&render_layout], push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"), layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState { 
                format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, 
                depth_compare: wgpu::CompareFunction::LessEqual, // القضاء على الـ Z-fighting
                stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() 
            }),
            multisample: wgpu::MultisampleState::default(), multiview: None,
        });

        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ], label: None,
        });

        let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&compute_layout], push_constant_ranges: &[] });
        let spawn_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("Spawn Pipeline"), layout: Some(&compute_pipeline_layout), module: &compute_shader, entry_point: "spawn_main" });
        let update_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("Update Pipeline"), layout: Some(&compute_pipeline_layout), module: &compute_shader, entry_point: "update_main" });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { layout: &compute_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: scene_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: indirect_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 2, resource: rule_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 3, resource: active_chunks_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 4, resource: camera_buffer.as_entire_binding() }], label: None });
        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { layout: &render_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: scene_buffer.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: camera_buffer.as_entire_binding() }], label: None });

        Self { surface, device, queue, config, window, spawn_pipeline, update_pipeline, render_pipeline, compute_bind_group, render_bind_group, indirect_buffer, scene_buffer, rule_buffer, active_chunks_buffer, camera_buffer, depth_view }
    }

    pub fn render(&mut self, active_chunk_count: u32) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.queue.write_buffer(&self.indirect_buffer, 4, bytemuck::bytes_of(&[0u32]));
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            compute_pass.set_pipeline(&self.update_pipeline);
            compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
            if active_chunk_count > 0 { compute_pass.dispatch_workgroups(active_chunk_count, 1, 1); }
        }
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &self.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                occlusion_query_set: None, timestamp_writes: None,
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.render_bind_group, &[]);
            render_pass.draw_indirect(&self.indirect_buffer, 0); 
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width; self.config.height = new_size.height; self.surface.configure(&self.device, &self.config);
            let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: self.config.width, height: self.config.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
            self.depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        }
    }
    pub fn upload_active_chunks(&mut self, active_indices: &[u32]) { self.queue.write_buffer(&self.active_chunks_buffer, 0, bytemuck::cast_slice(active_indices)); }
    pub fn update_camera(&mut self, camera: CameraUniform) { self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera)); }
    pub fn dispatch_spawn_rule(&mut self, rule: SpawnRule) { self.queue.write_buffer(&self.rule_buffer, 0, bytemuck::bytes_of(&rule)); let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None }); { let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None }); compute_pass.set_pipeline(&self.spawn_pipeline); compute_pass.set_bind_group(0, &self.compute_bind_group, &[]); compute_pass.dispatch_workgroups(NUM_CHUNKS, 1, 1); } self.queue.submit(std::iter::once(encoder.finish())); }
}