/// Propagation helpers for topology/propagation tasks.
/// Computes which chunks are affected by a disturbance.
///
/// This is a deterministic, index-based traversal over a square world grid.
pub fn compute_propagation(
    origin_chunk: u32,
    radius: f32,
    world_grid_size: u32,
) -> Vec<u32> {
    let mut affected = Vec::new();

    // Grid is organized as world_grid_size x world_grid_size chunks
    // origin_chunk index maps to (x, z) = (idx % world_grid_size, idx / world_grid_size)
    let ox = (origin_chunk % world_grid_size) as i32;
    let oz = (origin_chunk / world_grid_size) as i32;

    let radius_int = radius.ceil() as i32;

    // Iterate all chunks in radius deterministically
    for z in (oz - radius_int)..=(oz + radius_int) {
        for x in (ox - radius_int)..=(ox + radius_int) {
            if x >= 0 && x < world_grid_size as i32 && z >= 0 && z < world_grid_size as i32 {
                let idx = (z as u32 * world_grid_size) + x as u32;
                affected.push(idx);
            }
        }
    }

    affected
}
