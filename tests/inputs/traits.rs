// Test input: trait definitions and implementations.
// Uses no std-library functions.
#![no_std]

pub trait Describable {
    fn value(self) -> i32;
    fn doubled(self) -> i32;
}

pub struct Wrapper(pub i32);

impl Describable for Wrapper {
    fn value(self) -> i32 {
        self.0
    }

    fn doubled(self) -> i32 {
        self.0 * 2
    }
}

pub fn get_value(w: Wrapper) -> i32 {
    w.value()
}

pub fn get_doubled(w: Wrapper) -> i32 {
    w.doubled()
}

pub trait Counter {
    fn count(self) -> u32;
    fn is_even(self) -> bool;
}

pub struct Counter3(pub u32);
pub struct Counter7(pub u32);

impl Counter for Counter3 {
    fn count(self) -> u32 {
        self.0
    }
    fn is_even(self) -> bool {
        self.0 % 2 == 0
    }
}

impl Counter for Counter7 {
    fn count(self) -> u32 {
        self.0
    }
    fn is_even(self) -> bool {
        self.0 % 2 == 0
    }
}

pub fn triple_count(c: Counter3) -> u32 {
    c.count() * 3
}

pub fn seven_is_even(c: Counter7) -> bool {
    c.is_even()
}
