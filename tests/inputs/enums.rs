// Test input: enum definitions and match expressions.
// Uses no std-library functions.
#![no_std]

pub enum Shape {
    Circle { radius: i32 },
    Rectangle { width: i32, height: i32 },
    Triangle { base: i32, height: i32 },
}

/// Returns twice the area (to avoid fractions — circle area ≈ r*r here).
pub fn area_times_2(s: Shape) -> i32 {
    match s {
        Shape::Circle { radius } => radius * radius * 2,
        Shape::Rectangle { width, height } => width * height * 2,
        Shape::Triangle { base, height } => base * height,
    }
}

pub fn perimeter_approx(s: Shape) -> i32 {
    match s {
        Shape::Circle { radius } => radius * 6,     // ~2πr with π≈3
        Shape::Rectangle { width, height } => 2 * (width + height),
        Shape::Triangle { base, height } => base + height * 2, // simplified
    }
}

pub enum Direction {
    North,
    South,
    East,
    West,
}

pub fn opposite(d: Direction) -> Direction {
    match d {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::East  => Direction::West,
        Direction::West  => Direction::East,
    }
}

pub fn direction_value(d: Direction) -> i32 {
    match d {
        Direction::North => 0,
        Direction::South => 1,
        Direction::East  => 2,
        Direction::West  => 3,
    }
}
