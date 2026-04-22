// Test input: functions calling other functions.
// Uses no std-library functions.
#![no_std]

fn double(x: i32) -> i32 {
    x + x
}

fn triple(x: i32) -> i32 {
    x + x + x
}

pub fn double_then_triple(x: i32) -> i32 {
    triple(double(x))
}

pub fn sum_of_doubles(a: i32, b: i32) -> i32 {
    double(a) + double(b)
}

pub fn apply_twice(x: i32) -> i32 {
    double(double(x))
}

fn square(x: i32) -> i32 {
    x * x
}

pub fn sum_of_squares(a: i32, b: i32) -> i32 {
    square(a) + square(b)
}

pub fn pythagorean_check(a: i32, b: i32, c: i32) -> bool {
    square(a) + square(b) == square(c)
}
