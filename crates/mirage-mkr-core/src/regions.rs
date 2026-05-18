// ===================================================================
// mirage-mkr-core/src/regions.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Activation Regions — Lightweight Grid-Aligned Execution Islands
//
// ---------------------------------------------------------------
// DESIGN INTENT
// ---------------------------------------------------------------
//
// The activation field is currently a flat, undifferentiated array.
// Regional partitioning groups nearby cells into fixed-size tiles,
// enabling:
//   * O(regions) activity scan instead of O(cells)
//   * Region-local streaming eligibility decisions
//   * Future continuation locality (CEK: one continuation per region)
//   * Regional scheduling budget (emission budget per active region)
//
// GRID ALIGNMENT:
//   The field is partitioned into REGION_SIZE × REGION_SIZE tiles.
//   If the field is not divisible by REGION_SIZE, boundary regions
//   are smaller (partial regions are supported).
//
// ACTIVITY CLASSIFICATION:
//   Dormant   — mean probability below DORMANT_THRESHOLD
//   Warming   — mean probability above DORMANT_THRESHOLD
//   Active    — any cell above ACTIVE_THRESHOLD
//   Hot       — any cell above HOT_THRESHOLD
//
// NO ECS, NO GRAPH PARTITIONING, NO ASYNC.
// This is a simple grid scan over a flat Vec<ActivationCell>.
//
// TODO(V3-DIFFERENTIAL): RegionActivityState replaces the full-field
// emission scan.  When a region is Dormant, skip all cells in it.
// When a region is Warming, only scan its frontier cells.
// When a region is Active/Hot, scan all cells in it.
//
// TODO(V3-CEK): Each Active region will map to a CEK continuation
// locality domain — one continuation slot per region per tick.
// ===================================================================

use crate::activation::field::ActivationField;

// =====================================================================
// CONSTANTS
// =====================================================================

/// Side length of a grid-aligned region (in cells).
/// 8×8 = 64 cells per region — fits in a single cache line burst.
pub const REGION_SIZE: usize = 8;

/// Mean probability below this → region is Dormant.
pub const DORMANT_THRESHOLD: f32 = 0.02;

/// Any cell above this → region is at least Active.
pub const ACTIVE_THRESHOLD: f32 = 0.15;

/// Any cell above this → region is Hot.
pub const HOT_THRESHOLD: f32 = 0.50;

// =====================================================================
// REGION ACTIVITY STATE
// =====================================================================

/// Coarse activity classification of a region.
///
/// Used to gate downstream systems at region granularity before
/// doing per-cell work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RegionActivityState {
    /// All cells effectively dormant — skip this region entirely.
    #[default]
    Dormant  = 0,
    /// Some cells warming up — scan frontier only.
    Warming  = 1,
    /// At least one cell is meaningfully active — full region scan.
    Active   = 2,
    /// At least one cell is highly active — priority execution region.
    Hot      = 3,
}

impl RegionActivityState {
    /// True if this region requires any execution work this tick.
    #[inline]
    pub fn needs_execution(self) -> bool {
        matches!(self, RegionActivityState::Active | RegionActivityState::Hot)
    }

    /// True if this region requires streaming consideration.
    #[inline]
    pub fn needs_streaming(self) -> bool {
        !matches!(self, RegionActivityState::Dormant)
    }
}

// =====================================================================
// REGION BOUNDS
// =====================================================================

/// Grid-aligned bounds of a single region in the activation field.
///
/// Cell indices within the region range from
/// `(y * field_width + x)` for each `(x, y)` in
/// `[x_start, x_end) × [y_start, y_end)`.
#[derive(Debug, Clone, Copy)]
pub struct RegionBounds {
    /// Inclusive start column (cell x-coordinate).
    pub x_start: usize,
    /// Exclusive end column.
    pub x_end:   usize,
    /// Inclusive start row (cell y-coordinate).
    pub y_start: usize,
    /// Exclusive end row.
    pub y_end:   usize,
    /// Field width (for flat-index computation).
    pub field_width: usize,
}

