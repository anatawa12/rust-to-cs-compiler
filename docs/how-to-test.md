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

Expected output: **52 tests, 0 failures**.

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

Expected output: **24 tests, 0 failures**.

### Run the transpiler on a Rust source file

The transpiler acts as a `rustc` wrapper — it accepts the same flags rustc does
and writes the generated C# to **stdout**.  You need to point the dynamic
linker at rustc's own libraries:

```sh
# One-time helper (adjust if your sysroot path differs):
export LD_LIBRARY_PATH="$(rustc --print sysroot)/lib"

# Transpile a file:
./target/debug/rust-to-cs-compiler <path/to/input.rs> --edition 2021
```

Example:

```sh
cat > /tmp/hello.rs << 'EOF'
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
fn main() {}
EOF

./target/debug/rust-to-cs-compiler /tmp/hello.rs --edition 2021
```

The generated C# is printed to stdout.  Rustc diagnostics (warnings, errors)
are printed to stderr.

### Suppress rustc dead-code warnings during testing

Pass `--allow dead_code` to suppress the usual "function is never used"
warnings that appear when transpiling isolated files:

```sh
./target/debug/rust-to-cs-compiler /tmp/hello.rs --edition 2021 \
  -A dead_code 2>/dev/null
```

---

## Running both suites at once

```sh
cargo test && dotnet test dotnet/r2CsCompilerRuntime.slnx
```
