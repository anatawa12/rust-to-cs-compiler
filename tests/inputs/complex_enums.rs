// Test input: complex enum usage with multi-variant matching.
// Uses no std-library functions.
#![no_std]

pub enum Shape {
    Circle { radius: i32 },
    Rect { width: i32, height: i32 },
    Triangle { base: i32, height: i32 },
}

pub fn area_x2(s: Shape) -> i32 {
    match s {
        Shape::Circle { radius } => radius * radius, // approximation (not π)
        Shape::Rect { width, height } => width * height,
        Shape::Triangle { base, height } => base * height,
    }
}

pub fn is_square(s: Shape) -> bool {
    match s {
        Shape::Rect { width, height } => width == height,
        _ => false,
    }
}

pub fn make_circle(r: i32) -> Shape {
    Shape::Circle { radius: r }
}

pub fn make_rect(w: i32, h: i32) -> Shape {
    Shape::Rect { width: w, height: h }
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
        Direction::East => Direction::West,
        Direction::West => Direction::East,
    }
}

pub fn is_north(d: Direction) -> bool {
    match d {
        Direction::North => true,
        _ => false,
    }
}
