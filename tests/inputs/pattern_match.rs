// Test input: pattern matching with guards and nested data.
// Uses no std-library functions.
#![no_std]

pub enum Expr {
    Num(i32),
    Add(i32, i32),
    Mul(i32, i32),
    Neg(i32),
}

pub fn eval(e: Expr) -> i32 {
    match e {
        Expr::Num(x) => x,
        Expr::Add(a, b) => a + b,
        Expr::Mul(a, b) => a * b,
        Expr::Neg(x) => -x,
    }
}

pub fn is_zero(e: Expr) -> bool {
    eval(e) == 0
}

pub enum Color {
    Red,
    Green,
    Blue,
    Custom(u8, u8, u8),
}

pub fn is_primary(c: Color) -> bool {
    match c {
        Color::Red | Color::Green | Color::Blue => true,
        Color::Custom(_, _, _) => false,
    }
}

pub fn red_component(c: Color) -> u8 {
    match c {
        Color::Red => 255,
        Color::Green => 0,
        Color::Blue => 0,
        Color::Custom(r, _, _) => r,
    }
}

pub fn brightness(c: Color) -> u32 {
    match c {
        Color::Red => 76,
        Color::Green => 150,
        Color::Blue => 29,
        Color::Custom(r, g, b) => (r as u32) + (g as u32) + (b as u32),
    }
}
