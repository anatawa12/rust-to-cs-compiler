mod ext;

use crate::ext::ModuleExt;
use cfg::{CfgAtom, CfgDiff};
use hir::db::{DefDatabase, HirDatabase};
use hir::{AssocItem, Crate, HasName, HasSource, HirDisplay, ModuleDef, Name, Semantics, Symbol};
use ide_db::base_db::{all_crates, CrateDisplayName, SourceDatabase};
use ide_db::{FxHashMap, RootDatabase};
use load_cargo::{load_workspace, LoadCargoConfig};
use project_model::{CargoConfig, ProjectManifest, ProjectWorkspace};
use vfs::AbsPathBuf;

fn load_workspace_from_cargo(path: &str, env: &FxHashMap<String, Option<String>>) -> (RootDatabase, Vec<Crate>) {
    println!("discover_single");
    let manifest = ProjectManifest::discover_single(&AbsPathBuf::assert(path.into())).unwrap();

    let mut cargo_config = CargoConfig::default();
    cargo_config.cfg_overrides.global = CfgDiff::new(vec![
        CfgAtom::Flag(Symbol::intern("r2cs")),
    ], vec![]);
    println!("ProjectWorkspace::load");
    let ws = ProjectWorkspace::load(manifest, &cargo_config, &|_| {}).unwrap();

    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: true,
        with_proc_macro_server: load_cargo::ProcMacroServerChoice::Sysroot,
        prefill_caches: false, // important: this is very heavy
        num_worker_threads: 10,
        proc_macro_processes: 10,
    };

    println!("load_workspace");
    let (db, vfs, _proc_macro) = load_workspace(ws, env, &load_config).unwrap();

    let mut crates = Vec::new();

    println!("all_crates");
    for krate in all_crates(&db).into_iter() {
        crates.push(Crate::from(krate.clone()));
    }

    (db, crates)
}

fn main() {
    //tracing_subscriber::fmt::init();

    let (db, crates) = load_workspace_from_cargo(
        std::path::Path::new("./vrc-get/vrc-get-vpm/Cargo.toml").canonicalize().unwrap().to_string_lossy().as_ref(),
        &FxHashMap::default(),
    );
    let db = &db;

    let sem = Semantics::new(db);

    hir::attach_db(db, || {
        for c in crates {
            if c.display_name(db) == Some(CrateDisplayName::from_canonical_name("vrc_get_vpm")) {
                println!("{:}", c.display_name(db).unwrap());

                for module in c.modules(db) {
                    println!("{:}", module.module_path(db));
                    for def in module.declarations(db) {
                        match def {
                            ModuleDef::Module(m) => {
                                println!("  child mod: {}", m.module_path(db));
                            }
                            ModuleDef::Function(f) => {
                                println!("  fn: {}", f.name(db).as_str())
                            }
                            ModuleDef::Adt(adt) => {
                                println!("  adt: {}", adt.name(db).as_str());
                                println!("    ty: {:?}", adt.ty(db));
                            }
                            ModuleDef::EnumVariant(v) => {
                                println!("  variant: {}", v.name(db).as_str())
                            }
                            ModuleDef::Const(c) => {
                                println!("  const: {}", c.name(db).as_ref().map(Name::as_str).unwrap_or("{unnnamed}"))
                            }
                            ModuleDef::Static(s) => {
                                println!("  static: {}", s.name(db).as_str());
                            }
                            ModuleDef::Trait(t) => {
                                println!("  trait: {}", t.name(db).as_str());
                            }
                            ModuleDef::TypeAlias(a) => {
                                println!("  alias: {}", a.name(db).as_str());
                            }
                            ModuleDef::BuiltinType(t) => {
                                println!("  builtin: {}", t.name().as_str());
                            }
                            ModuleDef::Macro(m) => {
                                println!("  m: {}", m.name(db).as_str());
                            }
                        }
                    }
                    for impl_ in module.impl_defs(db) {
                        println!("  impl for: {:?}", impl_.self_ty(db));
                        for item in impl_.items(db) {
                            match item {
                                AssocItem::Function(f) => {
                                    println!("    fn: {}", f.name(db).as_str());
                                    let block = sem.source(f).unwrap().value.body().unwrap();
                                    let list = block.stmt_list().unwrap();
                                    for x in list.statements() {
                                        println!("      body: {x:?}");
                                    }
                                    if let Some(stmt) = list.tail_expr() {
                                        println!("      tail: {stmt:?}");
                                        println!("      type: {:?}", sem.type_of_expr(&stmt));
                                    }
                                }
                                AssocItem::Const(c) => {
                                    println!("      const: {}", c.name(db).unwrap().as_str())
                                }
                                AssocItem::TypeAlias(alias) => {
                                    println!("      alias: {}", alias.name(db).as_str())
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}
