/// ===================================================================
/// mirage-platform/src/debug.rs
/// PURPOSE: Runtime Profiling and Thermal Visualization
///
/// Provides comprehensive debug views into the adaptive runtime:
/// - Chunk thermal states
/// - GPU upload statistics
/// - Frame timings
/// - Residency maps
/// - Streaming diagnostics
/// ===================================================================

/// Runtime profiling statistics
#[derive(Debug, Clone)]
pub struct ProfileStats {
    pub frame: u64,
    pub frame_time_ms: f32,
    pub gpu_upload_bytes: usize,
    pub chunks_hot: usize,
    pub chunks_resident: usize,
    pub chunks_predictive: usize,
    pub chunks_dormant: usize,
    pub avg_thermal_heat: f32,
    pub streaming_queued: usize,
    pub streaming_loaded: usize,
}

/// Thermal state visualization data
#[derive(Debug, Clone)]
pub struct ThermalView {
    pub grid_width: u32,
    pub grid_height: u32,
    /// Thermal heat values (0.0-1.0) for each chunk
    pub heat_map: Vec<f32>,
    /// Chunk states as u8 (0=Dormant, 1=Predictive, 2=Resident, 3=Hot)
    pub state_map: Vec<u8>,
}

impl ThermalView {
    pub fn new(grid_width: u32, grid_height: u32) -> Self {
        let size = (grid_width * grid_height) as usize;
        Self {
            grid_width,
            grid_height,
            heat_map: vec![0.0; size],
            state_map: vec![0; size],
        }
    }

    /// Generate ASCII art thermal display for console
    pub fn render_ascii(&self) -> String {
        let mut output = String::new();
        output.push_str("╔════════════════════ THERMAL MAP ════════════════════╗\n");

        for z in 0..self.grid_height {
            output.push_str("║ ");
            for x in 0..self.grid_width {
                let idx = (z * self.grid_width + x) as usize;
                let state = self.state_map.get(idx).copied().unwrap_or(0);
                let heat = self.heat_map.get(idx).copied().unwrap_or(0.0);

                let ch = match state {
                    0 => '.', // Dormant (cold)
                    1 => '◌', // Predictive (warm)
                    2 => '●', // Resident (hotter)
                    3 => '◆', // Hot (hottest)
                    _ => '?',
                };

                output.push(ch);
            }
            output.push_str(" ║\n");
        }

        output.push_str("╚═══════════════════════════════════════════════════════╝\n");
        output
    }

    /// Export as JSON for external visualization
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"grid": {{"width": {}, "height": {}}}, "heat": {:?}, "states": {:?}}}"#,
            self.grid_width, self.grid_height, self.heat_map, self.state_map
        )
    }
}

/// Debug profiler - collects runtime statistics
pub struct DebugProfiler {
    stats: ProfileStats,
    thermal_view: ThermalView,
    enabled: bool,
}

impl DebugProfiler {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: ProfileStats {
                frame: 0,
                frame_time_ms: 0.0,
                gpu_upload_bytes: 0,
                chunks_hot: 0,
                chunks_resident: 0,
                chunks_predictive: 0,
                chunks_dormant: 0,
                avg_thermal_heat: 0.0,
                streaming_queued: 0,
                streaming_loaded: 0,
            },
            thermal_view: ThermalView::new(25, 25),
            enabled,
        }
    }

    /// Update profiler with frame statistics
    pub fn update_frame(&mut self, stats: ProfileStats) {
        if !self.enabled {
            return;
        }
        self.stats = stats;
    }

    /// Update thermal view from thermal system
    pub fn update_thermal_view(
        &mut self,
        heats: &[f32],
        states: &[u8],
    ) {
        if !self.enabled || heats.len() != self.thermal_view.heat_map.len() {
            return;
        }
        self.thermal_view.heat_map.copy_from_slice(heats);
        self.thermal_view.state_map.copy_from_slice(states);
    }

    /// Print summary to console
    pub fn print_summary(&self) {
        if !self.enabled {
            return;
        }

        println!("\n╔════════════════════ PROFILER STATS ════════════════════╗");
        println!("║ Frame: {:<47} ║", self.stats.frame);
        println!(
            "║ Frame Time: {:.2} ms{:<37} ║",
            self.stats.frame_time_ms, ""
        );
        println!(
            "║ GPU Upload: {} bytes{:<34} ║",
            self.stats.gpu_upload_bytes, ""
        );
        println!(
            "║ Chunks: HOT={:<4} RES={:<4} PRED={:<4} DORM={:<7} ║",
            self.stats.chunks_hot,
            self.stats.chunks_resident,
            self.stats.chunks_predictive,
            self.stats.chunks_dormant
        );
        println!(
            "║ Avg Heat: {:.3}{:<38} ║",
            self.stats.avg_thermal_heat, ""
        );
        println!(
            "║ Streaming: Q={:<4} L={:<4} {:<26} ║",
            self.stats.streaming_queued, self.stats.streaming_loaded, ""
        );
        println!("╚═════════════════════════════════════════════════════════╝\n");
    }

    /// Get thermal ASCII visualization
    pub fn get_thermal_display(&self) -> String {
        self.thermal_view.render_ascii()
    }

    /// Export thermal data as JSON
    pub fn export_thermal_json(&self) -> String {
        self.thermal_view.to_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_view_creation() {
        let view = ThermalView::new(25, 25);
        assert_eq!(view.heat_map.len(), 625);
        assert_eq!(view.state_map.len(), 625);
    }

    #[test]
    fn profiler_enabled() {
        let profiler = DebugProfiler::new(true);
        assert!(profiler.enabled);
    }

    #[test]
    fn thermal_ascii_render() {
        let view = ThermalView::new(5, 5);
        let output = view.render_ascii();
        assert!(output.contains("╔"));
        assert!(output.contains("║"));
        assert!(output.contains("╚"));
    }
}
