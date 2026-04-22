// Adapted from rustc tests/ui/structs/
// Tests struct construction and field access.
#![no_std]

struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn distance_sq(&self) -> i32 {
        self.x * self.x + self.y * self.y
    }

    fn translate(&self, dx: i32, dy: i32) -> Point {
        Point { x: self.x + dx, y: self.y + dy }
    }
}

struct Rect {
    top_left: Point,
    bottom_right: Point,
}

impl Rect {
    fn area(&self) -> i32 {
        let w = self.bottom_right.x - self.top_left.x;
        let h = self.bottom_right.y - self.top_left.y;
        w * h
    }
}

fn main() {
    let p = Point::new(3, 4);
    assert!(p.x == 3);
    assert!(p.y == 4);
    assert!(p.distance_sq() == 25);

    let q = p.translate(1, 2);
    assert!(q.x == 4);
    assert!(q.y == 6);

    let r = Rect {
        top_left: Point::new(0, 0),
        bottom_right: Point::new(5, 3),
    };
    assert!(r.area() == 15);
}
