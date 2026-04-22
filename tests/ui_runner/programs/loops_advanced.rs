#![no_std]
// Inspired by rustc tests/ui/loops/ and tests/ui/for/
// Tests loop, while, break with values, continue, and nested loops.

pub fn sum_evens_up_to(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    while i <= n {
        if i % 2 == 0 {
            sum += i;
        }
        i += 1;
    }
    sum
}

pub fn first_greater_than(arr_sum: i32, threshold: i32) -> i32 {
    // Simulate searching: return first cumulative sum > threshold.
    let mut s = 0;
    let mut i = 1;
    loop {
        s += i;
        if s > threshold {
            return s;
        }
        if i > 1000 { break; }
        i += 1;
    }
    -1
}

pub fn loop_break_value(n: i32) -> i32 {
    let mut i = 0;
    let result = loop {
        i += 1;
        if i >= n {
            break i * 2;
        }
    };
    result
}

pub fn nested_loop_sum(rows: i32, cols: i32) -> i32 {
    let mut total = 0;
    let mut r = 0;
    while r < rows {
        let mut c = 0;
        while c < cols {
            total += r * cols + c;
            c += 1;
        }
        r += 1;
    }
    total
}

pub fn continue_skip_multiples_of_3(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    while i < n {
        i += 1;
        if i % 3 == 0 {
            continue;
        }
        sum += i;
    }
    sum
}
