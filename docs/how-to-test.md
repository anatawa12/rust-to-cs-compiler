# How to Build, Run, and Test

## Prerequisites

* [Rust / rustup](https://rustup.rs/) — the nightly toolchain and the `rustc-dev`
  component are declared in `rust-toolchain.toml` and installed automatically
  the first time you run any `cargo` command in this directory.
* [.NET 8 SDK](https://dotnet.microsoft.com/download) — required for the C#
  runtime library and its tests.

---

## C# Runtime Library

### Build

```sh
dotnet build dotnet/r2CsCompilerRuntime.slnx
```

### Test

```sh
dotnet test dotnet/r2CsCompilerRuntime.slnx
```

Expected output: **96 tests, 0 failures** (52 runtime tests + 44 compiler output
tests).

---

## Rust Transpiler

### Build

```sh
cargo build
```

The first build may take a few minutes while rustup downloads and installs the
nightly toolchain (`nightly-2026-04-20`) together with `rustc-dev`.

### Unit tests (Rust)

```sh
cargo test
```

Expected output: **1 integration test + 24 unit tests, 0 failures**.

The integration test (`tests/transpiler_tests.rs`) automatically:
1. Runs the transpiler on every file in `tests/inputs/`.
2. Writes the generated C# to `dotnet/r2CsCompilerTests/Generated/`.
3. Runs `dotnet test dotnet/r2CsCompilerTests/` to verify the generated code
   compiles and produces correct results.

---

## Run the transpiler manually

The transpiler acts as a `rustc` wrapper and accepts the same flags.

### Write output to a file

Set the `R2CS_OUTPUT_DIR` environment variable to a directory.  The output file
is named `<crate_name>.cs` inside that directory.

```sh
export LD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export R2CS_OUTPUT_DIR=/tmp/my_output

mkdir -p "$R2CS_OUTPUT_DIR"
./target/debug/rust-to-cs-compiler path/to/input.rs --edition 2021
# → writes /tmp/my_output/<crate_name>.cs
```

### Write output to stdout (fallback)

Omit `R2CS_OUTPUT_DIR` and the generated C# is printed to stdout:

```sh
export LD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
./target/debug/rust-to-cs-compiler path/to/input.rs --edition 2021
```

### Transpile one of the test inputs

```sh
export LD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export R2CS_OUTPUT_DIR=/tmp/r2cs_out
mkdir -p "$R2CS_OUTPUT_DIR"

./target/debug/rust-to-cs-compiler tests/inputs/arithmetic.rs \
    --edition 2021 --crate-type lib -A dead_code

cat "$R2CS_OUTPUT_DIR/arithmetic.cs"
```

### Suppress rustc dead-code warnings

Pass `-A dead_code` to suppress "function/struct is never used" warnings when
transpiling isolated files.

---

## Running everything at once

```sh
cargo test && dotnet test dotnet/r2CsCompilerRuntime.slnx
```