impl RegionBounds {
    /// Iterate over flat cell indices within this region.
    pub fn iter_cells(&self) -> impl Iterator<Item = usize> + '_ {
        let fw = self.field_width;
        (self.y_start..self.y_end).flat_map(move |y| {
            (self.x_start..self.x_end).map(move |x| y * fw + x)
        })
    }

    /// Number of cells in this region.
    #[inline]
    pub fn cell_count(&self) -> usize {
        (self.x_end - self.x_start) * (self.y_end - self.y_start)
    }
}

// =====================================================================
// ACTIVATION REGION
// =====================================================================

/// A single grid-aligned region in the activation field.
///
/// Owned by `RegionMap` — not constructed standalone.
#[derive(Debug, Clone, Copy)]
pub struct ActivationRegion {
    /// Flat region index (row-major: `ry * regions_wide + rx`).
    pub region_idx:  usize,
    /// Grid-aligned bounds.
    pub bounds:      RegionBounds,
    /// Activity state computed last tick.
    pub activity:    RegionActivityState,
    /// Mean execution_probability across all cells in this region.
    pub mean_probability: f32,
    /// Peak execution_probability in this region.
    pub peak_probability: f32,
    /// Number of cells above ACTIVE_THRESHOLD in this region.
    pub active_cell_count: usize,
}

// =====================================================================
// REGION MAP
// =====================================================================

/// Grid of `ActivationRegion`s covering the entire activation field.
///
/// Constructed once per field resize; updated each tick by `refresh()`.
///
/// # Memory
/// Each `ActivationRegion` is ~72 bytes.  A 250×250 field with 8×8 regions
/// → 32×32 = 1024 regions → 72 KB.  Fits in L2 cache.
pub struct RegionMap {
    regions:       Vec<ActivationRegion>,
    regions_wide:  usize,
    regions_tall:  usize,
    field_width:   usize,
    /// TODO(V3-DIFFERENTIAL): Used for region resize when field dimensions change.
    #[allow(dead_code)]
    field_height:  usize,
}

impl RegionMap {
    /// Construct a region map for a `width × height` field.
    pub fn new(width: usize, height: usize) -> Self {
        let rw = width.div_ceil(REGION_SIZE);
        let rt = height.div_ceil(REGION_SIZE);
        let mut regions = Vec::with_capacity(rw * rt);

        for ry in 0..rt {
            for rx in 0..rw {
                let x_start = rx * REGION_SIZE;
                let y_start = ry * REGION_SIZE;
                regions.push(ActivationRegion {
                    region_idx: ry * rw + rx,
                    bounds: RegionBounds {
                        x_start,
                        x_end:   (x_start + REGION_SIZE).min(width),
                        y_start,
                        y_end:   (y_start + REGION_SIZE).min(height),
                        field_width: width,
                    },
                    activity:          RegionActivityState::Dormant,
                    mean_probability:  0.0,
                    peak_probability:  0.0,
                    active_cell_count: 0,
                });
            }
        }

        Self { regions, regions_wide: rw, regions_tall: rt, field_width: width, field_height: height }
    }

    /// Scan the activation field and update all region activity states.
    ///
    /// O(N) where N = total cells.  Run once per tick after the solver step.
    ///
    /// TODO(V3-DIFFERENTIAL): Once FieldDeltaMask is integrated, only
    /// re-scan regions that contain at least one changed cell.
    pub fn refresh(&mut self, field: &ActivationField) {
        for region in &mut self.regions {
            let mut sum    = 0.0f32;
            let mut peak   = 0.0f32;
            let mut active = 0usize;
            let count      = region.bounds.cell_count();

            for idx in region.bounds.iter_cells() {
                if idx >= field.cells.len() { break; }
                let p = field.cells[idx].execution_probability;
                sum  += p;
                if p > peak    { peak = p; }
                if p > ACTIVE_THRESHOLD { active += 1; }
            }

            let mean = if count > 0 { sum / count as f32 } else { 0.0 };
            region.mean_probability  = mean;
            region.peak_probability  = peak;
            region.active_cell_count = active;

            region.activity = if peak > HOT_THRESHOLD {
                RegionActivityState::Hot
            } else if active > 0 {
                RegionActivityState::Active
            } else if mean > DORMANT_THRESHOLD {
                RegionActivityState::Warming
            } else {
                RegionActivityState::Dormant
            };
        }
    }

