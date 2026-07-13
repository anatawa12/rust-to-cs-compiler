//! Extensions for the `hir` crate types without any non-hir crate types

use hir::db::HirDatabase;
use hir::{Adt, Trait, Type};

pub trait TraitExt {
    fn assoc_types(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias>;
}

impl TraitExt for Trait {
    fn assoc_types(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias> {
        self.items(db)
            .into_iter()
            .flat_map(|x| match x {
                hir::AssocItem::TypeAlias(t) => Some(t),
                _ => None,
            })
            .collect()
    }
}

pub trait TypeExt<'db> {
    #[allow(dead_code)]
    fn expect_adt_with_args(&self) -> (Adt, Vec<Option<Type<'db>>>);
    fn expect_adt_of(&self, adt: Adt) -> Vec<Type<'db>>;
}

impl<'db> TypeExt<'db> for Type<'db> {
    fn expect_adt_with_args(&self) -> (Adt, Vec<Option<Type<'db>>>) {
        self.as_adt_with_args()
            .unwrap_or_else(|| panic!("expected adt but was {self:?}"))
    }

    fn expect_adt_of(&self, adt: Adt) -> Vec<Type<'db>> {
        let (adt_of_ty, types) = self
            .as_adt_with_args()
            .unwrap_or_else(|| panic!("expected adt of {adt:?} but was {self:?}"));
        assert_eq!(adt_of_ty, adt, "expected adt of {adt:?} but was {self:?}");

        types.into_iter().flatten().collect()
    }
}
