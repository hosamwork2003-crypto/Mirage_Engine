const CHUNK_SIZE: u32 = 64u;

struct EntityChunk {
    positions: array<vec4<f32>, CHUNK_SIZE>,
    colors: array<vec4<f32>, CHUNK_SIZE>,
    velocities: array<vec4<f32>, CHUNK_SIZE>,
};

struct DrawIndirect {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
};

struct SpawnRule { entity_count: u32, seed: u32, spread: f32, speed: f32 };
struct CameraUniform { view_proj: mat4x4<f32> };

@group(0) @binding(0) var<storage, read_write> world_chunks: array<EntityChunk>;
@group(0) @binding(1) var<storage, read_write> draw_cmd: DrawIndirect;
@group(0) @binding(2) var<uniform> rule: SpawnRule;
@group(0) @binding(3) var<storage, read> active_chunks: array<u32>;
@group(0) @binding(4) var<uniform> camera: CameraUniform;

fn hash(u: u32) -> f32 {
    var x = u;
    x = ((x >> 16u) ^ x) * 0x45d9f3bu;
    x = ((x >> 16u) ^ x) * 0x45d9f3bu;
    x = (x >> 16u) ^ x;
    return f32(x) * (1.0 / f32(0xffffffffu));
}

@compute @workgroup_size(64)
fn spawn_main(@builtin(global_invocation_id) global_id: vec3<u32>, @builtin(workgroup_id) group_id: vec3<u32>, @builtin(local_invocation_id) local_id: vec3<u32>) {
    let chunk_idx = group_id.x; 
    let slot_idx = local_id.x;
    let flat_idx = global_id.x;
    if (flat_idx >= rule.entity_count) { return; }

    let angle = f32(flat_idx) * 137.5;
    let radius = sqrt(f32(flat_idx) / f32(rule.entity_count)) * rule.spread;
    let z_depth = (hash(flat_idx) - 0.5) * 3.0;

    world_chunks[chunk_idx].positions[slot_idx] = vec4<f32>(cos(angle) * radius, sin(angle) * radius, z_depth, 1.0);
    world_chunks[chunk_idx].colors[slot_idx] = vec4<f32>(radius * 0.25, 0.45, 1.0 - (radius * 0.25), 1.0);
    world_chunks[chunk_idx].velocities[slot_idx] = vec4<f32>(cos(angle + 1.5) * rule.speed, sin(angle + 1.5) * rule.speed, 0.0, 0.0);
}

@compute @workgroup_size(64)
fn update_main(@builtin(workgroup_id) group_id: vec3<u32>, @builtin(local_invocation_id) local_id: vec3<u32>) {
    let chunk_idx = active_chunks[group_id.x]; 
    let slot_idx = local_id.x;
    
    var pos = world_chunks[chunk_idx].positions[slot_idx];
    var vel = world_chunks[chunk_idx].velocities[slot_idx];
    
    let dist_sq = dot(pos.xy, pos.xy);
    let inv_dist = inverseSqrt(dist_sq + 0.1); 
    let dir = -pos.xy * inv_dist;
    
    vel.x += dir.x * 0.0001;
    vel.y += dir.y * 0.0001;

    let tangent = vec2<f32>(-pos.y, pos.x) * inv_dist;
    vel.x += tangent.x * 0.00025;
    vel.y += tangent.y * 0.00025;

    pos.x += vel.x;
    pos.y += vel.y;
    vel.x *= 0.999;
    vel.y *= 0.999;

    world_chunks[chunk_idx].positions[slot_idx] = pos;
    world_chunks[chunk_idx].velocities[slot_idx] = vel;

    let clip_pos = camera.view_proj * pos;
    let ndc = clip_pos.xyz / clip_pos.w;
    
    // 💡 هامش أمان 1.4 لمنع تذبذب المثلثات عند حواف الشاشة والزووم
    if (abs(ndc.x) <= 1.4 && abs(ndc.y) <= 1.4 && clip_pos.w > 0.0) {
        atomicAdd(&draw_cmd.instance_count, 1u);
    }
}