    /// Iterate over all regions.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, ActivationRegion> {
        self.regions.iter()
    }

    /// Iterate over regions that need execution this tick.
    pub fn iter_active(&self) -> impl Iterator<Item = &ActivationRegion> {
        self.regions.iter().filter(|r| r.activity.needs_execution())
    }

    /// Iterate over regions that need streaming consideration.
    pub fn iter_streaming_eligible(&self) -> impl Iterator<Item = &ActivationRegion> {
        self.regions.iter().filter(|r| r.activity.needs_streaming())
    }

    /// Get a region by its flat region index.
    #[inline]
    pub fn get(&self, region_idx: usize) -> Option<&ActivationRegion> {
        self.regions.get(region_idx)
    }

    /// Compute the region index containing field cell `cell_idx`.
    #[inline]
    pub fn region_for_cell(&self, cell_idx: usize) -> usize {
        let x  = cell_idx % self.field_width;
        let y  = cell_idx / self.field_width;
        let rx = x / REGION_SIZE;
        let ry = y / REGION_SIZE;
        ry * self.regions_wide + rx
    }

    /// Total number of regions.
    #[inline]
    pub fn region_count(&self) -> usize { self.regions.len() }

    /// Dimensions of the region grid (wide, tall).
    #[inline]
    pub fn region_grid_dims(&self) -> (usize, usize) {
        (self.regions_wide, self.regions_tall)
    }

    /// Returns true if the cell at `cell_idx` is in a Dormant region.
    ///
    /// Used by `ExecutionBridge::translate_region_filtered()` (Task 10)
    /// to suppress scheduling requests from dormant regions.
    ///
    /// O(1) — computes region index and checks activity state.
    ///
    /// TODO(V3-SPARSE-VALIDATION): Validate that zero non-dormant emission
    /// requests are suppressed (only dormant region cells should be dropped).
    #[inline]
    pub fn cell_is_dormant(&self, cell_idx: usize) -> bool {
        let region_idx = self.region_for_cell(cell_idx);
        self.regions.get(region_idx)
            .map(|r| r.activity == RegionActivityState::Dormant)
            .unwrap_or(true) // out-of-bounds → treat as dormant
    }

    /// Refresh only regions that contain cells in the changed set.
    ///
    /// **V3-SPARSE / Task 6: Region-gated execution preparation.**
    ///
    /// Instead of scanning all N cells, this scans only the regions that
    /// contain changed cells (from the `FieldDeltaMask`).  Unchanged regions
    /// retain their activity state from the previous tick.
    ///
    /// # Correctness Constraint
    /// A region's activity state can only change if at least one of its cells
    /// appears in the delta mask.  If no cell changed, the mean and peak
    /// probabilities are identical to last tick.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Run refresh() and refresh_changed_regions()
    /// in parallel for 1000 ticks.  Assert zero divergence in activity_stats().
    pub fn refresh_changed_regions(
        &mut self,
        field:      &ActivationField,
        delta_mask: &crate::activation::delta::FieldDeltaMask,
    ) {
        // Build set of changed region indices
        let mut changed_regions = Vec::with_capacity(64);
        for idx in delta_mask.iter_changed() {
            if idx >= field.cells.len() { break; }
            let region_idx = self.region_for_cell(idx);
            // Deduplicate — changed_regions is typically small
            if !changed_regions.contains(&region_idx) {
                changed_regions.push(region_idx);
            }
        }

        // Only refresh changed regions
        for &region_idx in &changed_regions {
            if let Some(region) = self.regions.get_mut(region_idx) {
                let mut sum    = 0.0f32;
                let mut peak   = 0.0f32;
                let mut active = 0usize;
                let count      = region.bounds.cell_count();

                for cell_idx in region.bounds.iter_cells() {
                    if cell_idx >= field.cells.len() { break; }
                    let p = field.cells[cell_idx].execution_probability;
                    sum += p;
                    if p > peak { peak = p; }
                    if p > ACTIVE_THRESHOLD { active += 1; }
                }

                let mean = if count > 0 { sum / count as f32 } else { 0.0 };
                region.mean_probability  = mean;
                region.peak_probability  = peak;
                region.active_cell_count = active;

                region.activity = if peak > HOT_THRESHOLD {
                    RegionActivityState::Hot
                } else if active > 0 {
                    RegionActivityState::Active
                } else if mean > DORMANT_THRESHOLD {
                    RegionActivityState::Warming
                } else {
                    RegionActivityState::Dormant
                };
            }
        }
    }

    /// Region activity summary statistics.
    pub fn activity_stats(&self) -> RegionStats {
        let mut s = RegionStats::default();
        s.total = self.regions.len();
        for r in &self.regions {
            match r.activity {
                RegionActivityState::Dormant => s.dormant  += 1,
                RegionActivityState::Warming => s.warming  += 1,
                RegionActivityState::Active  => s.active   += 1,
                RegionActivityState::Hot     => s.hot      += 1,
            }
        }
        s
    }
}

