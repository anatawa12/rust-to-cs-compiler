// Test input: reference operations.
// Uses no std-library functions.
#![no_std]

/// Read through a shared reference.
pub fn read_ref(x: &i32) -> i32 {
    *x
}

/// Add two values passed by reference.
pub fn add_by_ref(a: &i32, b: &i32) -> i32 {
    *a + *b
}

/// Write through a mutable reference.
pub fn write_ref(x: &mut i32, val: i32) {
    *x = val;
}

/// Increment through a mutable reference.
pub fn increment(x: &mut i32) {
    *x += 1;
}

/// Double the value in place.
pub fn double_in_place(x: &mut i32) {
    *x *= 2;
}

/// Swap two values via mutable references.
pub fn swap(a: &mut i32, b: &mut i32) {
    let tmp = *a;
    *a = *b;
    *b = tmp;
}

/// Compute sum of two referenced values via intermediate ref.
pub fn sum_via_ref(a: &i32, b: &i32) -> i32 {
    let av = *a;
    let bv = *b;
    av + bv
}
