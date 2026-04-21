// Test input: state machine simulation.
// Exercises: enums with data, multiple impls, recursive calls, conditions.
// Uses no std-library functions.
#![no_std]

pub enum Token {
    Number(i64),
    Plus,
    Minus,
    Mul,
    Div,
}

pub fn token_value(t: Token) -> i64 {
    match t {
        Token::Number(n) => n,
        _ => 0,
    }
}

pub fn is_operator(t: Token) -> bool {
    match t {
        Token::Plus | Token::Minus | Token::Mul | Token::Div => true,
        _ => false,
    }
}

pub struct Calculator {
    pub accumulator: i64,
    pub last_op: i32,  // 0=+, 1=-, 2=*, 3=/
}

impl Calculator {
    pub fn new() -> Calculator {
        Calculator { accumulator: 0, last_op: 0 }
    }

    pub fn apply(self, op: i32, val: i64) -> Calculator {
        let result = match op {
            0 => self.accumulator + val,
            1 => self.accumulator - val,
            2 => self.accumulator * val,
            3 => if val != 0 { self.accumulator / val } else { self.accumulator },
            _ => self.accumulator,
        };
        Calculator { accumulator: result, last_op: op }
    }

    pub fn result(self) -> i64 {
        self.accumulator
    }
}

pub fn compute(a: i64, op: i32, b: i64) -> i64 {
    let calc = Calculator::new();
    let calc2 = calc.apply(0, a);  // set to a via add 0+a
    let calc3 = calc2.apply(op, b); // apply op
    calc3.result()
}

pub fn multi_op(a: i64, b: i64, c: i64) -> i64 {
    // (a + b) * c
    let sum = a + b;
    let prod = sum * c;
    prod
}
