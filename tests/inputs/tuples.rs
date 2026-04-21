// Test input: tuple types and operations.
// Uses no std-library functions.
#![no_std]

pub fn make_pair(a: i32, b: i64) -> (i32, i64) {
    (a, b)
}

pub fn fst(t: (i32, i64)) -> i32 {
    t.0
}

pub fn snd(t: (i32, i64)) -> i64 {
    t.1
}

pub fn swap_pair(t: (i32, i64)) -> (i64, i32) {
    (t.1, t.0)
}

pub fn add_pair(t: (i32, i32)) -> i32 {
    t.0 + t.1
}

pub fn triple(a: i32, b: i32, c: i32) -> (i32, i32, i32) {
    (a, b, c)
}

pub fn sum_triple(t: (i32, i32, i32)) -> i32 {
    t.0 + t.1 + t.2
}

pub fn max_of_pair(t: (i32, i32)) -> i32 {
    if t.0 > t.1 { t.0 } else { t.1 }
}
