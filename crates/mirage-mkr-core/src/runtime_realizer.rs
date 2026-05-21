use mirage_mts::topology::TopologyGraph;
use mirage_mts::bridge::build_structural_propagation_sequence;
use mirage_morphogenic::continuity::StructuralContinuityField;
use mirage_morphogenic::propagation::{
    MorphogenicRealizer, DeterministicDecaySequence, apply_decay_sequence,
};
use mirage_morphogenic::reinforcement::ReinforcementMemoryField;
use mirage_morphogenic::resonance::{EmergenceResonanceField, ResonancePropagationSequence};
use mirage_morphogenic::convergence::StructuralConvergenceState;
use mirage_morphogenic::emergence::StructuralEmergenceState;
use mirage_morphogenic::persistence::StructuralPersistenceField;
use mirage_morphogenic::runtime_frame::{StructuralRuntimeFrame, RuntimeFrameIdentity};
use mirage_morphogenic::runtime_replay::{RuntimeReplayBuffer, RuntimeReplaySnapshot};
use crate::runtime_pipeline::RuntimeExecutionPhase;
use mirage_morphogenic::state::StructuralState;
use mirage_core::{
    canonicalize_f32, FloatNormalizationMode,
    hash_float_policy, hash_simd_policy,
    compute_platform_signature, verify_platform_compatibility,
    RuntimeDeterminismSeal,
};
use mirage_morphogenic::accumulation::CanonicalAccumulatorF32;

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeTickResult {
    pub runtime_frame: StructuralRuntimeFrame,
    pub executed_phases: Vec<RuntimeExecutionPhase>,
    pub replay_snapshot: RuntimeReplaySnapshot,
    pub runtime_epoch: u64,
}

#[derive(Clone, Debug)]
pub struct StructuralRuntimeRealizer {
    pub frame_sequence: u64,
    pub originating_tick: u64,
    pub runtime_epoch: u64,
    pub topology_generation: u64,
    pub continuity_field: StructuralContinuityField,
    pub reinforcement_field: ReinforcementMemoryField,
    pub resonance_field: EmergenceResonanceField,
    pub persistence_field: StructuralPersistenceField,
    pub replay_buffer: RuntimeReplayBuffer,
    pub emergence_state: StructuralEmergenceState,
    pub convergence_state: StructuralConvergenceState,
    pub deterministic_hash_seed: u64,
    pub float_policy: mirage_core::numerics::CanonicalFloatPolicy,
    pub simd_policy: mirage_math::DeterministicSimdPolicy,
    pub strict_replay_mode: mirage_morphogenic::replay_exactness::StrictReplayMode,
    pub expected_platform_signature: Option<mirage_core::platform_drift::PlatformDeterminismSignature>,
}

impl StructuralRuntimeRealizer {
    pub fn new(len: usize, initial_epoch: u64, seed: u64) -> Self {
        use mirage_core::numerics::CanonicalFloatPolicy;
        use mirage_math::DeterministicSimdPolicy;
        use mirage_morphogenic::replay_exactness::StrictReplayMode;

        Self {
            frame_sequence: 0,
            originating_tick: 0,
            runtime_epoch: initial_epoch,
            topology_generation: 0,
            continuity_field: StructuralContinuityField::new(len),
            reinforcement_field: ReinforcementMemoryField::new(len),
            resonance_field: EmergenceResonanceField::new(len),
            persistence_field: StructuralPersistenceField::new(len),
            replay_buffer: RuntimeReplayBuffer::new(initial_epoch),
            emergence_state: StructuralEmergenceState::new(0.0, 0.0, 0.0, 0.0),
            convergence_state: StructuralConvergenceState::compute_convergence(0.0, 0.0, 0.0, 0.0),
            deterministic_hash_seed: seed,
            float_policy: CanonicalFloatPolicy::default(),
            simd_policy: DeterministicSimdPolicy {
                simd_enabled: true,
                scalar_fallback_required: false,
                deterministic_lane_ordering: true,
                stable_reduction_required: true,
            },
            strict_replay_mode: StrictReplayMode {
                exact_replay_required: true,
                byte_equivalence_required: true,
                deterministic_hash_required: true,
                canonical_serialization_required: true,
            },
            expected_platform_signature: None,
        }
    }

