// Test input: explicit casts (as) between numeric types.
// Uses no std-library functions.
#![no_std]

pub fn i32_to_u8(x: i32) -> u8 {
    x as u8
}

pub fn u8_to_i32(x: u8) -> i32 {
    x as i32
}

pub fn i32_to_u32(x: i32) -> u32 {
    x as u32
}

pub fn u32_to_i32(x: u32) -> i32 {
    x as i32
}

pub fn u8_to_u32(x: u8) -> u32 {
    x as u32
}

pub fn i64_to_i32(x: i64) -> i32 {
    x as i32
}

pub fn i32_to_i64(x: i32) -> i64 {
    x as i64
}

pub fn u64_to_usize(x: u64) -> usize {
    x as usize
}

pub fn saturating_cast_u8(x: i32) -> u8 {
    // Wrapping cast just like Rust does.
    x as u8
}
