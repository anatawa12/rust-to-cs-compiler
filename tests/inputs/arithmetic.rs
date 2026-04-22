// Test input: basic arithmetic and control flow.
// Uses no std-library functions — all logic is pure computation over primitives.
#![no_std]

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn sub(a: i32, b: i32) -> i32 {
    a - b
}

pub fn mul(a: i32, b: i32) -> i32 {
    a * b
}

pub fn div(a: i32, b: i32) -> i32 {
    a / b
}

pub fn rem(a: i32, b: i32) -> i32 {
    a % b
}

pub fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

pub fn min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

pub fn abs(a: i32) -> i32 {
    if a < 0 { -a } else { a }
}

pub fn fib(n: i32) -> i32 {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}

pub fn factorial(n: u64) -> u64 {
    if n == 0 { 1 } else { n * factorial(n - 1) }
}

pub fn is_even(n: i32) -> bool {
    n % 2 == 0
}

pub fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}
