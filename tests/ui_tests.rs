/// UI-style integration tests: transpile programs from tests/ui_runner/programs/,
/// then verify the generated C# compiles and the dotnet tests pass.
///
/// Run with: `cargo test --test ui_tests`
///
/// See docs/running-ui-tests.md for full documentation.

use std::path::{Path, PathBuf};
use std::process::Command;

fn transpiler_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rust-to-cs-compiler"))
}

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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn transpile(input: &Path, out_dir: &Path) -> Result<(), String> {
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

    if status.success() {
        Ok(())
    } else {
        Err(format!("transpiler failed on {}", input.display()))
    }
}

#[test]
fn ui_transpile_programs_and_run_dotnet_tests() {
    let root = repo_root();
    let programs_dir = root.join("tests/ui_runner/programs");
    let out_dir = root.join("dotnet/r2CsCompilerTests/Generated");
    std::fs::create_dir_all(&out_dir).expect("failed to create GeneratedUI/ dir");

    // Discover all .rs programs, skipping any that are not yet supported.
    let skip_list = &[
        // Generic trait dispatch via type params not yet implemented.
        "traits_generic.rs",
    ];
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(&programs_dir)
        .expect("failed to read programs dir")
        .filter_map(|e| {
            let entry = e.ok()?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                let name = path.file_name()?.to_str()?.to_owned();
                if skip_list.iter().any(|s| *s == name) {
                    return None;
                }
                Some(path)
            } else {
                None
            }
        })
        .collect();
    inputs.sort();

    let mut failures: Vec<String> = Vec::new();
    for input in &inputs {
        if let Err(e) = transpile(input, &out_dir) {
            failures.push(e);
        }
    }

    if !failures.is_empty() {
        panic!("transpiler failures:\n{}", failures.join("\n"));
    }

    // Run dotnet test (the GeneratedUI files are included by the existing project).
    let project = root.join("dotnet/r2CsCompilerTests");
    let status = Command::new("dotnet")
        .args(["test", project.to_str().unwrap()])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn `dotnet test`: {e}"));

    assert!(status.success(), "`dotnet test` failed — see output above");
}
