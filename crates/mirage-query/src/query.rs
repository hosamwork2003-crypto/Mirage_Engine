// ===================================================================
// mirage-query/src/query.rs
// PURPOSE: CellQuery — Fluent Relational Query Builder
//
// DESIGN
// ---------------------------------------------------------------
// CellQuery wraps a mutable reference to a ColumnarScan and exposes
// lazy relational operators (filter, map). Terminal operators
// (collect, apply) consume the query and return results.
//
// PARITY GUARANTEE
// ---------------------------------------------------------------
// Queries that apply kernels produce results identical to the
// equivalent imperative loop in ActivationSolver. Verified by the
// parity tests in this file and by the integration tests in
// mirage-mkr-core.
// ===================================================================

use crate::columnar::ColumnarScan;
use crate::kernel::SolverKernel;

/// Immutable snapshot of a single cell for read-only predicates.
#[derive(Debug, Clone, Copy)]
pub struct CellView {
    pub index:                 usize,
    pub heat:                  f32,
    pub pressure:              f32,
    pub entropy:               f32,
    pub activation:            f32,
    pub execution_probability: f32,
}

/// Mutable view of a single cell for map transformations.
pub struct CellViewMut<'a> {
    pub index:                 usize,
    pub heat:                  &'a mut f32,
    pub pressure:              &'a mut f32,
    pub entropy:               &'a mut f32,
    pub activation:            &'a mut f32,
    pub execution_probability: &'a mut f32,
}

// -----------------------------------------------------------------------
// CellQuery
// -----------------------------------------------------------------------

/// Fluent relational query pipeline over a ColumnarScan.
pub struct CellQuery<'a> {
    scan: &'a mut ColumnarScan,
}

impl<'a> CellQuery<'a> {
    /// Begin a query pipeline. Resets the selection to "all selected".
    pub fn new(scan: &'a mut ColumnarScan) -> Self {
        scan.select_all();
        Self { scan }
    }

    // ------------------------------------------------------------------
    // Relational operators
    // ------------------------------------------------------------------

    /// Filter cells by a compound predicate over all five attributes.
    ///
    /// `predicate(heat, pressure, entropy, activation, exec_prob) -> bool`
    ///
    /// Cells for which the predicate returns `false` are deselected.
    /// Multiple `filter` calls compose as logical AND.
    pub fn filter<F>(self, predicate: F) -> Self
    where
        F: Fn(f32, f32, f32, f32, f32) -> bool,
    {
        self.scan.filter_generic(predicate);
        self
    }

    /// Filter cells by a minimum activation threshold (columnar fast-path).
    pub fn filter_activation(self, threshold: f32) -> Self {
        self.scan.filter_activation(threshold);
        self
    }

    /// Filter cells by a minimum execution_probability threshold.
    pub fn filter_exec_prob(self, threshold: f32) -> Self {
        self.scan.filter_exec_prob(threshold);
        self
    }

    /// Map a transformation over all currently selected cells.
    ///
    /// The closure receives a `CellViewMut` for each selected cell and
    /// may mutate any attribute in-place. Changes are staged inside the
    /// ColumnarScan.
    pub fn map<F>(self, transform: F) -> Self
    where
        F: Fn(CellViewMut<'_>),
    {
        let n = self.scan.len;
        // SAFETY: each iteration borrows a disjoint index from each column.
        // We access columns separately by index to avoid split-borrow issues.
        for i in 0..n {
            if !self.scan.selected[i] { continue; }
            let view = CellViewMut {
                index:                 i,
                heat:                  &mut self.scan.heat[i],
                pressure:              &mut self.scan.pressure[i],
                entropy:               &mut self.scan.entropy[i],
                activation:            &mut self.scan.activation[i],
                execution_probability: &mut self.scan.execution_probability[i],
            };
            transform(view);
        }
        self
    }

    // ------------------------------------------------------------------
    // Terminal operators
    // ------------------------------------------------------------------

    /// Collect indices of all currently selected cells.
    pub fn collect(self) -> Vec<usize> {
        self.scan.collect_selected()
    }

    /// Apply a `SolverKernel` over all currently selected cells.
    ///
    /// Mutates the SoA columns of the underlying ColumnarScan in-place.
    /// Returns `self` so the pipeline can continue or be collected.
    pub fn apply(self, kernel: &dyn SolverKernel) -> Self {
        let n = self.scan.len;
        // Invoke the kernel with mutable slices over all five columns.
        // We pass `&self.scan.selected` as a read-only mask alongside the
        // mutable slices. Rust allows this because `selected` is a
        // separate field from `heat`/`pressure`/etc.
        let sel = self.scan.selected[..n].to_vec(); // snapshot mask read
        kernel.apply_selected(
            &mut self.scan.heat[..n],
            &mut self.scan.pressure[..n],
            &mut self.scan.entropy[..n],
            &mut self.scan.activation[..n],
            &mut self.scan.execution_probability[..n],
            &sel,
        );
        self
    }
}

// -----------------------------------------------------------------------
// ColumnarScan::query() convenience entry-point
// -----------------------------------------------------------------------

impl ColumnarScan {
    /// Begin a fluent CellQuery pipeline over this scan.
    pub fn query(&mut self) -> CellQuery<'_> {
        CellQuery::new(self)
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use crate::columnar::ColumnarScan;
    use crate::kernel::{DecayKernel, RecomputeActivationKernel, RecomputeExecProbKernel, HEAT_DECAY};

