# Running Rust UI-style Tests

This directory (`tests/ui_runner/`) contains infrastructure for running a subset
of Rust runtime-behaviour tests through the transpiler and checking the C# output
produces correct results.

## What these tests cover

Each test is a small `.rs` file (in `tests/ui_runner/programs/`) that:
1. Contains `pub fn main_result() -> i32` returning a "check value" (or other
   exported functions whose return values are asserted in C#).
2. Represents a pattern taken from (or inspired by) Rust's own `tests/ui/`
   directory – covering arithmetic, overflow, string handling, iterators, etc.

## How to run

```sh
# Build the transpiler first:
cargo build

# Run all tests (transpile + dotnet test):
cargo test --test ui_tests

# Run only a specific test file by name filter:
cargo test --test ui_tests -- ui_basic_arithmetic
```

## How tests work

1. `tests/ui_tests.rs` discovers all `.rs` files under `tests/ui_runner/programs/`.
2. Each program is transpiled to `dotnet/r2CsUITests/Generated/<name>.cs`.
3. `dotnet test dotnet/r2CsUITests` runs the generated `UITest_<name>.cs` harness
   which calls the exported functions and asserts expected values.

## Adding a new test

1. Create `tests/ui_runner/programs/<name>.rs` with one or more `pub fn` returning
   a primitive value.
2. Create `dotnet/r2CsUITests/UITest_<name>.cs` with xUnit `[Fact]` methods that
   call the generated functions and `Assert.Equal(expected, actual)`.
3. The test is automatically discovered by `cargo test --test ui_tests`.

## Relationship to the Rust compiler test suite

The programs in `programs/` are designed to be semantically equivalent to programs
found in `rustc`'s `tests/ui/` directory (mainly under `tests/ui/numbers-arithmetic/`,
`tests/ui/closures/`, `tests/ui/structs/`, etc.).  They do **not** use `std` I/O
(to stay `#![no_std]`-compatible with the transpiler's current capabilities);
instead correctness is verified by return values.
