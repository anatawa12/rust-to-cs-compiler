#![no_std]
// Inspired by rustc tests/ui/closures/
// Tests simple closures (non-capturing) passed by value and as generic args.

pub fn apply_twice(f: fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}

pub fn double(x: i32) -> i32 { x * 2 }
pub fn inc(x: i32) -> i32 { x + 1 }
pub fn square(x: i32) -> i32 { x * x }
pub fn negate(x: i32) -> i32 { -x }

pub fn apply_twice_double(x: i32) -> i32 { apply_twice(double, x) }
pub fn apply_twice_inc(x: i32) -> i32    { apply_twice(inc, x) }
pub fn apply_twice_square(x: i32) -> i32 { apply_twice(square, x) }

// Higher-order: compose two functions
pub fn compose(f: fn(i32) -> i32, g: fn(i32) -> i32, x: i32) -> i32 {
    f(g(x))
}

pub fn double_then_inc(x: i32) -> i32 { compose(inc, double, x) }
pub fn inc_then_double(x: i32) -> i32 { compose(double, inc, x) }
pub fn square_then_negate(x: i32) -> i32 { compose(negate, square, x) }
