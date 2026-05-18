/// ===================================================================
/// mirage-platform/src/lib.rs
/// PURPOSE: Debug and Profiling Layer - Runtime Introspection
///
/// This module provides comprehensive visibility into the runtime:
/// - Thermal visualization
/// - Residency tracking
/// - GPU statistics
/// - Streaming diagnostics
///
/// DESIGN:
/// All debugging is opt-in (zero cost when disabled)
/// Can output to console, JSON, or UI integration
/// ===================================================================

pub mod debug;
pub mod hardware;
pub mod profiler;

pub use debug::{DebugProfiler, ProfileStats, ThermalView};

#[cfg(test)]
mod tests {
    #[test]
    fn platform_module_loads() {
        // Compilation success is the test
    }
}

