// Test input: nested struct operations.
// Uses no std-library functions.
#![no_std]

pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Vec2 {
        Vec2 { x, y }
    }

    pub fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }

    pub fn scale(self, factor: f32) -> Vec2 {
        Vec2 { x: self.x * factor, y: self.y * factor }
    }

    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn len_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
}

pub struct Rect {
    pub origin: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub fn area(self) -> f32 {
        self.size.x * self.size.y
    }

    pub fn center(self) -> Vec2 {
        Vec2 {
            x: self.origin.x + self.size.x * 0.5,
            y: self.origin.y + self.size.y * 0.5,
        }
    }
}

pub fn make_vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

pub fn add_vecs(a: Vec2, b: Vec2) -> Vec2 {
    a.add(b)
}

pub fn dot_product(a: Vec2, b: Vec2) -> f32 {
    a.dot(b)
}

pub fn rect_area(r: Rect) -> f32 {
    r.area()
}
