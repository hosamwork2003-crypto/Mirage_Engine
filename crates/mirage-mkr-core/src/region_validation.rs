// ===================================================================
// mirage-mkr-core/src/region_validation.rs
//
// V4 PASS 04:
// Differential Region Shadow Validation
// ===================================================================

use crate::activation::{
    delta::FieldDeltaMask,
    field::ActivationField,
};

use crate::regions::{
    RegionMap,
};

// ===================================================================
// CONSTANTS
// ===================================================================

pub const REGION_PROMOTION_THRESHOLD: u64 = 1000;

// ===================================================================
// MODE
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferentialRegionMode {
    Disabled,
    ShadowValidation,
    DifferentialAuthoritative, // reserved
}

// ===================================================================
// PARITY REPORT
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct RegionParityReport {
    pub mismatched_region_states: usize,

    pub active_region_count_match: bool,
    pub dormant_region_count_match: bool,
    pub hot_region_count_match: bool,

    pub region_transitions_checked: usize,

    pub severe_divergence: bool,
}

// ===================================================================
// VALIDATION REPORT
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct DifferentialRegionValidationReport {
    pub ticks_run: u64,
    pub ticks_passed: u64,
    pub ticks_failed: u64,

    pub consecutive_passes: u64,

    pub severe_divergence_events: u64,

    pub peak_mismatched_regions: usize,
}

impl DifferentialRegionValidationReport {
    pub fn record(
        &mut self,
        parity: &RegionParityReport,
    ) {
        self.ticks_run += 1;

        let passed =
            parity.mismatched_region_states == 0
            && !parity.severe_divergence;

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

        self.peak_mismatched_regions =
            self.peak_mismatched_regions
                .max(parity.mismatched_region_states);
    }

    #[inline]
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= REGION_PROMOTION_THRESHOLD
            && self.severe_divergence_events == 0
    }
}

// ===================================================================
// SHADOW VALIDATOR
// ===================================================================

pub struct RegionShadowValidator {
    pub mode: DifferentialRegionMode,

    pub shadow_region_map: RegionMap,

    pub last_report: Option<RegionParityReport>,

    pub validation_report:
        DifferentialRegionValidationReport,
}

impl RegionShadowValidator {
    pub fn new(
        width: usize,
        height: usize,
    ) -> Self {
        Self {
            mode: DifferentialRegionMode::Disabled,

            shadow_region_map:
                RegionMap::new(width, height),

            last_report: None,

            validation_report:
                DifferentialRegionValidationReport::default(),
        }
    }

    #[inline]
    pub fn enable_shadow(&mut self) {
        self.mode =
            DifferentialRegionMode::ShadowValidation;
    }

    #[inline]
    pub fn disable(&mut self) {
        self.mode =
            DifferentialRegionMode::Disabled;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.mode
            != DifferentialRegionMode::Disabled
    }

    // ===============================================================
    // VALIDATION TICK
    // ===============================================================

    pub fn validate_tick(
        &mut self,
        field: &ActivationField,
        delta_mask: &FieldDeltaMask,

        authoritative_regions: &RegionMap,
    ) {
        if !self.is_active() {
            return;
        }

        // -----------------------------------------------------------
        // Shadow sparse refresh
        // -----------------------------------------------------------

        self.shadow_region_map
            .refresh_changed_regions(
                field,
                delta_mask,
            );

        // -----------------------------------------------------------
        // Parity comparison
        // -----------------------------------------------------------

        let mut report =
            RegionParityReport::default();

        for idx in 0..authoritative_regions.region_count()
        {
            let shadow =
                self.shadow_region_map.get(idx);

            let authoritative =
                authoritative_regions.get(idx);

            if let (
                Some(shadow),
                Some(authoritative),
            ) = (shadow, authoritative)
            {
                report.region_transitions_checked += 1;

                if shadow.activity
                    != authoritative.activity
                {
                    report
                        .mismatched_region_states += 1;

                    report.severe_divergence = true;
                }
            }
        }

        let shadow_stats =
            self.shadow_region_map.activity_stats();

        let authoritative_stats =
            authoritative_regions.activity_stats();

        report.active_region_count_match =
            shadow_stats.active
                == authoritative_stats.active;

        report.dormant_region_count_match =
            shadow_stats.dormant
                == authoritative_stats.dormant;

        report.hot_region_count_match =
            shadow_stats.hot
                == authoritative_stats.hot;

        self.validation_report.record(&report);

        self.last_report = Some(report);
    }
}