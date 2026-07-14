//! Extensions for the `hir` crate types without any non-hir crate types

use hir::db::HirDatabase;
use hir::{Adt, Trait, Type};
use ra_internal::*;

pub trait TraitExt {
    fn assoc_types(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias>;
}

impl TraitExt for Trait {
    fn assoc_types(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias> {
        self.items(db)
            .into_iter()
            .flat_map(|x| variant_or_none!(x, hir::AssocItem::TypeAlias))
            .collect()
    }
}

pub trait TypeExt<'db> {
    #[allow(dead_code)]
    fn expect_adt_with_args(&self) -> (Adt, Vec<Option<Type<'db>>>);
    fn expect_adt_of(&self, adt: Adt) -> Vec<Type<'db>>;

    /// Returns Some of this type is `<SomeParam as Trait>::AssociatedType` or nested it
    fn as_assoc_of_type_param(
        &self,
        db: &dyn HirDatabase,
    ) -> Option<(hir::TypeParam, Vec<hir::TypeAlias>)>;
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

    fn as_assoc_of_type_param(
        &self,
        db: &dyn HirDatabase,
    ) -> Option<(hir::TypeParam, Vec<hir::TypeAlias>)> {
        let mut cur = self.clone();
        let mut aliases = vec![];

        while let Some((self_ty, alias)) = cur.as_associated_type() {
            aliases.push(alias);
            cur = self_ty;
        }

        aliases.reverse();

        if let Some(param) = cur.as_type_param(db)
            && !aliases.is_empty()
        {
            Some((param, aliases))
        } else {
            None
        }
    }
}
