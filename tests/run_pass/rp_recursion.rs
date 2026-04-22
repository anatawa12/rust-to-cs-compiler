// Adapted from rustc tests/ui/recursion/
// Tests recursive function calls including mutual recursion.
#![no_std]

fn fib(n: u32) -> u64 {
    if n <= 1 { n as u64 } else { fib(n - 1) + fib(n - 2) }
}

fn factorial(n: u64) -> u64 {
    if n == 0 { 1 } else { n * factorial(n - 1) }
}

fn is_even(n: u32) -> bool {
    if n == 0 { true } else { is_odd(n - 1) }
}

fn is_odd(n: u32) -> bool {
    if n == 0 { false } else { is_even(n - 1) }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn main() {
    assert!(fib(0) == 0);
    assert!(fib(1) == 1);
    assert!(fib(10) == 55);

    assert!(factorial(0) == 1);
    assert!(factorial(5) == 120);
    assert!(factorial(10) == 3628800);

    assert!(is_even(0));
    assert!(is_even(4));
    assert!(!is_even(3));
    assert!(is_odd(1));
    assert!(is_odd(7));

    assert!(gcd(12, 8) == 4);
    assert!(gcd(15, 25) == 5);
    assert!(gcd(7, 13) == 1);
}
