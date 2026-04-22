// Adapted from rustc tests/ui/while-loop.rs and tests/ui/loops/
// Tests while/loop control flow constructs.
#![no_std]

fn sum(n: i32) -> i32 {
    let mut s = 0;
    let mut i = 1;
    while i <= n {
        s += i;
        i += 1;
    }
    s
}

fn countdown(start: i32) -> i32 {
    let mut x = start;
    loop {
        if x == 0 { break; }
        x -= 1;
    }
    x
}

fn break_value() -> i32 {
    let mut i = 0;
    loop {
        i += 1;
        if i == 5 { break i * 2; }
    }
}

fn main() {
    assert!(sum(10) == 55);
    assert!(sum(0) == 0);
    assert!(sum(1) == 1);
    assert!(countdown(5) == 0);
    assert!(break_value() == 10);
}