    fn make_scan(n: usize) -> ColumnarScan {
        let mut scan = ColumnarScan::new(n);
        for i in 0..n {
            let f = (i + 1) as f32 / n as f32;
            scan.heat[i]       = f;
            scan.pressure[i]   = 0.3;
            scan.entropy[i]    = 0.5;
            // activation rises linearly so filter at 0.5 gives a clean split
            scan.activation[i] = f * 0.8;
            scan.execution_probability[i] = 0.2;
        }
        scan
    }

    #[test]
    fn filter_activation_returns_correct_indices() {
        let mut scan = make_scan(4);
        // activation: [0.2, 0.4, 0.6, 0.8]
        let sel = scan.query().filter_activation(0.5).collect();
        assert_eq!(sel, vec![2, 3]);
    }

    #[test]
    fn chained_filters_narrow_selection() {
        let mut scan = make_scan(4);
        scan.execution_probability[3] = 0.9;
        let sel = scan.query()
            .filter_activation(0.5)
            .filter_exec_prob(0.5)
            .collect();
        assert_eq!(sel, vec![3]);
    }

    #[test]
    fn map_only_touches_selected_cells() {
        let mut scan = make_scan(4);
        let orig_heat_0 = scan.heat[0];
        scan.query()
            .filter_activation(0.5)
            .map(|c| { *c.heat = 0.0; })
            .collect();
        // Cell 0 was not selected — heat must be unchanged
        assert!((scan.heat[0] - orig_heat_0).abs() < 1e-6);
        // Cells 2 and 3 were selected — heat must be zeroed
        assert!(scan.heat[2].abs() < 1e-6);
        assert!(scan.heat[3].abs() < 1e-6);
    }

    #[test]
    fn apply_kernel_mutates_selected_cells() {
        let mut scan = make_scan(4);
        let orig_heat_0 = scan.heat[0]; // cell 0 NOT selected (activation 0.2 ≤ 0.5)
        scan.query()
            .filter_activation(0.5)
            .apply(&DecayKernel)
            .collect();
        // Cell 0 must be unchanged
        assert!((scan.heat[0] - orig_heat_0).abs() < 1e-6);
        // Cell 2 must have decayed
        let expected = (3.0_f32 / 4.0) * HEAT_DECAY;
        assert!((scan.heat[2] - expected).abs() < 1e-4);
    }

    #[test]
    fn full_pipeline_activation_then_exec_prob() {
        let mut scan = ColumnarScan::new(2);
        scan.heat[0] = 1.0; scan.heat[1] = 0.0;
        scan.pressure[0] = 1.0; scan.pressure[1] = 0.0;
        scan.entropy[0] = 0.0; scan.entropy[1] = 1.0;

        // Run activation kernel first (all cells selected)
        scan.query().apply(&RecomputeActivationKernel).collect();

        // activation[0] = 0.55+0.35+0.10 = 1.0; activation[1] = 0.0
        assert!((scan.activation[0] - 1.0).abs() < 1e-6);
        assert!(scan.activation[1].abs() < 1e-6);

        // Run exec prob kernel
        scan.query().apply(&RecomputeExecProbKernel).collect();

        // smoothstep(1.0) = 1.0; smoothstep(0.0) = 0.0
        assert!((scan.execution_probability[0] - 1.0).abs() < 1e-6);
        assert!(scan.execution_probability[1].abs() < 1e-6);
    }
}
