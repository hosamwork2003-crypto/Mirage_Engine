/// SIMD-friendly SoA transform helpers (stable fallback path)

pub fn soa_translate_inplace(positions: &mut [[f32;4]], dx: f32, dy: f32, dz: f32) {
    for p in positions.iter_mut() {
        p[0] += dx;
        p[1] += dy;
        p[2] += dz;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translate_works() {
        let mut arr = vec![[0.0f32,0.0,0.0,0.0]; 4];
        soa_translate_inplace(&mut arr, 1.0, 2.0, 3.0);
        assert_eq!(arr[0][0], 1.0);
    }
}
