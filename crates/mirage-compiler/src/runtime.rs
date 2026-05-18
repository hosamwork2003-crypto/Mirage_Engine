//! Runtime kernel specialization helpers

pub trait Specializer {
    fn choose_kernel(&self, thermal_score: f32) -> &'static str;
}

pub struct RuntimeSpecializer {}
impl RuntimeSpecializer {
    pub fn new() -> Self { Self {} }
}

impl Specializer for RuntimeSpecializer {
    fn choose_kernel(&self, thermal_score: f32) -> &'static str {
        if thermal_score > 0.7 { "simd" } else { "scalar" }
    }
}
