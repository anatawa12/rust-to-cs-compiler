#![no_std]
// Inspired by rustc tests/ui/if-let/ and tests/ui/pattern/
// Tests if-let, while-let, and nested pattern matching.

pub enum Tree {
    Leaf(i32),
    Node(i32, i32), // left_val, right_val (simplified; no Box)
}

pub fn tree_sum(t: Tree) -> i32 {
    match t {
        Tree::Leaf(v) => v,
        Tree::Node(l, r) => l + r,
    }
}

pub fn max_of_tree(t: Tree) -> i32 {
    match t {
        Tree::Leaf(v) => v,
        Tree::Node(l, r) => if l >= r { l } else { r },
    }
}

pub enum MaybeInt {
    None,
    Some(i32),
}

pub fn unwrap_or(m: MaybeInt, default: i32) -> i32 {
    match m {
        MaybeInt::Some(v) => v,
        MaybeInt::None => default,
    }
}

pub fn map_double(m: MaybeInt) -> MaybeInt {
    match m {
        MaybeInt::Some(v) => MaybeInt::Some(v * 2),
        MaybeInt::None => MaybeInt::None,
    }
}

pub fn is_some(m: MaybeInt) -> bool {
    match m {
        MaybeInt::Some(_) => true,
        MaybeInt::None => false,
    }
}

// Nested enum matching
pub enum Pair {
    Both(MaybeInt, MaybeInt),
    OnlyFirst(i32),
    Empty,
}

pub fn pair_sum(p: Pair) -> i32 {
    match p {
        Pair::Both(MaybeInt::Some(a), MaybeInt::Some(b)) => a + b,
        Pair::Both(MaybeInt::Some(a), MaybeInt::None) => a,
        Pair::Both(MaybeInt::None, MaybeInt::Some(b)) => b,
        Pair::Both(MaybeInt::None, MaybeInt::None) => 0,
        Pair::OnlyFirst(v) => v,
        Pair::Empty => 0,
    }
}
