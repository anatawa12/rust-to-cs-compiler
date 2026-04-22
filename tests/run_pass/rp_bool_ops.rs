// Adapted from rustc tests/ui/bool/ and tests/ui/if/
// Tests boolean operations and control flow.
#![no_std]

fn main() {
    assert!(true);
    assert!(!false);
    assert!(true && true);
    assert!(!(true && false));
    assert!(true || false);
    assert!(!(false || false));

    // if/else
    let x = if true { 1 } else { 2 };
    assert!(x == 1);

    let y = if false { 1 } else { 2 };
    assert!(y == 2);

    // Nested
    let z = if 3 > 2 { if 5 > 4 { 10 } else { 20 } } else { 30 };
    assert!(z == 10);
}
