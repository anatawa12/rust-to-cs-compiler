#![no_std]
// Inspired by rustc tests/ui/structs/ and tests/ui/impl-trait/
// Tests mutable struct fields, self-referential methods, and builder patterns.

pub struct Vector2 {
    pub x: f64,
    pub y: f64,
}

impl Vector2 {
    pub fn dot(&self, other: &Vector2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn length_squared(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    pub fn scale(&self, factor: f64) -> Vector2 {
        Vector2 { x: self.x * factor, y: self.y * factor }
    }

    pub fn add(&self, other: &Vector2) -> Vector2 {
        Vector2 { x: self.x + other.x, y: self.y + other.y }
    }
}

pub fn dot_product(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let a = Vector2 { x: ax, y: ay };
    let b = Vector2 { x: bx, y: by };
    a.dot(&b)
}

pub fn length_sq(x: f64, y: f64) -> f64 {
    let v = Vector2 { x, y };
    v.length_squared()
}

pub fn scale_x(x: f64, y: f64, factor: f64) -> f64 {
    let v = Vector2 { x, y };
    v.scale(factor).x
}

// Mutable struct update
pub struct Accumulator {
    pub total: i64,
    pub count: i32,
}

impl Accumulator {
    pub fn add(&mut self, val: i64) {
        self.total += val;
        self.count += 1;
    }

    pub fn average(&self) -> i64 {
        if self.count == 0 { 0 } else { self.total / self.count as i64 }
    }
}

pub fn accumulate(a: i64, b: i64, c: i64) -> i64 {
    let mut acc = Accumulator { total: 0, count: 0 };
    acc.add(a);
    acc.add(b);
    acc.add(c);
    acc.average()
}
