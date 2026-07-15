//! Extensions for the `hir` crate types without any non-hir crate types

use hir::db::HirDatabase;
use hir::{Adt, GenericDef, HasContainer, HasCrate, ItemContainer, Trait, Type, sym};
use ra_internal::*;

pub trait TraitExt {
    fn assoc_types_for_cs(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias>;
}

impl TraitExt for Trait {
    fn assoc_types_for_cs(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias> {
        fn is_omit_trait_assoc_type(db: &dyn HirDatabase, alias: hir::TypeAlias) -> bool {
            let trait_ = match alias.container(db) {
                ItemContainer::Trait(t) => t,
                _ => panic!(),
            };
            if trait_.name(db).symbol() == &sym::IntoIterator
                && alias.name(db).symbol() == &sym::IntoIter
            {
                return true;
            }
            false
        }

        self.items(db)
            .into_iter()
            .flat_map(|x| variant_or_none!(x, hir::AssocItem::TypeAlias))
            .filter(|&alias| !is_omit_trait_assoc_type(db, alias))
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

pub trait TypeParamExt<'db> {
    fn is_self(&self, db: &dyn HirDatabase) -> bool;
    /// Returns true if the type parameter is removed in C# code
    fn is_ignored(&self, db: &dyn HirDatabase) -> bool;
}

impl TypeParamExt<'_> for hir::TypeParam {
    fn is_self(&self, db: &dyn HirDatabase) -> bool {
        self.is_implicit(db) && self.name(db) == sym::Self_
    }

    fn is_ignored(&self, db: &dyn HirDatabase) -> bool {
        self.is_self(db)
            || self.is_unstable(db)
            || self
                .default(db)
                .and_then(|x| x.as_adt())
                .map(|x| x.name(db))
                .as_ref()
                .map(|x| x.as_str())
                == Some("RandomState")
    }
}

pub trait GenericDefExt {
    fn params0(self, db: &dyn HirDatabase) -> Vec<hir::GenericParam>;
}

impl GenericDefExt for GenericDef {
    fn params0(self, db: &dyn HirDatabase) -> Vec<hir::GenericParam> {
        if let GenericDef::Function(f) = self
            && let ItemContainer::Impl(impl_) = f.container(db)
            && let Some(trait_) = impl_.trait_(db)
            && Some(trait_) == LangItems::new(db, trait_.krate(db)).Hash()
            && f.name(db) == sym::hash
        {
            // it's hash. Derive method has <H> but `GenericDef::from` returns empty array for
            // derived Hash::hash so we retrieve the H from parameter instead of GenericDef
            let param = f.params_without_self(db).swap_remove(0);
            let param_type = param.ty();
            let param_type = param_type.remove_ref().unwrap();
            let type_param = param_type
                .as_type_param(db)
                .unwrap_or_else(|| panic!("type {param_type:?} is not type_param"));
            vec![type_param.into()]
        } else {
            #[allow(clippy::disallowed_methods)]
            self.params(db)
        }
    }
}
