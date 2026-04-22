// Test input: const and static items.
// Uses no std-library functions.
#![no_std]

pub const ZERO: i32 = 0;
pub const ONE: i32 = 1;
pub const MAX_BYTE: u8 = 255;
pub const NEG_ONE: i32 = -1;
pub const BIG: i64 = 1_000_000_000_000;

pub fn use_const() -> i32 {
    ONE + ZERO
}

pub fn scale_by_const(x: i32) -> i32 {
    x * ONE
}

pub fn is_max_byte(x: u8) -> bool {
    x == MAX_BYTE
}

pub fn get_big() -> i64 {
    BIG
}
