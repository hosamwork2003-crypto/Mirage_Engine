use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, PartialEq, Eq, Copy, Serialize, Deserialize)]
pub enum RuntimeExecutionPhase {
    TopologyExtraction,
    PropagationRealization,
    ContinuityDecay,
    ReinforcementAccumulation,
    ResonancePropagation,
    ConvergenceEvaluation,
    EmergenceRealization,
    PersistenceStabilization,
    ReplaySnapshotSealing,
}

impl RuntimeExecutionPhase {
    pub fn ordinal(&self) -> u8 {
        match self {
            Self::TopologyExtraction => 0,
            Self::PropagationRealization => 1,
            Self::ContinuityDecay => 2,
            Self::ReinforcementAccumulation => 3,
            Self::ResonancePropagation => 4,
            Self::ConvergenceEvaluation => 5,
            Self::EmergenceRealization => 6,
            Self::PersistenceStabilization => 7,
            Self::ReplaySnapshotSealing => 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalRuntimePipeline {
    phases: Vec<RuntimeExecutionPhase>,
}

impl Default for CanonicalRuntimePipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl CanonicalRuntimePipeline {
    pub fn new() -> Self {
        Self {
            phases: vec![
                RuntimeExecutionPhase::TopologyExtraction,
                RuntimeExecutionPhase::PropagationRealization,
                RuntimeExecutionPhase::ContinuityDecay,
                RuntimeExecutionPhase::ReinforcementAccumulation,
                RuntimeExecutionPhase::ResonancePropagation,
                RuntimeExecutionPhase::ConvergenceEvaluation,
                RuntimeExecutionPhase::EmergenceRealization,
                RuntimeExecutionPhase::PersistenceStabilization,
                RuntimeExecutionPhase::ReplaySnapshotSealing,
            ],
        }
    }

    pub fn phases(&self) -> &[RuntimeExecutionPhase] {
        &self.phases
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePipelineExecutionDescriptor {
    pub phase: RuntimeExecutionPhase,
    pub deterministic_sequence_index: u64,
    pub originating_tick: u64,
    pub runtime_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePipelineExecutionSequence {
    descriptors: Vec<RuntimePipelineExecutionDescriptor>,
}

impl RuntimePipelineExecutionSequence {
    pub fn new(descriptors: Vec<RuntimePipelineExecutionDescriptor>) -> Self {
        Self { descriptors }
    }

    pub fn stable_sort(&mut self) {
        self.descriptors.sort_by(|a, b| {
            a.deterministic_sequence_index
                .cmp(&b.deterministic_sequence_index)
                .then_with(|| a.phase.ordinal().cmp(&b.phase.ordinal()))
        });
    }

    pub fn descriptors(&self) -> &[RuntimePipelineExecutionDescriptor] {
        &self.descriptors
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_phase_ordering() {
        let pipeline = CanonicalRuntimePipeline::new();
        let expected = [
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
        assert_eq!(pipeline.phases(), &expected[..]);
    }

    #[test]
    fn phase_sort_stability() {
        let mut seq = RuntimePipelineExecutionSequence::new(vec![
            RuntimePipelineExecutionDescriptor {
                phase: RuntimeExecutionPhase::ResonancePropagation,
                deterministic_sequence_index: 2,
                originating_tick: 10,
                runtime_epoch: 1,
            },
            RuntimePipelineExecutionDescriptor {
                phase: RuntimeExecutionPhase::TopologyExtraction,
                deterministic_sequence_index: 2,
                originating_tick: 10,
                runtime_epoch: 1,
            },
            RuntimePipelineExecutionDescriptor {
                phase: RuntimeExecutionPhase::ContinuityDecay,
                deterministic_sequence_index: 1,
                originating_tick: 10,
                runtime_epoch: 1,
            },
        ]);
        seq.stable_sort();
        let desc = seq.descriptors();
        assert_eq!(desc[0].deterministic_sequence_index, 1);
        assert_eq!(desc[1].deterministic_sequence_index, 2);
        assert_eq!(desc[1].phase, RuntimeExecutionPhase::TopologyExtraction);
        assert_eq!(desc[2].deterministic_sequence_index, 2);
        assert_eq!(desc[2].phase, RuntimeExecutionPhase::ResonancePropagation);
    }

    #[test]
    fn deterministic_pipeline_equality() {
        let pipeline1 = CanonicalRuntimePipeline::new();
        let pipeline2 = CanonicalRuntimePipeline::new();
        assert_eq!(pipeline1, pipeline2);
    }

    #[test]
    fn replay_equivalent_pipeline_sequences() {
        let seq1 = RuntimePipelineExecutionSequence::new(vec![
            RuntimePipelineExecutionDescriptor {
                phase: RuntimeExecutionPhase::ContinuityDecay,
                deterministic_sequence_index: 0,
                originating_tick: 42,
                runtime_epoch: 2,
            }
        ]);
        let seq2 = RuntimePipelineExecutionSequence::new(vec![
            RuntimePipelineExecutionDescriptor {
                phase: RuntimeExecutionPhase::ContinuityDecay,
                deterministic_sequence_index: 0,
                originating_tick: 42,
                runtime_epoch: 2,
            }
        ]);
        assert_eq!(seq1, seq2);
    }
}
