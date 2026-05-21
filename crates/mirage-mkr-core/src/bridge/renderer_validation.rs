// ===================================================================
// mirage-mkr-core/src/renderer_validation.rs
//
// V4 PASS 03:
// Differential Renderer Shadow Validation
//
// PURPOSE:
// Runs sparse renderer updates in shadow alongside the authoritative
// full-field renderer path.
//
// AUTHORITY:
// apply_to_directory() ALWAYS remains authoritative.
// apply_changed_cells() is SHADOW ONLY.
//
// The validator compares:
//
// - chunk_runtime_states
// - probability buffers
// - unchanged-cell preservation
//
// NO authoritative state is overwritten here.
// ===================================================================

use mirage_core::pool::RuntimeDirectory;
use mirage_core::runtime::ChunkState;

use crate::activation::{
    delta::FieldDeltaMask,
    field::ActivationField,
};

use crate::bridge::renderer_bridge::RendererBridge;

// ===================================================================
// CONSTANTS
// ===================================================================

pub const RENDERER_PROBABILITY_EPSILON: f32 = 1e-4;
pub const RENDERER_PROMOTION_THRESHOLD: u64 = 1000;

// ===================================================================
// DIFFERENTIAL RENDERER MODE
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferentialRendererMode {
    Disabled,
    ShadowValidation,
    DifferentialAuthoritative, // reserved
}

// ===================================================================
// PARITY REPORT
// ===================================================================

#[derive(Debug, Clone)]
pub struct RendererParityReport {
    pub mismatched_chunk_states: usize,
    pub max_probability_drift: f32,
    pub mean_probability_drift: f32,
    pub changed_cells_checked: usize,
    pub severe_divergence: bool,
}

impl Default for RendererParityReport {
    fn default() -> Self {
        Self {
            mismatched_chunk_states: 0,
            max_probability_drift: 0.0,
            mean_probability_drift: 0.0,
            changed_cells_checked: 0,
            severe_divergence: false,
        }
    }
}

// ===================================================================
// VALIDATION REPORT
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct DifferentialRendererValidationReport {
    pub ticks_run: u64,
    pub ticks_passed: u64,
    pub ticks_failed: u64,

    pub consecutive_passes: u64,

    pub severe_divergence_events: u64,

    pub peak_probability_drift: f32,
    pub peak_chunk_state_mismatches: usize,
}

impl DifferentialRendererValidationReport {
    pub fn record(&mut self, parity: &RendererParityReport) {
        self.ticks_run += 1;

        let passed =
            parity.mismatched_chunk_states == 0 &&
            !parity.severe_divergence;

        if passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }

        if parity.severe_divergence {
            self.severe_divergence_events += 1;
        }

        self.peak_probability_drift =
            self.peak_probability_drift.max(parity.max_probability_drift);

        self.peak_chunk_state_mismatches =
            self.peak_chunk_state_mismatches
                .max(parity.mismatched_chunk_states);
    }

    #[inline]
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= RENDERER_PROMOTION_THRESHOLD
            && self.severe_divergence_events == 0
    }
}

// ===================================================================
// SHADOW VALIDATOR
// ===================================================================

pub struct RendererShadowValidator {

    previous_shadow_states: Vec<ChunkState>,
    pub mode: DifferentialRendererMode,

    pub shadow_directory: RuntimeDirectory,
    pub shadow_probability_buffer: Vec<f32>,

    pub last_report: Option<RendererParityReport>,
    pub validation_report: DifferentialRendererValidationReport,

    bridge: RendererBridge,
}

impl RendererShadowValidator {
    pub fn new(total_chunks: usize) -> Self {
        Self {

            previous_shadow_states:
    vec![ChunkState::Dormant; total_chunks],
            mode: DifferentialRendererMode::Disabled,

            shadow_directory: RuntimeDirectory::new(total_chunks),

            shadow_probability_buffer: vec![0.0; total_chunks],

            last_report: None,

            validation_report:
                DifferentialRendererValidationReport::default(),

            bridge: RendererBridge::new(),
        }
    }

    #[inline]
    pub fn enable_shadow(&mut self) {
        self.mode = DifferentialRendererMode::ShadowValidation;
    }

