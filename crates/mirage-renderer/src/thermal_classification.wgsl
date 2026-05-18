// ===================================================================
// mirage-renderer/src/thermal_classification.wgsl (V3 — Federated Stabilization Pass)
//
// V3-RENDERER-PASSIVE: This shader is a PASSIVE consumer.
//
// chunk_states (binding 0) is written by the V3 RendererBridge
// (mirage-mkr-core/src/bridge/renderer_bridge.rs) which translates
// ActivationField::execution_probability → ChunkState enum.
//
// The renderer GPU shader reads this buffer and classifies chunks —
// it does NOT compute thermal state autonomously.
//
// TODO(V3-RENDERER-PASSIVE): Once GPU shaders are updated to consume
// a continuous float buffer instead of u32 enum states, replace
// chunk_states: array<u32> with execution_probability: array<f32>.
// Classification logic becomes direct threshold comparisons on f32
// instead of u32 enum arms, which is more GPU-native.
//
// TODO(V3-RENDERER-PASSIVE): MAX_CHUNKS is hardcoded to 15625.
// This must be driven by the field grid dimensions (width * height)
// at runtime, not a compile-time constant.
// ===================================================================

const STATE_DORMANT    : u32 = 0u;
const STATE_PREDICTIVE : u32 = 1u;
const STATE_RESIDENT   : u32 = 2u;
const STATE_HOT        : u32 = 3u;

const MAX_CHUNKS : u32 = 15625u;


struct ChunkList {
    count: atomic<u32>,
    indices: array<u32, 15625>,
};

@group(0) @binding(0)
var<storage, read> chunk_states: array<u32>;

@group(0) @binding(1)
var<storage, read_write> hot_chunks: ChunkList;

@group(0) @binding(2)
var<storage, read_write> resident_chunks: ChunkList;

@compute @workgroup_size(64)
fn classify_main(
    @builtin(global_invocation_id) gid: vec3<u32>
) {
    let chunk_idx = gid.x;

    if (chunk_idx >= MAX_CHUNKS) {
        return;
    }

    let state = chunk_states[chunk_idx];

    // =====================================================
    // HOT CHUNKS
    // =====================================================

    if (state == STATE_HOT) {

        let hot_index =
            atomicAdd(
                &hot_chunks.count,
                1u
            );

        hot_chunks.indices[hot_index] =
            chunk_idx;

        return;
    }

    // =====================================================
    // RESIDENT CHUNKS
    // =====================================================

    if (state == STATE_RESIDENT) {

        let resident_index =
            atomicAdd(
                &resident_chunks.count,
                1u
            );

        resident_chunks.indices[resident_index] =
            chunk_idx;

        return;
    }
}