    pub fn execute_canonical_runtime_tick(
        &mut self,
        topology: &TopologyGraph,
        resonance_sequence: &ResonancePropagationSequence,
        decay_sequence: &DeterministicDecaySequence,
    ) -> RuntimeTickResult {
        let mut executed_phases = Vec::with_capacity(9);

        // 1. Topology Extraction
        executed_phases.push(RuntimeExecutionPhase::TopologyExtraction);
        let propagation_sequence = build_structural_propagation_sequence(
            topology,
            self.originating_tick,
            self.topology_generation,
            self.runtime_epoch,
        );

        // 2. Propagation Realization
        executed_phases.push(RuntimeExecutionPhase::PropagationRealization);
        let base_snapshot = self.continuity_field.snapshot(self.runtime_epoch);
        let structural_state = StructuralState::default();
        let realized_snapshot = MorphogenicRealizer::realize_sequence(
            self.originating_tick,
            &base_snapshot,
            &propagation_sequence,
            &structural_state,
        );
        self.continuity_field.apply_snapshot(&realized_snapshot);

        // 3. Continuity Decay
        executed_phases.push(RuntimeExecutionPhase::ContinuityDecay);
        apply_decay_sequence(&mut self.continuity_field, decay_sequence);

        // 4. Reinforcement Accumulation
        executed_phases.push(RuntimeExecutionPhase::ReinforcementAccumulation);
        for desc in propagation_sequence.descriptors.iter() {
            self.reinforcement_field.accumulate(desc.target_node, desc.reinforcement_weight);
        }

        // 5. Resonance Propagation
        executed_phases.push(RuntimeExecutionPhase::ResonancePropagation);
        for desc in resonance_sequence.descriptors.iter() {
            if desc.target_index < self.resonance_field.len() {
                let current_val = self.resonance_field.get(desc.target_index).unwrap_or(0.0);
                self.resonance_field.set(desc.target_index, (current_val + desc.weight).clamp(0.0, 1.0));
            }
        }
        self.resonance_field.smoothing_pass();

        // 6. Convergence Evaluation
        executed_phases.push(RuntimeExecutionPhase::ConvergenceEvaluation);
        let len = self.continuity_field.len();
        let continuity_pressure = if len > 0 {
            let mut acc = CanonicalAccumulatorF32::new(self.runtime_epoch);
            for i in 0..len {
                acc.push(i as u64, self.continuity_field.get(i).unwrap_or(0.0));
            }
            let raw_pressure = acc.stable_sum() / len as f32;
            canonicalize_f32(raw_pressure, &self.float_policy, FloatNormalizationMode::ClampOnly).unwrap_or(0.0)
        } else {
            0.0
        };

        let reinforcement_pressure = if len > 0 {
            let mut acc = CanonicalAccumulatorF32::new(self.runtime_epoch);
            for i in 0..len {
                acc.push(i as u64, self.reinforcement_field.get(i).map(|c| c.accumulated_weight).unwrap_or(0.0));
            }
            let raw_pressure = acc.stable_sum() / len as f32;
            canonicalize_f32(raw_pressure, &self.float_policy, FloatNormalizationMode::ClampOnly).unwrap_or(0.0)
        } else {
            0.0
        };

        let resonance_pressure = if len > 0 {
            let mut acc = CanonicalAccumulatorF32::new(self.runtime_epoch);
            for i in 0..len {
                acc.push(i as u64, self.resonance_field.get(i).unwrap_or(0.0));
            }
            let raw_pressure = acc.stable_sum() / len as f32;
            canonicalize_f32(raw_pressure, &self.float_policy, FloatNormalizationMode::ClampOnly).unwrap_or(0.0)
        } else {
            0.0
        };

        let stabilization_pressure = if len > 0 {
            let mut acc = CanonicalAccumulatorF32::new(self.runtime_epoch);
            for i in 0..len {
                acc.push(i as u64, self.persistence_field.get(i).unwrap_or(0.0));
            }
            let raw_pressure = acc.stable_sum() / len as f32;
            canonicalize_f32(raw_pressure, &self.float_policy, FloatNormalizationMode::ClampOnly).unwrap_or(0.0)
        } else {
            0.0
        };

        self.convergence_state = StructuralConvergenceState::compute_convergence(
            continuity_pressure,
            reinforcement_pressure,
            resonance_pressure,
            stabilization_pressure,
        );

        // 7. Emergence Realization
        executed_phases.push(RuntimeExecutionPhase::EmergenceRealization);
        let score_raw = continuity_pressure * 0.4 + resonance_pressure * 0.6;
        let score = canonicalize_f32(score_raw, &self.float_policy, FloatNormalizationMode::ClampOnly).unwrap_or(0.0);
        let convergence_raw = (reinforcement_pressure * continuity_pressure).sqrt();
        let convergence = canonicalize_f32(convergence_raw, &self.float_policy, FloatNormalizationMode::ClampOnly).unwrap_or(0.0);

        self.emergence_state = StructuralEmergenceState::new(
            score,
            resonance_pressure,
            stabilization_pressure,
            convergence,
        );

        // 8. Persistence Stabilization
        executed_phases.push(RuntimeExecutionPhase::PersistenceStabilization);
        for i in 0..len {
            let current = self.persistence_field.get(i).unwrap_or(0.0);
            let increment = self.emergence_state.stabilization_factor * 0.05;
            self.persistence_field.set(i, (current + increment).clamp(0.0, 1.0));
        }
        self.persistence_field.decay(0.98);

        // 9. Replay Snapshot Sealing
        executed_phases.push(RuntimeExecutionPhase::ReplaySnapshotSealing);
        let final_continuity_snapshot = self.continuity_field.snapshot(self.runtime_epoch);
        let final_resonance_snapshot = self.resonance_field.snapshot(self.runtime_epoch);

        // Compute signatures & check drift
        let float_policy_hash = hash_float_policy(&self.float_policy);
        let simd_policy_hash = hash_simd_policy(&self.simd_policy);
        let mut runtime_policy_bytes = Vec::new();
        runtime_policy_bytes.push(self.strict_replay_mode.exact_replay_required as u8);
        runtime_policy_bytes.push(self.strict_replay_mode.byte_equivalence_required as u8);
        runtime_policy_bytes.push(self.strict_replay_mode.deterministic_hash_required as u8);
        runtime_policy_bytes.push(self.strict_replay_mode.canonical_serialization_required as u8);
        let runtime_policy_hash = mirage_core::numerics::hash_bytes(&runtime_policy_bytes);

        let current_sig = compute_platform_signature(
            float_policy_hash,
            simd_policy_hash,
            runtime_policy_hash,
        );

        let drift_detected = if let Some(ref expected) = self.expected_platform_signature {
            let report = verify_platform_compatibility(&current_sig, expected);
            report.drift_detected
        } else {
            false
        };

        let replay_identity = RuntimeFrameIdentity {
            frame_sequence: self.frame_sequence,
            originating_tick: self.originating_tick,
            deterministic_hash_seed: self.deterministic_hash_seed,
        };

        let dummy_seal = RuntimeDeterminismSeal {
            runtime_hash: 0,
            frame_hash: 0,
            replay_hash: 0,
            policy_hash: 0,
            platform_signature: current_sig.clone(),
        };

        let mut frame = StructuralRuntimeFrame {
            frame_sequence: self.frame_sequence,
            originating_tick: self.originating_tick,
            runtime_epoch: self.runtime_epoch,
            topology_generation: self.topology_generation,
            continuity_snapshot: final_continuity_snapshot,
            emergence_state: self.emergence_state.clone(),
            resonance_snapshot: final_resonance_snapshot,
            convergence_state: self.convergence_state.clone(),
            persistence_snapshot: self.persistence_field.clone(),
            replay_identity: replay_identity.clone(),
            determinism_seal: dummy_seal,
            canonical_numeric_state: true,
            replay_exactness_verified: !drift_detected,
        };

        let policy_hash = mirage_core::numerics::hash_u64(
            runtime_policy_hash,
            mirage_core::numerics::hash_u64(
                simd_policy_hash,
                mirage_core::numerics::hash_u64(float_policy_hash, 0)
            )
        );

        let frame_bytes = mirage_morphogenic::canonical_serialization::canonicalize_runtime_frame_bytes(&frame);
        let frame_hash = mirage_core::numerics::hash_bytes(&frame_bytes);

        self.replay_buffer.push_frame(frame.clone());
        let replay_bytes = mirage_morphogenic::canonical_serialization::canonicalize_replay_bytes(&self.replay_buffer);
        let replay_hash = mirage_core::numerics::hash_bytes(&replay_bytes);

        let runtime_hash = mirage_core::numerics::hash_u64(
            self.deterministic_hash_seed,
            mirage_core::numerics::hash_u64(
                policy_hash,
                mirage_core::numerics::hash_u64(
                    replay_hash,
                    mirage_core::numerics::hash_u64(frame_hash, 0)
                )
            )
        );

        let seal = RuntimeDeterminismSeal {
            runtime_hash,
            frame_hash,
            replay_hash,
            policy_hash,
            platform_signature: current_sig,
        };

        frame.determinism_seal = seal.clone();
        if let Some(last_frame) = self.replay_buffer.frames_mut().last_mut() {
            last_frame.determinism_seal = seal;
        }

        let replay_snapshot = self.replay_buffer.seal(replay_identity);

        let result = RuntimeTickResult {
            runtime_frame: frame,
            executed_phases,
            replay_snapshot,
            runtime_epoch: self.runtime_epoch,
        };

        self.frame_sequence += 1;
        self.originating_tick += 1;

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirage_mts::topology::{TopologyNode, ExecutionLane};

    fn test_topology() -> TopologyGraph {
        let mut g = TopologyGraph::new();
        // Add nodes
        g.nodes.push(TopologyNode {
            id: 0,
            thermal_state: mirage_core::runtime::ChunkState::Hot,
            execution_lane: ExecutionLane::Background,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 1.0,
            activation_pull: 0.8,
            cache_pressure: 0.5,
        });
        g.nodes.push(TopologyNode {
            id: 1,
            thermal_state: mirage_core::runtime::ChunkState::Hot,
            execution_lane: ExecutionLane::Background,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 1.0,
            activation_pull: 0.4,
            cache_pressure: 0.2,
        });
        g.edges.push(vec![1]);
        g.edges.push(vec![]);
        g
    }

    #[test]
    fn canonical_tick_execution_order() {
        let mut realizer = StructuralRuntimeRealizer::new(2, 1, 12345);
        let topo = test_topology();
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let result = realizer.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);

        let expected_order = vec![
            RuntimeExecutionPhase::TopologyExtraction,
            RuntimeExecutionPhase::PropagationRealization,
            RuntimeExecutionPhase::ContinuityDecay,
            RuntimeExecutionPhase::ReinforcementAccumulation,
            RuntimeExecutionPhase::ResonancePropagation,
            RuntimeExecutionPhase::ConvergenceEvaluation,
            RuntimeExecutionPhase::EmergenceRealization,
            RuntimeExecutionPhase::PersistenceStabilization,
            RuntimeExecutionPhase::ReplaySnapshotSealing,
        ];
        assert_eq!(result.executed_phases, expected_order);
    }

