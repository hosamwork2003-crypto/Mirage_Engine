# MKR Wiring Report — Activation Engine Integration Pass

> **Compile status:** ✅ Zero errors — `cargo check -p mirage-mkr-core`  
> **Test status:** ✅ 35/35 tests pass — `cargo test -p mirage-mkr-core`

---

## Task 1 — Topology Wiring

### What was done

`TopologyGraph` (from `mirage-matrix`) is now an **owned field** of `MKRWorld` and is consulted in every tick.

**Files changed:**
- `crates/mirage-mkr-core/Cargo.toml` — added `mirage-matrix = { path = "../mirage-matrix" }` dependency
- `crates/mirage-mkr-core/src/main.rs` — added `topology: TopologyGraph` field; wired Phase 0

**The bridge (Phase 0 of `MKRWorld::tick()`):**
```rust
// Phase 0 — Topology influence pre-pass
let topo_influence = self.topology.influence_scalars();

// Phase 1 — Activation field step
self.last_step_stats = self.activation_solver.step(
    &mut self.activation_field,
    &topo_influence,    // ← previously &[]
);
```

### Data flow
```
TopologyNode::activation_pull (f32, manually set per node)
    ↓  TopologyGraph::influence_scalars()
Vec<f32>  (one entry per node, clamped 0..1)
    ↓  ActivationSolver::propagate_pressure()
ActivationCell::pressure  (blended 30% topology, 70% prior)
    ↓  recompute_activation()
ActivationCell::activation
    ↓  recompute_execution_probability()
ActivationCell::execution_probability
```

### Public API added to `MKRWorld`
```rust
pub fn topology_mut(&mut self) -> &mut TopologyGraph
```
External systems (streaming, physics, renderer) set `activation_pull` on nodes and add directed edges through this accessor.

### Test proving wiring is live
```
test tests::topology_influence_reaches_field ... ok
```
Sets `activation_pull = 1.0` on node 0, runs one tick, asserts `cells[0].pressure > 0.0`.

### Known limitation / TODO
`TopologyGraph::influence_scalars()` currently returns each node's own `activation_pull` directly (stub). Real directed edge-weight accumulation (sum in-edge pulls, normalise by max in-degree) is marked `TODO(V3-TOPOLOGY)`. The wiring is correct and live; the graph math is the next implementation step.

---

## Task 2 — Fiber Emission Gate

### What was done

New file: **`crates/mirage-mkr-core/src/emission.rs`**

`EmissionGate` scans `execution_probability` each tick and produces a bounded, probability-sorted list of `EmissionRequest`s.

**Integrated as Phase 2 of `MKRWorld::tick()`:**
```rust
// Phase 2 — Fiber emission gate
let requests = self.emission_gate.collect(&self.activation_field);
self.last_emission.clear();
self.last_emission.extend_from_slice(requests);
```

### Constants

| Constant | Value | Meaning |
|---|---|---|
| `EMIT_GATE` | `0.05` | Min `execution_probability` for emission eligibility |
| `MAX_EMIT_PER_TICK` | `128` | Hard per-frame spawn budget |

### `EmissionRequest` struct
```rust
pub struct EmissionRequest {
    pub cell_index:  usize,   // flat field index = chunk index
    pub probability: f32,     // raw probability at emission time
}
```

### Algorithm (O(n) field scan + O(n) partial sort)
1. Scan all cells; push those with `probability > EMIT_GATE` into pre-allocated scratch
2. If scratch > budget: `select_nth_unstable_by` (O(n) average) — no full sort needed
3. Truncate to `MAX_EMIT_PER_TICK`
4. Return immutable slice; caller clones into `MKRWorld::last_emission`

### Why branchless-structured
The inner loop has one conditional write:
```rust
if cell.execution_probability > EMIT_GATE {
    self.scratch.push(...)
}
```
This is correctly branch-predicted as "not-taken" for dormant cells. The structure is SIMD/GPU-portable: on GPU each thread writes to a pre-sized emit list slot via `atomicAdd`, eliminating the branch entirely.

