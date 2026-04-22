#![no_std]
// Inspired by rustc tests/ui/numbers-arithmetic/ and tests/ui/cast/
// Tests integer casting, overflow detection, and mixed-size arithmetic.

pub fn i32_to_i64(x: i32) -> i64 { x as i64 }
pub fn i64_to_i32(x: i64) -> i32 { x as i32 }
pub fn u8_to_i32(x: u8) -> i32 { x as i32 }
pub fn i32_to_u8(x: i32) -> u8 { x as u8 }
pub fn i32_to_f64(x: i32) -> f64 { x as f64 }
pub fn f64_to_i32(x: f64) -> i32 { x as i32 }
pub fn u32_to_i32_wrap(x: u32) -> i32 { x as i32 }
pub fn i32_to_u32_wrap(x: i32) -> u32 { x as u32 }

// Widening / narrowing chains
pub fn chain_cast(x: u8) -> u32 {
    let a = x as u16;
    let b = a as u32;
    b
}

pub fn truncate_cycle(x: i32) -> i32 {
    let b = x as u8;
    b as i32
}

// Bit counting
pub fn popcount_u32(x: u32) -> u32 {
    let mut v = x;
    let mut count: u32 = 0;
    while v != 0 {
        count += v & 1;
        v >>= 1;
    }
    count
}

pub fn leading_zeros(mut x: u32) -> u32 {
    if x == 0 { return 32; }
    let mut count: u32 = 0;
    while (x & 0x8000_0000) == 0 {
        x <<= 1;
        count += 1;
    }
    count
}
