// ===================================================================
// mirage-query/src/columnar.rs
// PURPOSE: ColumnarScan — Structure-of-Arrays (SoA) Execution Backend
//
// SoA layout keeps each physical attribute in its own contiguous Vec,
// enabling the compiler to emit wider SIMD loads and avoiding the
// AoS stride penalty of ActivationCell's 20-byte struct.
//
// INVARIANT: All columns have equal length at all times.
// ===================================================================

/// Structure-of-Arrays store for activation field attributes.
///
/// Each column maps 1-to-1 with a cell index identical to the
/// AoS `ActivationField::cells` index. The two representations
/// hold the same logical data; `ColumnarScan` is the query-execution
/// projection, not the authoritative store.
pub struct ColumnarScan {
    /// cell heat values
    pub heat: Vec<f32>,
    /// cell pressure values
    pub pressure: Vec<f32>,
    /// cell entropy values
    pub entropy: Vec<f32>,
    /// cell activation values
    pub activation: Vec<f32>,
    /// cell execution probability values
    pub execution_probability: Vec<f32>,
    /// selection bitset: cell i is selected iff selected[i] == true
    pub selected: Vec<bool>,
    /// total number of cells
    pub len: usize,
}

impl ColumnarScan {
    /// Create an empty scan with pre-allocated capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            heat:                  vec![0.0; capacity],
            pressure:              vec![0.0; capacity],
            entropy:               vec![0.0; capacity],
            activation:            vec![0.0; capacity],
            execution_probability: vec![0.0; capacity],
            selected:              vec![true; capacity],
            len:                   capacity,
        }
    }

    /// Resize all columns in-place without heap re-allocation when
    /// capacity is already sufficient.
    pub fn resize(&mut self, n: usize) {
        self.heat.resize(n, 0.0);
        self.pressure.resize(n, 0.0);
        self.entropy.resize(n, 0.0);
        self.activation.resize(n, 0.0);
        self.execution_probability.resize(n, 0.0);
        self.selected.resize(n, true);
        self.len = n;
    }

    /// Load from a flat AoS slice of (heat, pressure, entropy,
    /// activation, execution_probability) tuples by scatter-copying
    /// into each SoA column. The `cells` iterator yields
    /// `(heat, pressure, entropy, activation, exec_prob)` tuples.
    pub fn load_from_cells<I>(&mut self, cells: I)
    where
        I: Iterator<Item = (f32, f32, f32, f32, f32)>,
    {
        let mut n = 0;
        for (heat, pressure, entropy, activation, exec_prob) in cells {
            if n >= self.len {
                // Grow dynamically
                self.heat.push(heat);
                self.pressure.push(pressure);
                self.entropy.push(entropy);
                self.activation.push(activation);
                self.execution_probability.push(exec_prob);
                self.selected.push(true);
                n += 1;
            } else {
                self.heat[n] = heat;
                self.pressure[n] = pressure;
                self.entropy[n] = entropy;
                self.activation[n] = activation;
                self.execution_probability[n] = exec_prob;
                self.selected[n] = true;
                n += 1;
            }
        }
        self.len = n;
    }

    /// Reset all selection bits to `true` (select all cells).
    #[inline]
    pub fn select_all(&mut self) {
        self.selected[..self.len].fill(true);
    }

    /// Apply a predicate over the activation column and narrow the
    /// selection set. Cells that fail the predicate are deselected.
    ///
    /// This is a columnar AND-filter: it only deselects, never re-selects.
    #[inline]
    pub fn filter_activation(&mut self, threshold: f32) {
        for i in 0..self.len {
            if self.selected[i] && self.activation[i] <= threshold {
                self.selected[i] = false;
            }
        }
    }

    /// Apply a predicate over the execution_probability column.
    #[inline]
    pub fn filter_exec_prob(&mut self, threshold: f32) {
        for i in 0..self.len {
            if self.selected[i] && self.execution_probability[i] <= threshold {
                self.selected[i] = false;
            }
        }
    }

    /// Apply a generic per-cell predicate over all five columns.
    ///
    /// The predicate receives `(heat, pressure, entropy, activation,
    /// exec_prob)` and returns `true` to keep the cell selected.
    pub fn filter_generic<F>(&mut self, predicate: F)
    where
        F: Fn(f32, f32, f32, f32, f32) -> bool,
    {
        for i in 0..self.len {
            if self.selected[i]
                && !predicate(
                    self.heat[i],
                    self.pressure[i],
                    self.entropy[i],
                    self.activation[i],
                    self.execution_probability[i],
                )
            {
                self.selected[i] = false;
            }
        }
    }

    /// Collect indices of all currently selected cells.
    pub fn collect_selected(&self) -> Vec<usize> {
        (0..self.len)
            .filter(|&i| self.selected[i])
            .collect()
    }

    /// Count selected cells without allocating.
    #[inline]
    pub fn count_selected(&self) -> usize {
        self.selected[..self.len].iter().filter(|&&s| s).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scan(n: usize, activation: f32) -> ColumnarScan {
        let mut scan = ColumnarScan::new(n);
        scan.activation.fill(activation);
        scan
    }

    #[test]
    fn new_selects_all() {
        let scan = ColumnarScan::new(8);
        assert_eq!(scan.count_selected(), 8);
    }

    #[test]
    fn filter_activation_narrows_selection() {
        let mut scan = make_scan(4, 0.1);
        scan.activation[2] = 0.9;
        scan.filter_activation(0.5);
        // Only cell 2 should remain selected
        let sel = scan.collect_selected();
        assert_eq!(sel, vec![2]);
    }

    #[test]
    fn load_from_cells_fills_columns() {
        let mut scan = ColumnarScan::new(3);
        let data = vec![
            (1.0_f32, 0.5, 0.2, 0.8, 0.9),
            (0.5, 0.3, 0.4, 0.6, 0.7),
            (0.2, 0.1, 0.6, 0.4, 0.5),
        ];
        scan.load_from_cells(data.into_iter());
        assert!((scan.heat[0] - 1.0).abs() < 1e-6);
        assert!((scan.activation[1] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn filter_generic_uses_compound_predicate() {
        let mut scan = ColumnarScan::new(4);
        scan.heat = vec![0.8, 0.1, 0.9, 0.05];
        scan.activation = vec![0.7, 0.3, 0.8, 0.1];
        // Select cells where heat > 0.5 AND activation > 0.5
        scan.filter_generic(|h, _p, _e, a, _x| h > 0.5 && a > 0.5);
        let sel = scan.collect_selected();
        assert_eq!(sel, vec![0, 2]);
    }
}
