// Test input: string operations (using &str, char types).
// Note: no heap allocation — only operations on literals and char primitives.
#![no_std]

pub fn first_char_code(s: &str) -> u8 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        0
    } else {
        bytes[0]
    }
}

pub fn is_ascii_digit(c: char) -> bool {
    c >= '0' && c <= '9'
}

pub fn is_ascii_alpha(c: char) -> bool {
    (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

pub fn ascii_to_upper(c: char) -> char {
    if c >= 'a' && c <= 'z' {
        (c as u8 - b'a' + b'A') as char
    } else {
        c
    }
}

pub fn ascii_to_lower(c: char) -> char {
    if c >= 'A' && c <= 'Z' {
        (c as u8 - b'A' + b'a') as char
    } else {
        c
    }
}

pub fn char_digit_value(c: char) -> i32 {
    if c >= '0' && c <= '9' {
        (c as i32) - ('0' as i32)
    } else {
        -1
    }
}
