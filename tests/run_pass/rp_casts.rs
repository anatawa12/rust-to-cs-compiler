// Adapted from rustc tests/ui/cast/ and tests/ui/type-coercion/
// Tests integer casting behaviour.
#![no_std]

fn main() {
    // Widening casts
    assert!(42_i32 as i64 == 42_i64);
    assert!(200_u8 as i32 == 200);
    assert!((-1_i8) as i32 == -1);   // sign-extension

    // Truncating casts
    assert!(256_i32 as u8 == 0);
    assert!(255_i32 as u8 == 255);
    assert!((-1_i32) as u32 == u32::MAX);

    // Float casts
    assert!(3_i32 as f64 == 3.0_f64);
    assert!(3.7_f64 as i32 == 3);    // truncates toward zero
    assert!((-3.7_f64) as i32 == -3);

    // usize / isize
    assert!(42_usize as u64 == 42_u64);
}
