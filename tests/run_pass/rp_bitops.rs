// Adapted from rustc tests/ui/bitops/
// Tests bitwise operations and shift operators.
#![no_std]

fn main() {
    // Bitwise AND / OR / XOR
    assert!(0b1010_u32 & 0b1100 == 0b1000);
    assert!(0b1010_u32 | 0b1100 == 0b1110);
    assert!(0b1010_u32 ^ 0b1100 == 0b0110);
    assert!(u32::MAX == 4294967295);

    // Shifts
    assert!(1_u32 << 3 == 8);
    assert!(16_u32 >> 2 == 4);
    assert!((-8_i32) >> 1 == -4);    // arithmetic right shift

    // Mixed
    assert!((0xFF_u32 & 0x0F) == 0x0F);
    assert!((1_u32 << 8) == 256);

    // Count set bits in a u32 manually
    let x: u32 = 0b10110100;
    let mut ones = 0_u32;
    let mut i = 0_u32;
    while i < 8 {
        if (x >> i) & 1 == 1 { ones += 1; }
        i += 1;
    }
    assert!(ones == 4);
}