    #[test]
    fn deterministic_tick_equivalence() {
        let mut r1 = StructuralRuntimeRealizer::new(2, 1, 12345);
        let mut r2 = StructuralRuntimeRealizer::new(2, 1, 12345);
        let topo = test_topology();
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let res1 = r1.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);
        let res2 = r2.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);

        assert_eq!(res1, res2);
    }

    #[test]
    fn replay_safe_tick_execution() {
        let mut realizer = StructuralRuntimeRealizer::new(2, 1, 12345);
        let topo = test_topology();
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let res1 = realizer.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);
        let res2 = realizer.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);

        assert_ne!(res1.runtime_frame.originating_tick, res2.runtime_frame.originating_tick);
        assert_eq!(res1.replay_snapshot.sealed_frames.len(), 1);
        assert_eq!(realizer.replay_buffer.canonical_frame_count, 2);
    }

    #[test]
    fn stable_runtime_frame_generation() {
        let mut realizer = StructuralRuntimeRealizer::new(2, 1, 12345);
        let topo = test_topology();
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let res = realizer.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);

        assert_eq!(res.runtime_frame.frame_sequence, 0);
        assert_eq!(res.runtime_frame.originating_tick, 0);
        assert_eq!(res.runtime_frame.runtime_epoch, 1);
        assert_eq!(res.runtime_frame.topology_generation, 0);
    }

    #[test]
    fn runtime_float_canonicalization() {
        let mut realizer = StructuralRuntimeRealizer::new(2, 1, 12345);
        realizer.float_policy.deterministic_rounding_precision = 1; // round to 1 decimal place
        
        realizer.continuity_field.set(0, 0.123);
        realizer.continuity_field.set(1, 0.456);
        // raw average = (0.123 + 0.456)/2 = 0.2895. With precision 1, it should round to 0.3.

        let mut topo = test_topology();
        topo.edges = vec![vec![], vec![]];
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let result = realizer.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);
        assert_eq!(result.runtime_frame.convergence_state.continuity_pressure, 0.3);
    }

    #[test]
    fn runtime_exact_replay() {
        let mut r1 = StructuralRuntimeRealizer::new(2, 1, 12345);
        let mut r2 = StructuralRuntimeRealizer::new(2, 1, 12345);
        let topo = test_topology();
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let res1 = r1.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);
        let res2 = r2.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);

        let bytes1 = mirage_morphogenic::canonical_serialization::canonicalize_runtime_frame_bytes(&res1.runtime_frame);
        let bytes2 = mirage_morphogenic::canonical_serialization::canonicalize_runtime_frame_bytes(&res2.runtime_frame);
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn runtime_signature_stability() {
        let realizer = StructuralRuntimeRealizer::new(2, 1, 12345);
        let float_policy_hash = hash_float_policy(&realizer.float_policy);
        let simd_policy_hash = hash_simd_policy(&realizer.simd_policy);
        let mut runtime_policy_bytes = Vec::new();
        runtime_policy_bytes.push(realizer.strict_replay_mode.exact_replay_required as u8);
        runtime_policy_bytes.push(realizer.strict_replay_mode.byte_equivalence_required as u8);
        runtime_policy_bytes.push(realizer.strict_replay_mode.deterministic_hash_required as u8);
        runtime_policy_bytes.push(realizer.strict_replay_mode.canonical_serialization_required as u8);
        let runtime_policy_hash = mirage_core::numerics::hash_bytes(&runtime_policy_bytes);

        let sig = compute_platform_signature(float_policy_hash, simd_policy_hash, runtime_policy_hash);
        assert_eq!(sig.float_policy_hash, float_policy_hash);
        assert_eq!(sig.simd_policy_hash, simd_policy_hash);
    }

    #[test]
    fn deterministic_runtime_hashing() {
        let mut r1 = StructuralRuntimeRealizer::new(2, 1, 12345);
        let mut r2 = StructuralRuntimeRealizer::new(2, 1, 54321); // different seed
        let topo = test_topology();
        let resonance_seq = ResonancePropagationSequence::new();
        let decay_seq = DeterministicDecaySequence::new();

        let res1 = r1.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);
        let res2 = r2.execute_canonical_runtime_tick(&topo, &resonance_seq, &decay_seq);

        assert_ne!(res1.runtime_frame.determinism_seal.runtime_hash, res2.runtime_frame.determinism_seal.runtime_hash);
    }
}