### Public API on `MKRWorld`
```rust
pub fn emission_requests(&self) -> &[EmissionRequest]
```

### Tests (5 passing)
```
test emission::tests::dormant_field_emits_nothing ... ok
test emission::tests::hot_field_emits_up_to_budget ... ok
test emission::tests::gate_filters_below_threshold ... ok
test emission::tests::emission_requests_are_probability_ordered ... ok
test emission::tests::cell_index_matches_field_position ... ok
test tests::emission_gate_produces_requests_when_hot ... ok
test tests::emission_requests_cleared_each_tick ... ok
```

### CEK integration point
`last_emission` is the **exact data structure CEK will consume**. When CEK is implemented, Phase 2 becomes a channel send instead of a Vec clone, with no API change visible to callers.

---

## Task 3 — Field-to-Renderer Bridge

### What was done

New files:
- **`crates/mirage-mkr-core/src/bridge/mod.rs`** — module root
- **`crates/mirage-mkr-core/src/bridge/renderer_bridge.rs`** — `RendererBridge` + free translation kernel

**Integrated as Phase 3 of `MKRWorld::tick()`:**
```rust
// Phase 3 — Renderer bridge (field → chunk_runtime_states)
if self.directory.chunk_runtime_states.len() == self.activation_field.len() {
    self.renderer_bridge.apply_to_directory(
        &self.activation_field,
        &mut self.directory,
    );
}
```

`MKRWorld::directory` was changed from `crate::pool::RuntimeDirectory` (the local stub, no `chunk_runtime_states`) to `mirage_core::pool::RuntimeDirectory` (which owns `chunk_runtime_states` and `get_raw_states()`).

### Translation kernel (`probability_to_chunk_state`)

```rust
pub fn probability_to_chunk_state(probability: f32) -> ChunkState {
    if probability >= 0.70 { ChunkState::Hot }
    else if probability >= 0.35 { ChunkState::Resident }
    else if probability >= 0.05 { ChunkState::Predictive }
    else { ChunkState::Dormant }
}
```

| Threshold | ChunkState | Intent |
|---|---|---|
| ≥ 0.70 | Hot | Full simulation eligible |
| ≥ 0.35 | Resident | In VRAM, light simulation |
| ≥ 0.05 | Predictive | Async streaming eligible |
| < 0.05 | Dormant | Skip entirely |

Thresholds are named constants `BRIDGE_HOT_THRESHOLD`, `BRIDGE_RESIDENT_THRESHOLD`, `BRIDGE_PREDICTIVE_THRESHOLD` for easy calibration.

### Override semantics
The bridge **unconditionally overwrites** `chunk_runtime_states` with V3-derived values. Any distance-based state the renderer wrote last frame is superseded. This is intentional — the activation field is the V3 authority.

### Forward-looking: continuous float buffer
```rust
pub fn fill_probability_buffer(&self, field: &ActivationField, output: &mut Vec<f32>)
```
When the GPU shader is updated to consume a `Vec<f32>` float buffer instead of a u32 enum buffer, this method produces the correct data. No API change to the renderer is needed at that point — just swap the buffer being passed to `renderer.update_states_buffer()`.

### Helper predicates (for streaming layer)
```rust
pub fn should_render(&self, probability: f32) -> bool     // ≥ BRIDGE_RESIDENT
pub fn should_stream(&self, probability: f32) -> bool     // ≥ BRIDGE_PREDICTIVE && < BRIDGE_RESIDENT
pub fn is_emission_eligible(&self, probability: f32) -> bool  // > EMIT_GATE
```

### Tests (5 passing)
```
test bridge::renderer_bridge::tests::probability_mapping_boundaries ... ok
test bridge::renderer_bridge::tests::apply_to_directory_full_hot ... ok
test bridge::renderer_bridge::tests::apply_to_directory_dormant_field ... ok
test bridge::renderer_bridge::tests::fill_probability_buffer_matches_field ... ok
test bridge::renderer_bridge::tests::helper_predicates ... ok
test tests::renderer_bridge_overrides_states_after_tick ... ok
```

