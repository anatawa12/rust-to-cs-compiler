use tracing_subscriber::prelude::*;

mod codegen;

use cfg::{CfgAtom, CfgDiff};
use hir::db::HirDatabase;
use hir::{Crate, Symbol};
use ide_db::base_db::{CrateDisplayName, all_crates};
use ide_db::{FxHashMap, RootDatabase};
use load_cargo::{LoadCargoConfig, load_workspace};
use project_model::{CargoConfig, CargoFeatures, ProjectManifest, ProjectWorkspace, RustLibSource};
use vfs::{AbsPathBuf, Vfs};

fn load_workspace_from_cargo(
    path: &str,
    env: &FxHashMap<String, Option<String>>,
) -> (RootDatabase, Vfs, Vec<Crate>) {
    let manifest = ProjectManifest::discover_single(&AbsPathBuf::assert(path.into())).unwrap();

    let mut cargo_config = CargoConfig {
        sysroot: Some(RustLibSource::Discover),
        features: CargoFeatures::Selected {
            features: vec![],
            no_default_features: true,
        },
        ..CargoConfig::default()
    };
    cargo_config.cfg_overrides.global =
        CfgDiff::new(vec![CfgAtom::Flag(Symbol::intern("r2cs"))], vec![]);
    let ws = ProjectWorkspace::load(manifest, &cargo_config, &|_| {}).unwrap();

    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: true,
        with_proc_macro_server: load_cargo::ProcMacroServerChoice::Sysroot,
        prefill_caches: false,
        num_worker_threads: 10,
        proc_macro_processes: 10,
    };

    let (db, vfs, _proc_macro) = load_workspace(ws, env, &load_config).unwrap();

    let crates = all_crates(&db).iter().map(|&k| Crate::from(k)).collect();

    (db, vfs, crates)
}

fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .pretty(),
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("rust_to_cs_compiler=debug,warn")
            }),
        )
        .init();
    std::panic::set_hook(Box::new(tracing_panic::panic_hook));

    let manifest_path = std::path::Path::new("./vrc-get/vrc-get-vpm/Cargo.toml")
        .canonicalize()
        .unwrap();

    let (db, vfs, crates) = load_workspace_from_cargo(
        manifest_path.to_string_lossy().as_ref(),
        &FxHashMap::default(),
    );
    let db: &dyn HirDatabase = &db;

    hir::attach_db(db, || {
        for krate in &crates {
            let name = krate.display_name(db);
            //println!("crate map: {name:?}: {krate:?}");
            if name != Some(CrateDisplayName::from_canonical_name("vrc_get_vpm")) {
                continue;
            }
            eprintln!("Transpiling crate: {}", name.unwrap());

            let generator = codegen::CodeGenerator::new(db, &vfs, *krate, "VrcGetVpm".into());
            let output = generator.emit_crate(*krate);
            println!("{}", output);
        }
    });
}
