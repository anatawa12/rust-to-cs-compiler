// Test input: borrowing local and param variables.
// Uses no std-library functions.
#![no_std]

/// Sum through immutable refs to param values.
pub fn sum_refs(a: i32, b: i32, c: i32) -> i32 {
    let ra = &a;
    let rb = &b;
    let rc = &c;
    *ra + *rb + *rc
}

/// Max of two via refs.
pub fn max_by_ref(a: i32, b: i32) -> i32 {
    let ra = &a;
    let rb = &b;
    if *ra > *rb { *ra } else { *rb }
}

/// Mutate via ref inside function.
pub fn double_local(x: i32) -> i32 {
    let mut v = x;
    let r = &mut v;
    *r *= 2;
    v
}

/// Increment via ref.
pub fn increment_local(x: i32) -> i32 {
    let mut v = x;
    {
        let r = &mut v;
        *r += 1;
    }
    v
}
