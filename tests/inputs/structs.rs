// Test input: struct definitions and inherent methods.
// Uses no std-library functions.
#![no_std]

pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    pub fn distance_squared(self) -> i32 {
        self.x * self.x + self.y * self.y
    }

    pub fn translate(self, dx: i32, dy: i32) -> Point {
        Point { x: self.x + dx, y: self.y + dy }
    }

    pub fn scale(self, factor: i32) -> Point {
        Point { x: self.x * factor, y: self.y * factor }
    }

    pub fn x(self) -> i32 {
        self.x
    }

    pub fn y(self) -> i32 {
        self.y
    }
}

pub struct Rectangle {
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    pub fn new(width: i32, height: i32) -> Rectangle {
        Rectangle { width, height }
    }

    pub fn area(self) -> i32 {
        self.width * self.height
    }

    pub fn perimeter(self) -> i32 {
        2 * (self.width + self.height)
    }

    pub fn is_square(self) -> bool {
        self.width == self.height
    }
}