---

## Task 4 — Stub Annotation

### `mirage-core/src/pool.rs`
Expanded from minimal stub to full compatible `RuntimeDirectory` with:
- `Handle` (generation-tracked index, was `RuntimeHandle`)
- `AddressMapping`
- `register_entity(uuid, page_id, chunk_idx, slot_idx)` — uuid param retained for `NeuralCluster` macro compat
- `get_mapping(handle) -> Option<AddressMapping>`
- `get_raw_states() -> Vec<u32>` — for GPU buffer upload
- `chunk_runtime_states: Vec<ChunkState>` — bridge target

**V3 TODO annotations:**

```rust
// TODO(V3): [Handle] Replace with FieldCellHandle(usize) — direct index
//   into ActivationField::cells. Generation tracking moves to a separate
//   sparse generational array consulted only during streaming I/O.

// TODO(V3): [AddressMapping] Replace with:
//   StreamingDescriptor { field_index: usize, oasis_page_id: u32 }
//   so streaming layer and activation field share one primary key space.

// TODO(V3): [RuntimeDirectory] Once renderer reads execution_probability
//   directly, collapse to: FieldIndex→OasisPageId lookup flat array.

// TODO(V3): Remove _uuid from register_entity once NeuralCluster macro
//   is updated to use field-cell indices.
```

### `mirage-core/src/oasis.rs`
Expanded from uuid-only stub to full `OasisManager` with `OasisVirtualPage` and `load_chunk_data()` (required by `mirage-renderer/src/main.rs`).

**V3 TODO annotations:**

```rust
// TODO(V3): [OasisManager] Become `StreamingCoordinator`:
//   - queue_stream(field_index, oasis_page_id) -> StreamHandle
//   - poll_ready() -> impl Iterator<Item = (field_index, Vec<u8>)>
//   Completion injects heat into MKRWorld::inject_heat_at_chunk(),
//   eliminating the manual streaming loop in renderer/main.rs.

// TODO(V3): [MirageUuid] Replace chunk-level addressing with
//   field cell indices. Retain MirageUuid only for serialised
//   asset manifests (pak files, save data).
```

---

## Full Tick Phase Order (Post-Wiring)

```
MKRWorld::tick()
├── Phase 0  topology.influence_scalars()            → Vec<f32>
├── Phase 1  activation_solver.step(field, &topo)   → SolverStepStats
│   ├── field.decay()
│   ├── field.diffuse()
│   ├── solver.propagate_pressure(field, topo)
│   ├── field.recompute_activation()
│   └── field.recompute_execution_probability()
├── Phase 2  emission_gate.collect(field)            → &[EmissionRequest]
│   └── → copied into last_emission
├── Phase 3  renderer_bridge.apply_to_directory()   → chunk_runtime_states
├── Phase 4  [COMPAT] thermal.update_frame()
└── Phase 5  [COMPAT] stabilisation hook
```

---

## Remaining Warnings (3, all pre-existing)

| File | Warning | Action |
|---|---|---|
| `mirage-core/src/runtime.rs:53` | `AtomicU8`, `Ordering` unused | Pre-existing V2 import; leave for now |
| `mirage-mkr-core/src/pool/mod.rs:47` | `free_slots` never read | Harmless compat stub field |

---

## What is Still Missing (Roadmap)

| Item | Where | Blocker |
|---|---|---|
| Real edge-weight accumulation in `influence_scalars()` | `topology.rs` | None — implement as directed graph traversal |
| `EmissionRequest` → `FiberPool::spawn()` | `main.rs Phase 2` | CEK protocol not yet designed |
| Streaming completion → `inject_heat_at_chunk()` | `OasisManager` | CEK / StreamingCoordinator design |
| Renderer reads `fill_probability_buffer()` directly | `renderer/main.rs` | GPU shader update required |
| Physics reads `execution_probability` as sim factor | `physics/lib.rs` | Bridge planned in `bridge/executor_bridge.rs` |
