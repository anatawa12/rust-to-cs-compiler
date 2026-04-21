// Test input: floating-point arithmetic.
// Uses no std-library functions.
#![no_std]

pub fn add_f32(a: f32, b: f32) -> f32 {
    a + b
}

pub fn sub_f32(a: f32, b: f32) -> f32 {
    a - b
}

pub fn mul_f32(a: f32, b: f32) -> f32 {
    a * b
}

pub fn div_f32(a: f32, b: f32) -> f32 {
    a / b
}

pub fn add_f64(a: f64, b: f64) -> f64 {
    a + b
}

pub fn mul_f64(a: f64, b: f64) -> f64 {
    a * b
}

pub fn f64_to_f32(x: f64) -> f32 {
    x as f32
}

pub fn f32_to_f64(x: f32) -> f64 {
    x as f64
}

pub fn i32_to_f64(x: i32) -> f64 {
    x as f64
}

pub fn f64_to_i32(x: f64) -> i32 {
    x as i32
}

pub fn negate_f32(x: f32) -> f32 {
    -x
}

pub fn is_positive_f64(x: f64) -> bool {
    x > 0.0
}
