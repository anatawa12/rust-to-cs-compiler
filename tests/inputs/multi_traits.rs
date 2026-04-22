// Test input: multiple trait implementations (polymorphism).
// Uses no std-library functions.
#![no_std]

pub trait Measurable {
    fn measure(self) -> i64;
}

pub trait Scalable {
    fn scale(self, factor: i32) -> Self;
}

pub struct Circle {
    pub radius: i32,
}

pub struct Square {
    pub side: i32,
}

impl Measurable for Circle {
    fn measure(self) -> i64 {
        // area ≈ r*r (ignoring π)
        (self.radius as i64) * (self.radius as i64)
    }
}

impl Measurable for Square {
    fn measure(self) -> i64 {
        (self.side as i64) * (self.side as i64)
    }
}

impl Scalable for Circle {
    fn scale(self, factor: i32) -> Circle {
        Circle { radius: self.radius * factor }
    }
}

impl Scalable for Square {
    fn scale(self, factor: i32) -> Square {
        Square { side: self.side * factor }
    }
}

pub fn circle_measure(c: Circle) -> i64 {
    c.measure()
}

pub fn square_measure(s: Square) -> i64 {
    s.measure()
}

pub fn scaled_circle_measure(c: Circle, factor: i32) -> i64 {
    c.scale(factor).measure()
}

pub fn scaled_square_measure(s: Square, factor: i32) -> i64 {
    s.scale(factor).measure()
}

pub fn compare_areas(c: Circle, s: Square) -> bool {
    c.measure() > s.measure()
}
