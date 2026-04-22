// Adapted from rustc tests/ui/closures/ and tests/ui/fn-ptr/
// Tests function pointers and simple non-capturing closures passed as fn().
#![no_std]

fn apply(f: fn(i32) -> i32, x: i32) -> i32 { f(x) }
fn double(x: i32) -> i32 { x * 2 }
fn square(x: i32) -> i32 { x * x }
fn inc(x: i32) -> i32 { x + 1 }

fn compose(f: fn(i32) -> i32, g: fn(i32) -> i32, x: i32) -> i32 { f(g(x)) }

fn main() {
    assert!(apply(double, 5) == 10);
    assert!(apply(square, 4) == 16);
    assert!(apply(inc, 99) == 100);

    assert!(compose(double, inc, 3) == 8);   // double(inc(3)) = double(4) = 8
    assert!(compose(inc, double, 3) == 7);   // inc(double(3)) = inc(6) = 7
    assert!(compose(square, inc, 3) == 16);  // square(inc(3)) = square(4) = 16
}
