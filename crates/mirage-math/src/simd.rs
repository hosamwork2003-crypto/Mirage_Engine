/// ===================================================================
/// mirage-math/src/simd.rs
/// PURPOSE: Stable SIMD Abstraction Layer
///
/// Provides unified SIMD operations that compile on stable Rust
/// while supporting optional nightly SIMD acceleration.
/// ===================================================================

use std::ops::{Add, Sub, Mul, AddAssign, SubAssign, MulAssign};

/// Generic SIMD vector trait for 4-element vectors
pub trait SimdVec4: Sized + Clone + Copy {
    fn splat(val: f32) -> Self;
    fn from_array(arr: [f32; 4]) -> Self;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn mul(self, other: Self) -> Self;
    fn mul_scalar(self, scalar: f32) -> Self;
    fn div(self, other: Self) -> Self;
    fn dot(self, other: Self) -> f32;
    fn reduce_sum(self) -> f32;
    fn sqrt(self) -> Self;
    fn normalize(self) -> Self;
}

/// Generic SIMD vector trait for 16-element vectors
pub trait SimdVec16: Sized + Clone + Copy {
    fn splat(val: f32) -> Self;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn mul(self, other: Self) -> Self;
    fn mul_scalar(self, scalar: f32) -> Self;
    fn reduce_sum(self) -> f32;
}

// ===================================================================
// SCALAR BACKEND (always available, no nightly required)
// ===================================================================

#[derive(Clone, Copy, Debug, Default)]
pub struct F32x4(pub [f32; 4]);

impl SimdVec4 for F32x4 {
    #[inline(always)]
    fn splat(val: f32) -> Self {
        F32x4([val; 4])
    }

    #[inline(always)]
    fn from_array(arr: [f32; 4]) -> Self {
        F32x4(arr)
    }

    #[inline(always)]
    fn add(self, other: Self) -> Self {
        F32x4([
            self.0[0] + other.0[0],
            self.0[1] + other.0[1],
            self.0[2] + other.0[2],
            self.0[3] + other.0[3],
        ])
    }

    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        F32x4([
            self.0[0] - other.0[0],
            self.0[1] - other.0[1],
            self.0[2] - other.0[2],
            self.0[3] - other.0[3],
        ])
    }

    #[inline(always)]
    fn mul(self, other: Self) -> Self {
        F32x4([
            self.0[0] * other.0[0],
            self.0[1] * other.0[1],
            self.0[2] * other.0[2],
            self.0[3] * other.0[3],
        ])
    }

    #[inline(always)]
    fn mul_scalar(self, scalar: f32) -> Self {
        F32x4([
            self.0[0] * scalar,
            self.0[1] * scalar,
            self.0[2] * scalar,
            self.0[3] * scalar,
        ])
    }

    #[inline(always)]
    fn div(self, other: Self) -> Self {
        F32x4([
            self.0[0] / other.0[0],
            self.0[1] / other.0[1],
            self.0[2] / other.0[2],
            self.0[3] / other.0[3],
        ])
    }

    #[inline(always)]
    fn dot(self, other: Self) -> f32 {
        self.0[0] * other.0[0]
            + self.0[1] * other.0[1]
            + self.0[2] * other.0[2]
            + self.0[3] * other.0[3]
    }

    #[inline(always)]
    fn reduce_sum(self) -> f32 {
        self.0[0] + self.0[1] + self.0[2] + self.0[3]
    }

    #[inline(always)]
    fn sqrt(self) -> Self {
        F32x4([
            self.0[0].sqrt(),
            self.0[1].sqrt(),
            self.0[2].sqrt(),
            self.0[3].sqrt(),
        ])
    }

    #[inline(always)]
    fn normalize(self) -> Self {
        let len_sq = self.dot(self);
        let inv_len = 1.0 / len_sq.sqrt();
        self.mul_scalar(inv_len)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct F32x16(pub [[f32; 4]; 4]);

impl SimdVec16 for F32x16 {
    #[inline(always)]
    fn splat(val: f32) -> Self {
        F32x16([[val; 4]; 4])
    }

    #[inline(always)]
    fn add(self, other: Self) -> Self {
        let mut result = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                result[i][j] = self.0[i][j] + other.0[i][j];
            }
        }
        F32x16(result)
    }

    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        let mut result = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                result[i][j] = self.0[i][j] - other.0[i][j];
            }
        }
        F32x16(result)
    }

    #[inline(always)]
    fn mul(self, other: Self) -> Self {
        let mut result = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                result[i][j] = self.0[i][j] * other.0[i][j];
            }
        }
        F32x16(result)
    }

    #[inline(always)]
    fn mul_scalar(self, scalar: f32) -> Self {
        let mut result = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                result[i][j] = self.0[i][j] * scalar;
            }
        }
        F32x16(result)
    }

    #[inline(always)]
    fn reduce_sum(self) -> f32 {
        let mut sum = 0.0;
        for i in 0..4 {
            for j in 0..4 {
                sum += self.0[i][j];
            }
        }
        sum
    }
}

// ===================================================================
// OPERATOR OVERLOADS FOR ERGONOMICS
// ===================================================================

impl Add for F32x4 {
    type Output = Self;
    #[inline(always)]
    fn add(self, other: Self) -> Self {
        SimdVec4::add(self, other)
    }
}

impl Sub for F32x4 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        SimdVec4::sub(self, other)
    }
}

impl Mul for F32x4 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, other: Self) -> Self {
        SimdVec4::mul(self, other)
    }
}

impl Mul<f32> for F32x4 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, scalar: f32) -> Self {
        SimdVec4::mul_scalar(self, scalar)
    }
}

impl AddAssign for F32x4 {
    #[inline(always)]
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl SubAssign for F32x4 {
    #[inline(always)]
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl MulAssign for F32x4 {
    #[inline(always)]
    fn mul_assign(&mut self, other: Self) {
        *self = *self * other;
    }
}

impl Add for F32x16 {
    type Output = Self;
    #[inline(always)]
    fn add(self, other: Self) -> Self {
        SimdVec16::add(self, other)
    }
}

impl Sub for F32x16 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        SimdVec16::sub(self, other)
    }
}

impl Mul for F32x16 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, other: Self) -> Self {
        SimdVec16::mul(self, other)
    }
}

impl Mul<f32> for F32x16 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, scalar: f32) -> Self {
        SimdVec16::mul_scalar(self, scalar)
    }
}

impl AddAssign for F32x16 {
    #[inline(always)]
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl SubAssign for F32x16 {
    #[inline(always)]
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl MulAssign for F32x16 {
    #[inline(always)]
    fn mul_assign(&mut self, other: Self) {
        *self = *self * other;
    }
}