    #[inline]
    pub fn disable(&mut self) {
        self.mode = DifferentialRendererMode::Disabled;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.mode != DifferentialRendererMode::Disabled
    }

    // ===============================================================
    // VALIDATION TICK
    // ===============================================================

    pub fn validate_tick(

        
        &mut self,
        field: &ActivationField,
        delta_mask: &FieldDeltaMask,

        authoritative_directory: &RuntimeDirectory,
        authoritative_probability_buffer: &[f32],
    ) {
        if !self.is_active() {
            return;
        }

        // -----------------------------------------------------------
        // SHADOW sparse update
        // -----------------------------------------------------------


        self.previous_shadow_states
    .copy_from_slice(
        &self.shadow_directory.chunk_runtime_states
    );

    self.previous_shadow_states.copy_from_slice(
    &self.shadow_directory.chunk_runtime_states
);
self.bridge.apply_changed_cells(
    field,
    delta_mask,
    &mut self.shadow_directory,
);

        self.bridge.update_probability_buffer_sparse(
            field,
            &mut self.shadow_probability_buffer,
            delta_mask,
        );

        // -----------------------------------------------------------
        // PARITY comparison
        // -----------------------------------------------------------

        let mut report = RendererParityReport::default();

        let mut drift_sum = 0.0;

        let shadow_states =
            &self.shadow_directory.chunk_runtime_states;

        let authoritative_states =
            &authoritative_directory.chunk_runtime_states;

        for idx in delta_mask.iter_changed() {
            if idx >= shadow_states.len() {
                break;
            }

            report.changed_cells_checked += 1;

            // -------------------------------------------------------
            // ChunkState parity
            // -------------------------------------------------------

            if shadow_states[idx] != authoritative_states[idx] {
                report.mismatched_chunk_states += 1;
                report.severe_divergence = true;
            }

            // -------------------------------------------------------
            // Probability parity
            // -------------------------------------------------------

            let drift =
                (self.shadow_probability_buffer[idx]
                    - authoritative_probability_buffer[idx])
                    .abs();

            drift_sum += drift;

            if drift > report.max_probability_drift {
                report.max_probability_drift = drift;
            }
        }

        if report.changed_cells_checked > 0 {
            report.mean_probability_drift =
                drift_sum / report.changed_cells_checked as f32;
        }

        for idx in 0..shadow_states.len() {
    if !delta_mask.is_changed(idx) {
        if shadow_states[idx]
            != self.previous_shadow_states[idx]
        {
            report.severe_divergence = true;
            report.mismatched_chunk_states += 1;
        }
    }
}

        self.validation_report.record(&report);

        self.last_report = Some(report);
    }
}

// ===================================================================
// TESTS
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use crate::activation::{
        ActivationField,
        delta::FieldDeltaMask,
    };

    #[test]
    fn validator_disabled_by_default() {
        let validator = RendererShadowValidator::new(16);

        assert!(!validator.is_active());

        assert_eq!(
            validator.validation_report.ticks_run,
            0
        );
    }

    #[test]
    fn promotion_requires_clean_history() {
        let mut report =
            DifferentialRendererValidationReport::default();

        report.consecutive_passes =
            RENDERER_PROMOTION_THRESHOLD;

        assert!(report.eligible_for_promotion());

        report.severe_divergence_events = 1;

        assert!(!report.eligible_for_promotion());
    }

    #[test]
    fn parity_detects_chunk_state_mismatch() {
        let mut field = ActivationField::new(4, 4);

        field.cells[0].execution_probability = 1.0;

        let mut validator =
            RendererShadowValidator::new(16);

        validator.enable_shadow();

        let mut mask = FieldDeltaMask::new(16);
        mask.set(0);

        let mut authoritative =
            RuntimeDirectory::new(16);

        authoritative.chunk_runtime_states[0] =
            ChunkState::Dormant;

        let authoritative_probabilities =
            vec![1.0; 16];

        validator.validate_tick(
            &field,
            &mask,
            &authoritative,
            &authoritative_probabilities,
        );

        let report =
            validator.last_report.as_ref().unwrap();

        assert!(report.severe_divergence);

        assert_eq!(
            report.mismatched_chunk_states,
            1
        );
    }
}