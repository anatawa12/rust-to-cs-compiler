// Test input: recursive functions.
// Uses no std-library functions.
#![no_std]

pub fn factorial(n: u64) -> u64 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

pub fn fibonacci(n: u32) -> u64 {
    if n == 0 {
        0
    } else if n == 1 {
        1
    } else {
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

pub fn power(base: i64, exp: u32) -> i64 {
    if exp == 0 {
        1
    } else {
        base * power(base, exp - 1)
    }
}

pub fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

pub fn sum_digits(n: u64) -> u64 {
    if n < 10 {
        n
    } else {
        (n % 10) + sum_digits(n / 10)
    }
}
