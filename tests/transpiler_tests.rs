/// Integration tests: run the transpiler on each test input, then run `dotnet
/// test` to verify the generated C# code compiles and produces correct results.
///
/// Run with: `cargo test --test transpiler_tests`
///
/// Prerequisites:
/// - The transpiler binary must be built (`cargo build`).
/// - The .NET 8 SDK must be installed (`dotnet` on PATH).
/// - `LD_LIBRARY_PATH` is set automatically from `rustc --print sysroot`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the compiled transpiler binary.
fn transpiler_bin() -> PathBuf {
    // CARGO_BIN_EXE_<name> is set by cargo for integration tests.
    PathBuf::from(env!("CARGO_BIN_EXE_rust-to-cs-compiler"))
}

/// LD_LIBRARY_PATH value needed to run the transpiler (rustc's own shared libs).
fn sysroot_lib() -> String {
    let out = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("`rustc --print sysroot` failed");
    let sysroot = std::str::from_utf8(&out.stdout)
        .expect("sysroot is not valid UTF-8")
        .trim()
        .to_owned();
    format!("{sysroot}/lib")
}

/// Root of the repository (the directory that contains `Cargo.toml`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run the transpiler on a single `.rs` file, writing output to `out_dir`.
fn transpile(input: &Path, out_dir: &Path) {
    let status = Command::new(transpiler_bin())
        .env("R2CS_OUTPUT_DIR", out_dir)
        .env("LD_LIBRARY_PATH", sysroot_lib())
        .args([
            input.to_str().unwrap(),
            "--edition", "2021",
            "--crate-type", "lib",
            "-A", "dead_code",
        ])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn transpiler: {e}"));

    assert!(
        status.success(),
        "transpiler failed on {}",
        input.display()
    );
}

#[test]
fn transpile_inputs_and_run_dotnet_tests() {
    let root = repo_root();
    let out_dir = root.join("dotnet/r2CsCompilerTests/Generated");
    std::fs::create_dir_all(&out_dir).expect("failed to create Generated/ dir");

    // Transpile all test inputs.
    let inputs = [
        "tests/inputs/arithmetic.rs",
        "tests/inputs/structs.rs",
        "tests/inputs/enums.rs",
        "tests/inputs/casts.rs",
        "tests/inputs/consts.rs",
        "tests/inputs/bitwise.rs",
        "tests/inputs/calls.rs",
        "tests/inputs/traits.rs",
        "tests/inputs/recursion.rs",
        "tests/inputs/generics.rs",
        "tests/inputs/complex_enums.rs",
        "tests/inputs/loops.rs",
    ];
    for rel in &inputs {
        let input = root.join(rel);
        transpile(&input, &out_dir);
    }

    // Run dotnet test on the compiler-tests project.
    let project = root.join("dotnet/r2CsCompilerTests");
    let status = Command::new("dotnet")
        .args(["test", project.to_str().unwrap()])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn `dotnet test`: {e}"));

    assert!(status.success(), "`dotnet test` failed — see output above");
}
