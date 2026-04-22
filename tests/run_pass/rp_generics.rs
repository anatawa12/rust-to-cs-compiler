// Adapted from rustc tests/ui/generics/
// Tests generic structs and generic functions with bounds.
#![no_std]

struct Pair<A, B> {
    first: A,
    second: B,
}

impl<A: Copy, B: Copy> Pair<A, B> {
    fn new(a: A, b: B) -> Self { Pair { first: a, second: b } }
    fn swap(self) -> Pair<B, A> { Pair { first: self.second, second: self.first } }
}

fn max_of<T: Copy>(a: T, b: T, gt: fn(T, T) -> bool) -> T {
    if gt(a, b) { a } else { b }
}

fn gt_i32(a: i32, b: i32) -> bool { a > b }

fn main() {
    let p = Pair::new(1_i32, 2_i64);
    assert!(p.first == 1);
    assert!(p.second == 2);

    let q = Pair::new(10_i32, 20_i32).swap();
    assert!(q.first == 20);
    assert!(q.second == 10);

    assert!(max_of(3, 7, gt_i32) == 7);
    assert!(max_of(9, 2, gt_i32) == 9);
    assert!(max_of(5, 5, gt_i32) == 5);
}
