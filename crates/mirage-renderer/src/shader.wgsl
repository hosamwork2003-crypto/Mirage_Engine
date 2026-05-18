const CHUNK_SIZE: u32 = 64u;
struct EntityChunk { 
    positions: array<vec4<f32>, CHUNK_SIZE>, 
    colors: array<vec4<f32>, CHUNK_SIZE>, 
    velocities: array<vec4<f32>, CHUNK_SIZE> 
};
struct CameraUniform { view_proj: mat4x4<f32> };

@group(0) @binding(0) var<storage, read_write> world_chunks: array<EntityChunk>;
@group(0) @binding(3) var<uniform> camera: CameraUniform;

struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) color: vec4<f32> };

@vertex
fn vs_main(@builtin(vertex_index) v_idx: u32, @builtin(instance_index) i_idx: u32) -> VertexOutput {
    let chunk_idx = i_idx; // يتم تمرير الـ active_chunks كمثيلات
    let slot_idx = v_idx / 3u; // كل 3 رؤوس تمثل مثلث (بسيط)
    
    // ملاحظة: للتطوير، سنستخدم instance_index لتحديد الـ chunk_idx من الـ active_chunks buffer
    // لكن للتبسيط الآن نفترض أن i_idx هو الـ index المباشر
    let pos = world_chunks[chunk_idx].positions[v_idx % CHUNK_SIZE];
    let col = world_chunks[chunk_idx].colors[v_idx % CHUNK_SIZE];

    let size = 0.015;
    var offset = vec2<f32>(0.0);
    let tri_idx = v_idx % 3u;
    if (tri_idx == 0u) { offset = vec2<f32>(0.0, size); }
    else if (tri_idx == 1u) { offset = vec2<f32>(-size, -size); }
    else if (tri_idx == 2u) { offset = vec2<f32>(size, -size); }

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(pos.xyz + vec3<f32>(offset, 0.0), 1.0);
    out.color = col;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
