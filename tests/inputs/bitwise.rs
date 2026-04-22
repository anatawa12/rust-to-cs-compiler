// Test input: bitwise operations on integers.
// Uses no std-library functions.
#![no_std]

pub fn bit_and(a: u32, b: u32) -> u32 {
    a & b
}

pub fn bit_or(a: u32, b: u32) -> u32 {
    a | b
}

pub fn bit_xor(a: u32, b: u32) -> u32 {
    a ^ b
}

pub fn bit_not(a: u32) -> u32 {
    !a
}

pub fn shl(a: u32, b: u32) -> u32 {
    a << b
}

pub fn shr(a: u32, b: u32) -> u32 {
    a >> b
}

pub fn logical_not(a: bool) -> bool {
    !a
}

pub fn count_set_bits(mut x: u32) -> u32 {
    let mut count: u32 = 0;
    while x != 0 {
        count += x & 1;
        x >>= 1;
    }
    count
}

pub fn is_power_of_two(x: u32) -> bool {
    x != 0 && (x & (x - 1)) == 0
}
