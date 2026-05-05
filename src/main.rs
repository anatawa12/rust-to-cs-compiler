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

    let mut cargo_config = CargoConfig::default();
    cargo_config.sysroot = Some(RustLibSource::Discover);
    cargo_config.features = CargoFeatures::Selected {
        features: vec![],
        no_default_features: true,
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

    let crates = all_crates(&db)
        .into_iter()
        .map(|k| Crate::from(k.clone()))
        .collect();

    (db, vfs, crates)
}

fn main() {
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
            if name != Some(CrateDisplayName::from_canonical_name("vrc_get_vpm")) {
                continue;
            }
            eprintln!("Transpiling crate: {}", name.unwrap());

            let generator = codegen::CodeGenerator::new(db, &vfs, "VrcGetVpm".into());
            let output = generator.emit_crate(*krate);
            println!("{}", output);
        }
    });
}
