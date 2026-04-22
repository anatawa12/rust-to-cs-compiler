// Test input: generic structs and functions.
// Uses no std-library functions.
#![no_std]

pub struct Pair<A, B> {
    pub first: A,
    pub second: B,
}

impl<A: Copy, B: Copy> Pair<A, B> {
    pub fn swap(self) -> Pair<B, A> {
        Pair { first: self.second, second: self.first }
    }

    pub fn first(self) -> A {
        self.first
    }

    pub fn second(self) -> B {
        self.second
    }
}

pub fn make_pair(a: i32, b: i64) -> Pair<i32, i64> {
    Pair { first: a, second: b }
}

pub fn swap_ints(p: Pair<i32, i64>) -> Pair<i64, i32> {
    p.swap()
}

pub fn get_first(p: Pair<i32, i64>) -> i32 {
    p.first()
}

pub fn get_second(p: Pair<i32, i64>) -> i64 {
    p.second()
}

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub fn origin() -> Point {
    Point { x: 0, y: 0 }
}

pub fn distance_squared(p: Point) -> i64 {
    let x = p.x as i64;
    let y = p.y as i64;
    x * x + y * y
}
