use crate::TypeExt;
use crate::internal::{ParsedProjection, parse_bounds_for};
use hir::HasContainer;
use hir::db::HirDatabase;

pub trait TypeAliasExt: Copy {
    fn bounds<'db>(self, db: &'db dyn HirDatabase) -> Vec<(hir::Trait, Vec<hir::Type<'db>>)>;
}

impl TypeAliasExt for hir::TypeAlias {
    fn bounds<'db>(self, db: &'db dyn HirDatabase) -> Vec<(hir::Trait, Vec<hir::Type<'db>>)> {
        let db_0 = match self.container(db) {
            hir::ItemContainer::Trait(t) => {
                let self_ty = hir::GenericDef::Trait(t)
                    .params(db)
                    .into_iter()
                    .flat_map(|x| variant_or_none!(x, hir::GenericParam::TypeParam))
                    .next()
                    .unwrap();
                self_ty
                    .ty(db)
                    .new_associated_type(&[self], db, |_, _| vec![])
            }
            _ => unreachable!(),
        };

        match parse_bounds_for([], &db_0, db) {
            ParsedProjection::NoBounds => vec![],
            ParsedProjection::Projection(_) => panic!(),
            ParsedProjection::Traits(traits) => traits,
        }
    }
}
