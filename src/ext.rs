use hir::db::HirDatabase;
use hir::{HasName, Module};
use hir::tt::pretty;

pub trait ModuleExt {
    fn module_path(self, db: &dyn HirDatabase) -> String;
}

impl ModuleExt for Module {
    fn module_path(self, db: &dyn HirDatabase) -> String {
        if let Some(parent) = self.parent(db) {
            let mut path = parent.module_path(db);
            path.push_str("::");
            path.push_str(self.name(db).as_ref().map(|x| x.as_str()).unwrap_or("{}"));
            path
        } else {
            self.krate(db).display_name(db).unwrap().as_str().to_string()
        }
    }
}
