// Enable access to rustc internal crates.  Requires the `rustc-dev` component
// from rustup and the nightly toolchain specified in .rust-toolchain.toml.
#![feature(rustc_private)]

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
/// context and drives the THIR traversal.
fn compile(tcx: TyCtxt<'_>) {
    use rustc_hir::def_id::LOCAL_CRATE;
    use rustc_hir::ItemKind;

    let crate_name = tcx.crate_name(LOCAL_CRATE);
    eprintln!("[r2cs] Compiling crate: {crate_name}");

    // Enumerate all local items and dispatch on their kind.
    for item_id in tcx.hir_crate_items(()).free_items() {
        let item = tcx.hir_item(item_id);
        match item.kind {
            ItemKind::Fn { ident, .. } => {
                let def_id = item_id.owner_id.def_id;
                compile_fn(tcx, def_id, ident.name.as_str());
            }
            ItemKind::Struct(ident, _, _) => {
                let cs_name = codegen::naming::struct_name(ident.name.as_str());
                eprintln!("[r2cs]   struct {} → {cs_name}", ident.name);
            }
            ItemKind::Trait(_, _, _, _, ident, _, _, _) => {
                let name = ident.name.as_str();
                let cs_iface = codegen::naming::trait_iface_name(name);
                let cs_static = codegen::naming::trait_static_iface_name(name);
                eprintln!("[r2cs]   trait {name} → {cs_iface}<Self> + {cs_static}");
            }
            _ => {}
        }
    }
}

/// Compile a single function using its THIR representation.
fn compile_fn(tcx: TyCtxt<'_>, def_id: rustc_hir::def_id::LocalDefId, name: &str) {
    let cs_name = codegen::naming::method_name(name);
    eprintln!("[r2cs]   fn {name} → {cs_name}");

    // Obtain THIR for this function.  THIR is the representation used
    // throughout code generation because it retains structured control flow
    // and async/await information (unlike MIR which flattens these).
    let Ok((thir, _root_expr)) = tcx.thir_body(def_id) else {
        eprintln!("[r2cs]     (THIR unavailable — skipping)");
        return;
    };

    let thir = thir.borrow();
    eprintln!(
        "[r2cs]     THIR: {} exprs, {} stmts, {} params",
        thir.exprs.len(),
        thir.stmts.len(),
        thir.params.len(),
    );

    // TODO: walk THIR exprs/stmts and emit C# via CsWriter.
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
