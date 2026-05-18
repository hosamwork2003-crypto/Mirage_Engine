const CHUNK_SIZE : u32 = 64u;

struct EntityChunk {
    positions  : array<vec4<f32>, CHUNK_SIZE>,
    colors     : array<vec4<f32>, CHUNK_SIZE>,
    velocities : array<vec4<f32>, CHUNK_SIZE>,
};

struct DrawIndirect {
    vertex_count   : u32,
    instance_count : atomic<u32>,
    first_vertex   : u32,
    first_instance : u32,
};

struct CameraUniform {
    view_proj : mat4x4<f32>,
};

struct PulseUniform {
    origin        : vec4<f32>,
    payload       : vec4<f32>,
    mutation_data : vec4<f32>,
};

struct ChunkList {
    count   : atomic<u32>,
    indices : array<u32, 15625>,
};

@group(0) @binding(0)
var<storage, read_write>
world_chunks : array<EntityChunk>;

@group(0) @binding(1)
var<storage, read_write>
draw_cmd : DrawIndirect;

@group(0) @binding(2)
var<storage, read>
hot_chunks : ChunkList;

@group(0) @binding(3)
var<uniform>
camera : CameraUniform;

@group(0) @binding(4)
var<uniform>
pulse : PulseUniform;

@group(0) @binding(5)
var<storage, read>
chunk_states : array<u32>;

@compute @workgroup_size(64)
fn update_main(

    @builtin(workgroup_id)
    group_id : vec3<u32>,

    @builtin(local_invocation_id)
    local_id : vec3<u32>

) {

    let hot_count =
        atomicLoad(&hot_chunks.count);

    if (group_id.x >= hot_count) {
        return;
    }

    let chunk_idx =
        hot_chunks.indices[group_id.x];

    let slot_idx =
        local_id.x;

    // =====================================================
    // LOAD ENTITY
    // =====================================================

    var pos =
        world_chunks[chunk_idx]
            .positions[slot_idx];

    var vel =
        world_chunks[chunk_idx]
            .velocities[slot_idx];

    // =====================================================
    // ORBITAL PHYSICS
    // =====================================================

    let dist_sq =
        dot(pos.xy, pos.xy);

    let inv_dist =
        inverseSqrt(dist_sq + 0.1);

    let dir =
        -pos.xy * inv_dist;

    vel.x += dir.x * 0.0001;
    vel.y += dir.y * 0.0001;

    let tangent =
        vec2<f32>(
            -pos.y,
             pos.x
        ) * inv_dist;

    vel.x += tangent.x * 0.00025;
    vel.y += tangent.y * 0.00025;

    // =====================================================
    // PULSE SYSTEM
    // =====================================================

    let delta =
        pos.xyz - pulse.origin.xyz;

    let dist_to_pulse =
        length(delta);

    let radius =
        pulse.mutation_data.x;

    let mutation_type =
        pulse.mutation_data.y;

    if (dist_to_pulse < radius) {

        // COLOR MUTATION
        if (mutation_type == 1.0) {

            world_chunks[chunk_idx]
                .colors[slot_idx] =
                    pulse.payload;
        }

        // FORCE IMPULSE
        if (mutation_type == 2.0) {

            let force =
                (1.0 -
                (dist_to_pulse / radius))
                * pulse.payload.x;

            let push_dir =
                normalize(delta);

            vel.x += push_dir.x * force;
            vel.y += push_dir.y * force;
            vel.z += push_dir.z * force;
        }
    }

    // =====================================================
    // APPLY VELOCITY
    // =====================================================

    pos.x += vel.x;
    pos.y += vel.y;
    pos.z += vel.z;

    world_chunks[chunk_idx]
        .positions[slot_idx] = pos;

    world_chunks[chunk_idx]
        .velocities[slot_idx] = vel;

    // =====================================================
    // DRAW COUNT
    // =====================================================

    if (local_id.x == 0u) {

        atomicAdd(
            &draw_cmd.instance_count,
            1u
        );
    }
}