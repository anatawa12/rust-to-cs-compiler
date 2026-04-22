#![no_std]
/// Inspired by rustc tests/ui/closures/ and tests/ui/traits/

pub trait Summable {
    fn value(&self) -> i32;
}

pub struct Wrapper(pub i32);

impl Summable for Wrapper {
    fn value(&self) -> i32 {
        self.0
    }
}

pub fn double_value<T: Summable>(x: &T) -> i32 {
    x.value() * 2
}

pub fn sum_wrapper(a: i32, b: i32) -> i32 {
    let wa = Wrapper(a);
    let wb = Wrapper(b);
    wa.value() + wb.value()
}

pub fn double_wrapper(v: i32) -> i32 {
    let w = Wrapper(v);
    double_value(&w)
}

pub trait Transformable {
    fn transform(self, factor: i32) -> i32;
}

pub struct Scaler(pub i32);

impl Transformable for Scaler {
    fn transform(self, factor: i32) -> i32 {
        self.0 * factor
    }
}

pub fn apply_transform(v: i32, factor: i32) -> i32 {
    let s = Scaler(v);
    s.transform(factor)
}
