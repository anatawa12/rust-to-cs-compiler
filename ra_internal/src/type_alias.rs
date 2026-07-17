use crate::internal::{ParsedProjection, parse_bounds_for};
use hir::db::HirDatabase;

pub trait TypeAliasExt {
    fn bounds<'db>(&self, db: &'db dyn HirDatabase) -> Vec<(hir::Trait, Vec<hir::Type<'db>>)>;
}

impl TypeAliasExt for hir::TypeAlias {
    fn bounds<'db>(&self, db: &'db dyn HirDatabase) -> Vec<(hir::Trait, Vec<hir::Type<'db>>)> {
        match parse_bounds_for([], &self.ty(db), db) {
            ParsedProjection::NoBounds => vec![],
            ParsedProjection::Projection(_) => panic!(),
            ParsedProjection::Traits(traits) => traits,
        }
    }
}
