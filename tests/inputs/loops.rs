// Test input: loop and iteration constructs.
// Uses no std-library functions.
#![no_std]

pub fn sum_to(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 1;
    while i <= n {
        sum += i;
        i += 1;
    }
    sum
}

pub fn product_to(n: i32) -> i32 {
    let mut prod = 1;
    let mut i = 2;
    while i <= n {
        prod *= i;
        i += 1;
    }
    prod
}

pub fn count_down(start: i32) -> i32 {
    let mut x = start;
    let mut count = 0;
    loop {
        if x <= 0 {
            break;
        }
        x -= 1;
        count += 1;
    }
    count
}

pub fn first_multiple_of_7_above(x: i32) -> i32 {
    let mut n = x + 1;
    loop {
        if n % 7 == 0 {
            return n;
        }
        n += 1;
    }
}

pub fn collatz_steps(mut n: u64) -> u64 {
    let mut steps = 0u64;
    while n != 1 {
        if n % 2 == 0 {
            n /= 2;
        } else {
            n = 3 * n + 1;
        }
        steps += 1;
    }
    steps
}

pub fn max_in_range(start: i32, end: i32) -> i32 {
    let mut max = start;
    let mut i = start + 1;
    while i < end {
        if i > max {
            max = i;
        }
        i += 1;
    }
    max
}
