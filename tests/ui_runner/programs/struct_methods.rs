#![no_std]
/// Inspired by rustc tests/ui/structs/

pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub struct Rect {
    pub top_left: Point,
    pub bottom_right: Point,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.bottom_right.x - self.top_left.x
    }
    pub fn height(&self) -> i32 {
        self.bottom_right.y - self.top_left.y
    }
    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }
}

pub fn make_rect(x1: i32, y1: i32, x2: i32, y2: i32) -> Rect {
    Rect {
        top_left: Point { x: x1, y: y1 },
        bottom_right: Point { x: x2, y: y2 },
    }
}

pub fn rect_area(x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    let r = make_rect(x1, y1, x2, y2);
    r.area()
}

pub struct Counter {
    pub value: i32,
}

impl Counter {
    pub fn increment(&mut self) {
        self.value += 1;
    }
    pub fn get(&self) -> i32 {
        self.value
    }
}

pub fn count_up(n: i32) -> i32 {
    let mut c = Counter { value: 0 };
    let mut i = 0;
    while i < n {
        c.increment();
        i += 1;
    }
    c.get()
}