/// Diagnostic summary of region activity distribution.
#[derive(Debug, Clone, Copy, Default)]
pub struct RegionStats {
    pub total:   usize,
    pub dormant: usize,
    pub warming: usize,
    pub active:  usize,
    pub hot:     usize,
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    #[test]
    fn region_map_creation_4x4_field() {
        // 4×4 field, REGION_SIZE=8 → ceil(4/8)=1 region on each axis → 1 region total
        let map = RegionMap::new(4, 4);
        assert_eq!(map.region_count(), 1);
    }

    #[test]
    fn region_map_16x16_field() {
        // 16×16 field, REGION_SIZE=8 → 2×2 = 4 regions
        let map = RegionMap::new(16, 16);
        assert_eq!(map.region_count(), 4);
        let (w, t) = map.region_grid_dims();
        assert_eq!((w, t), (2, 2));
    }

    #[test]
    fn dormant_field_all_dormant_regions() {
        let field = ActivationField::new(16, 16);
        let mut map = RegionMap::new(16, 16);
        map.refresh(&field);
        let stats = map.activity_stats();
        assert_eq!(stats.dormant, 4);
        assert_eq!(stats.active, 0);
    }

    #[test]
    fn hot_cell_makes_region_active() {
        let mut field = ActivationField::new(16, 16);
        field.cells[0].execution_probability = 0.8; // above HOT_THRESHOLD
        let mut map = RegionMap::new(16, 16);
        map.refresh(&field);
        let region = map.get(0).unwrap();
        assert_eq!(region.activity, RegionActivityState::Hot);
    }

    #[test]
    fn region_for_cell_correct() {
        // 16×16 field, 8×8 regions
        // cell 0 → (x=0, y=0) → region (0,0) → idx 0
        // cell 9 → (x=9, y=0) → region (1,0) → idx 1
        // cell 136 → (x=8, y=8) → region (1,1) → idx 3
        let map = RegionMap::new(16, 16);
        assert_eq!(map.region_for_cell(0),   0);
        assert_eq!(map.region_for_cell(9),   1);
        assert_eq!(map.region_for_cell(128+8), 3);
    }

    #[test]
    fn iter_active_filters_dormant() {
        let mut field = ActivationField::new(16, 16);
        // Only activate cells in the second region (x=8..16, y=0..8)
        field.cells[8].execution_probability = 0.6;
        let mut map = RegionMap::new(16, 16);
        map.refresh(&field);
        let active: Vec<_> = map.iter_active().collect();
        assert_eq!(active.len(), 1, "only one region should be active");
        assert_eq!(active[0].region_idx, 1);
    }
}
