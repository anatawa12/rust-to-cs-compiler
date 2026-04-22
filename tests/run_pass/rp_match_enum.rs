// Adapted from rustc tests/ui/match/
// Tests match expressions and enum patterns.
#![no_std]

enum Direction {
    North,
    South,
    East,
    West,
}

fn opposite(d: Direction) -> Direction {
    match d {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::East  => Direction::West,
        Direction::West  => Direction::East,
    }
}

fn is_horizontal(d: Direction) -> bool {
    match d {
        Direction::East | Direction::West => true,
        _ => false,
    }
}

enum Shape {
    Circle(i32),
    Rectangle(i32, i32),
    Triangle(i32, i32, i32),
}

fn perimeter(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => 6 * r,      // approx 2*pi*r using 3 for pi
        Shape::Rectangle(w, h) => 2 * (w + h),
        Shape::Triangle(a, b, c) => a + b + c,
    }
}

fn main() {
    assert!(is_horizontal(Direction::East));
    assert!(is_horizontal(Direction::West));
    assert!(!is_horizontal(Direction::North));
    assert!(!is_horizontal(Direction::South));

    assert!(perimeter(Shape::Circle(5)) == 30);
    assert!(perimeter(Shape::Rectangle(4, 3)) == 14);
    assert!(perimeter(Shape::Triangle(3, 4, 5)) == 12);
}
