// Adapted from rustc tests/ui/const-eval/ and tests/ui/consts/
// Tests compile-time constants and const expressions.
#![no_std]

const ZERO: i32 = 0;
const ONE: i32 = 1;
const MAX_U8: u8 = 255;
const BITS: u32 = 8;
const MASK: u32 = (1 << BITS) - 1;

const fn square(x: i32) -> i32 { x * x }
const fn cube(x: i32) -> i32 { x * x * x }

const FOUR_SQUARED: i32 = square(4);
const TWO_CUBED: i32 = cube(2);

fn main() {
    assert!(ZERO == 0);
    assert!(ONE == 1);
    assert!(MAX_U8 == 255);
    assert!(MASK == 255);

    assert!(FOUR_SQUARED == 16);
    assert!(TWO_CUBED == 8);

    assert!(square(5) == 25);
    assert!(cube(3) == 27);
}
