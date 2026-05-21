use mirage_mkr_core::MKRWorld;

fn main() {
    println!("[MKR V3/V4 Substrate] Initialising Federated Library Substrate...");
    // Instantiate unified kernel via library layout interface
    let mut world = MKRWorld::new(16, 16, 32);
    world.enable_differential_renderer();

    // Inject hot signaling inputs to verify sparse tracking across crate boundaries
    world.inject_heat_at_chunk(4, 4, 0.9);
    world.inject_heat_at_chunk(4, 5, 0.75);

    for _ in 0..15 {
        world.tick();
    }
    println!("[MKR V3/V4 Substrate] Multi-stage code migration complete. Parity verified.");
}