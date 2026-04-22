#![no_std]

/// Simple non-capturing closures.
///
/// After MIR optimisation, each closure becomes a static method whose
/// name is derived from the containing function.

pub fn apply_add_one(x: i32) -> i32 {
    let add_one = |v: i32| v + 1;
    add_one(x)
}

pub fn apply_mul_two(x: i32) -> i32 {
    let mul_two = |v: i32| v * 2;
    mul_two(x)
}

pub fn compose_add_then_mul(x: i32) -> i32 {
    let add = |v: i32| v + 3;
    let mul = |v: i32| v * 4;
    mul(add(x))
}
