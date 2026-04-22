#![no_std]
/// Inspired by rustc tests/ui/numbers-arithmetic/

pub fn add(a: i32, b: i32) -> i32 { a + b }
pub fn sub(a: i32, b: i32) -> i32 { a - b }
pub fn mul(a: i32, b: i32) -> i32 { a * b }
pub fn div(a: i32, b: i32) -> i32 { a / b }
pub fn rem(a: i32, b: i32) -> i32 { a % b }
pub fn neg(a: i32) -> i32 { -a }

pub fn wrapping_add(a: i32, b: i32) -> i32 { a.wrapping_add(b) }
pub fn wrapping_mul(a: i32, b: i32) -> i32 { a.wrapping_mul(b) }

pub fn max_i32() -> i32 { i32::MAX }
pub fn min_i32() -> i32 { i32::MIN }

pub fn abs_val(x: i32) -> i32 {
    if x < 0 { -x } else { x }
}

pub fn clamp(x: i32, lo: i32, hi: i32) -> i32 {
    if x < lo { lo } else if x > hi { hi } else { x }
}

pub fn fibonacci(n: u32) -> u64 {
    if n == 0 { return 0; }
    if n == 1 { return 1; }
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    let mut i: u32 = 2;
    while i <= n {
        let c = a + b;
        a = b;
        b = c;
        i += 1;
    }
    b
}
