// Enable access to rustc internal crates.  Requires the `rustc-dev` component
// from rustup and the nightly toolchain specified in .rust-toolchain.toml.
#![feature(rustc_private)]

extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;

mod codegen;

use rustc_driver::Compilation;
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;
use rustc_session::EarlyDiagCtxt;

/// `rustc_driver` callbacks that intercept compilation after type-checking so
/// we can access THIR and generate C# output.
struct R2CsCallbacks;

impl rustc_driver::Callbacks for R2CsCallbacks {
    /// Called after analysis (type-checking, borrow-checking, etc.) completes.
    /// `tcx` is the fully-resolved type context from which we retrieve THIR.
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        compile(tcx);
        // Stop after analysis — we don't want rustc to emit object files.
        Compilation::Stop
    }
}

/// Entry point for C# code generation.  Receives the fully-analysed type
/// context and drives the MIR traversal.
fn compile(tcx: TyCtxt<'_>) {
    let mut w = codegen::writer::CsWriter::new();
    codegen::item::compile_crate(tcx, &mut w);
    let output = w.finish();

    // If R2CS_OUTPUT_DIR is set, write <crate_name>.cs to that directory.
    // Otherwise fall back to stdout (useful for quick manual testing).
    if let Ok(dir) = std::env::var("R2CS_OUTPUT_DIR") {
        let crate_name = tcx.crate_name(rustc_hir::def_id::LOCAL_CRATE);
        let filename = format!("{crate_name}.cs");
        let path = std::path::Path::new(&dir).join(&filename);
        std::fs::write(&path, &output)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
    } else {
        print!("{output}");
    }
}

fn main() {
    // Initialise rustc's env-var-based logger (e.g. RUSTC_LOG=info).
    let early_dcx = EarlyDiagCtxt::new(rustc_session::config::ErrorOutputType::default());
    rustc_driver::init_rustc_env_logger(&early_dcx);

    let exit_code = rustc_driver::catch_with_exit_code(|| {
        let args: Vec<String> = std::env::args().collect();
        rustc_driver::run_compiler(&args, &mut R2CsCallbacks);
    });

    // ExitCode does not implement Into<i32>; use process::exit(0/1) directly.
    std::process::exit(if exit_code == std::process::ExitCode::SUCCESS {
        0
    } else {
        1
    });
}
