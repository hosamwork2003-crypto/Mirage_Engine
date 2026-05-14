const CHUNK_SIZE: u32 = 64u;
struct EntityChunk { positions: array<vec4<f32>, CHUNK_SIZE>, colors: array<vec4<f32>, CHUNK_SIZE>, velocities: array<vec4<f32>, CHUNK_SIZE> };
struct CameraUniform { view_proj: mat4x4<f32> };

@group(0) @binding(0) var<storage, read> world_chunks: array<EntityChunk>;
@group(0) @binding(1) var<uniform> camera: CameraUniform;

struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) color: vec4<f32> };

@vertex
fn vs_main(@builtin(vertex_index) v_idx: u32, @builtin(instance_index) i_idx: u32) -> VertexOutput {
    let chunk_idx = i_idx / CHUNK_SIZE;
    let slot_idx = i_idx % CHUNK_SIZE;
    let pos = world_chunks[chunk_idx].positions[slot_idx];
    let col = world_chunks[chunk_idx].colors[slot_idx];

    let size = 0.015;
    var offset = vec2<f32>(0.0);
    if (v_idx == 0u) { offset = vec2<f32>(0.0, size); }
    else if (v_idx == 1u) { offset = vec2<f32>(-size, -size); }
    else { offset = vec2<f32>(size, -size); }

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(pos.x + offset.x, pos.y + offset.y, pos.z, 1.0);
    out.color = col;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> { return in.color; }