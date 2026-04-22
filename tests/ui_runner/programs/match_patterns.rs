#![no_std]
/// Inspired by rustc tests/ui/match/

pub enum Direction {
    North,
    South,
    East,
    West,
}

pub fn direction_value(d: Direction) -> i32 {
    match d {
        Direction::North => 0,
        Direction::South => 1,
        Direction::East  => 2,
        Direction::West  => 3,
    }
}

pub enum Shape {
    Circle(i32),       // radius
    Rectangle(i32, i32), // width, height
    Triangle(i32),     // base (height == base)
}

pub fn area_times_2(s: Shape) -> i32 {
    match s {
        Shape::Circle(r)         => r * r,           // π≈1 for integer math
        Shape::Rectangle(w, h)   => w * h * 2,
        Shape::Triangle(b)       => b * b,           // base*height/2*2 = base*base when height==base
    }
}

pub fn categorize(n: i32) -> i32 {
    match n {
        i32::MIN..=-1 => -1,
        0             => 0,
        1..=9         => 1,
        10..=99       => 2,
        _             => 3,
    }
}

pub fn fizzbuzz(n: i32) -> i32 {
    match (n % 3 == 0, n % 5 == 0) {
        (true, true)  => 15,
        (true, false) => 3,
        (false, true) => 5,
        (false, false) => n,
    }
}
