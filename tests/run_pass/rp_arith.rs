// Adapted from rustc tests/ui/numbers-arithmetic/add.rs and similar.
// Tests basic integer arithmetic operations.
#![no_std]

fn main() {
    assert!(1_i32 + 1 == 2);
    assert!(10 - 3 == 7);
    assert!(3 * 4 == 12);
    assert!(10 / 3 == 3);
    assert!(10 % 3 == 1);
    assert!(-5_i32 + 5 == 0);
    assert!(i32::MAX == 2147483647);
    assert!(i32::MIN == -2147483648);
    assert!(u32::MAX == 4294967295);
    assert!(u8::MAX == 255);
}
