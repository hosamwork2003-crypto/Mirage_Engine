// ===================================================================
// mirage-morphogenic/src/spatial_continuity.rs
// PURPOSE: Spatial continuity representation, accumulation, decay.
// ===================================================================

use std::collections::BTreeMap;
use mirage_core::numerics::{canonicalize_f64, CanonicalFloatPolicy, FloatNormalizationMode};

#[derive(Clone, Copy, Debug)]
pub struct SpatialContinuityState {
    pub intensity: f64,
    pub persistence: f64,
    pub resonance: f64,
}

impl PartialEq for SpatialContinuityState {
    fn eq(&self, other: &Self) -> bool {
        self.intensity.to_bits() == other.intensity.to_bits()
            && self.persistence.to_bits() == other.persistence.to_bits()
            && self.resonance.to_bits() == other.resonance.to_bits()
    }
}

impl Eq for SpatialContinuityState {}

impl Ord for SpatialContinuityState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare bit patterns for binary-exact deterministic comparison
        self.intensity.to_bits().cmp(&other.intensity.to_bits())
            .then_with(|| self.persistence.to_bits().cmp(&other.persistence.to_bits()))
            .then_with(|| self.resonance.to_bits().cmp(&other.resonance.to_bits()))
    }
}

impl PartialOrd for SpatialContinuityState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpatialContinuityField {
    pub values: BTreeMap<u64, SpatialContinuityState>,
}

impl Default for SpatialContinuityField {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialContinuityField {
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Inherit continuity from a source region to target region.
    pub fn inherit_continuity(&mut self, source: u64, target: u64, scale: f64) {
        if let Some(src_state) = self.values.get(&source).copied() {
            let policy = CanonicalFloatPolicy::default();
            let mode = FloatNormalizationMode::CanonicalNormalize;
            let intensity = canonicalize_f64(src_state.intensity * scale, &policy, mode).unwrap_or(0.0);
            let inherited = SpatialContinuityState {
                intensity,
                persistence: src_state.persistence,
                resonance: src_state.resonance,
            };
            self.values.insert(target, inherited);
        }
    }

    /// Decay continuity intensities using a deterministic decay factor.
    pub fn decay(&mut self, factor: f64) {
        let policy = CanonicalFloatPolicy::default();
        let mode = FloatNormalizationMode::CanonicalNormalize;
        let canonical_factor = canonicalize_f64(factor, &policy, mode).unwrap_or(0.0);
        for state in self.values.values_mut() {
            state.intensity = canonicalize_f64(state.intensity * canonical_factor, &policy, mode).unwrap_or(0.0);
        }
    }

    /// Stabilize continuity values by clamping to canonical bounds.
    pub fn stabilize(&mut self) {
        let policy = CanonicalFloatPolicy::default();
        let mode = FloatNormalizationMode::CanonicalNormalize;
        for state in self.values.values_mut() {
            state.intensity = canonicalize_f64(state.intensity, &policy, mode).unwrap_or(0.0);
            state.persistence = canonicalize_f64(state.persistence, &policy, mode).unwrap_or(1.0);
            state.resonance = canonicalize_f64(state.resonance, &policy, mode).unwrap_or(1.0);
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpatialContinuityPropagationDescriptor {
    pub source_region: u64,
    pub target_region: u64,
    pub factor: f64,
    pub sequence_index: u64,
}

impl PartialEq for SpatialContinuityPropagationDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_index == other.sequence_index
            && self.source_region == other.source_region
            && self.target_region == other.target_region
            && self.factor.to_bits() == other.factor.to_bits()
    }
}

impl Eq for SpatialContinuityPropagationDescriptor {}

impl Ord for SpatialContinuityPropagationDescriptor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence_index.cmp(&other.sequence_index)
            .then_with(|| self.source_region.cmp(&other.source_region))
            .then_with(|| self.target_region.cmp(&other.target_region))
            .then_with(|| self.factor.to_bits().cmp(&other.factor.to_bits()))
    }
}

impl PartialOrd for SpatialContinuityPropagationDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
pub struct SpatialContinuitySequence {
    pub propagations: Vec<SpatialContinuityPropagationDescriptor>,
}

impl SpatialContinuitySequence {
    pub fn new(propagations: Vec<SpatialContinuityPropagationDescriptor>) -> Self {
        Self { propagations }
    }

    /// Sort the propagations deterministically to avoid ordering divergence.
    pub fn stable_sort(&mut self) {
        self.propagations.sort();
    }

    /// Canonicalize the sequence.
    pub fn canonicalize(&mut self) {
        self.stable_sort();
    }

    /// Accumulate continuity changes into the field in deterministic sequence order.
    pub fn apply_propagation(&self, field: &mut SpatialContinuityField) {
        let policy = CanonicalFloatPolicy::default();
        let mode = FloatNormalizationMode::CanonicalNormalize;
        for prop in &self.propagations {
            let src_state = field.values.get(&prop.source_region).copied().unwrap_or(SpatialContinuityState {
                intensity: 0.0,
                persistence: 1.0,
                resonance: 1.0,
            });

            let delta = canonicalize_f64(src_state.intensity * prop.factor, &policy, mode).unwrap_or(0.0);

            let target_state = field.values.entry(prop.target_region).or_insert(SpatialContinuityState {
                intensity: 0.0,
                persistence: 1.0,
                resonance: 1.0,
            });

            target_state.intensity = canonicalize_f64(target_state.intensity + delta, &policy, mode).unwrap_or(0.0);
        }
    }
}
