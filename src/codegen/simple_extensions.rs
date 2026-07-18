//! Extensions for the `hir` crate types without any non-hir crate types

use crate::codegen::ty::is_omit_trait_assoc_type;
use hir::db::HirDatabase;
use hir::{
    Adt, AssocItem, AssocItemContainer, GenericDef, GenericParam, HasContainer, HasCrate,
    ItemContainer, Trait, Type, sym,
};
use itertools::Either;
use ra_internal::function::FunctionExt;
use ra_internal::*;
use std::collections::HashSet;

pub trait TraitExt {
    fn assoc_types_for_cs(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias>;
    fn needs_statics(self, db: &dyn HirDatabase) -> bool;
}

impl TraitExt for Trait {
    fn assoc_types_for_cs(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias> {
        self.items(db)
            .into_iter()
            .flat_map(|x| variant_or_none!(x, hir::AssocItem::TypeAlias))
            .filter(|&alias| !is_omit_trait_assoc_type(db, alias))
            .collect()
    }

    fn needs_statics(self, db: &dyn HirDatabase) -> bool {
        self.items(db).iter().any(|item| match *item {
            hir::AssocItem::Function(f) => !f.has_self_param(db) && !f.is_explicit_sized_self(db),
            hir::AssocItem::Const(_) => false,
            hir::AssocItem::TypeAlias(_) => false,
        })
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

pub trait TypeParamExt {
    fn trait_bounds_with_args(
        self,
        db: &'_ dyn HirDatabase,
    ) -> Vec<(hir::Trait, Vec<hir::Type<'_>>)>;
    fn trait_bounds_of_nested_type_with_args<'db>(
        self,
        t: &hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(hir::Trait, Vec<hir::Type<'db>>)>, hir::Type<'db>>;
    fn is_self(&self, db: &dyn HirDatabase) -> bool;
    /// Returns true if the type parameter is removed in C# code
    fn is_ignored(&self, db: &dyn HirDatabase) -> bool;
}

impl TypeParamExt for hir::TypeParam {
    fn trait_bounds_with_args(
        self,
        db: &'_ dyn HirDatabase,
    ) -> Vec<(hir::Trait, Vec<hir::Type<'_>>)> {
        //*
        match self.trait_bounds_of_nested_type_with_args(&self.ty(db), db) {
            Either::Left(traits) => traits,
            Either::Right(_) => unreachable!(),
        }
        /*
        let mut self_args = self.trait_bounds_with_args_self(db);
        if let Some(parent) = self.get_trait_base(db) {
            self_args.extend(parent.trait_bounds_with_args_self(db));
        }
        self_args
        // */
    }
    fn trait_bounds_of_nested_type_with_args<'db>(
        self,
        t: &hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(hir::Trait, Vec<hir::Type<'db>>)>, hir::Type<'db>> {
        match self.trait_bounds_of_nested_type_with_args_self(t, db) {
            Either::Left(mut traits) => {
                if let Some(base) = self.get_trait_base(db) {
                    let AssocItemContainer::Impl(impl_) =
                        self.parent(db).to_assoc_item().unwrap().container(db)
                    else {
                        unreachable!();
                    };

                    let t = t.instantiate(
                        self.parent(db),
                        &GenericDef::from(impl_)
                            .params0(db)
                            .into_iter()
                            .flat_map(|a| match a {
                                GenericParam::TypeParam(_) => {
                                    Some(Type::error(db, impl_.krate(db)))
                                }
                                _ => None,
                            })
                            .chain(base.parent(db).params0(db).into_iter().flat_map(|param| {
                                match param {
                                    GenericParam::TypeParam(p) => Some(p.ty(db)),
                                    _ => None,
                                }
                            }))
                            .collect::<Vec<_>>(),
                        db,
                    );

                    match base.trait_bounds_of_nested_type_with_args_self(&t, db) {
                        Either::Left(mut traits_base) => {
                            if !traits_base.is_empty() {
                                let trait_ref = impl_.trait_ref(db).unwrap();
                                let generic_args = trait_ref
                                    .generic_types(db)
                                    .flatten()
                                    .chain(self.parent(db).params0(db).into_iter().flat_map(
                                        |param| match param {
                                            GenericParam::TypeParam(p) => Some(p.ty(db)),
                                            _ => None,
                                        },
                                    ))
                                    .collect::<Vec<_>>();

                                for (_, args) in &mut traits_base {
                                    for ty in args {
                                        *ty = ty.instantiate(base.parent(db), &generic_args, db);
                                    }
                                }

                                traits.extend(traits_base);
                                let mut map = HashSet::new();
                                traits.retain(|trait_| map.insert(trait_.clone()));
                            }
                        }
                        Either::Right(t) => {
                            return Either::Right(t);
                        }
                    }
                }
                Either::Left(traits)
            }
            Either::Right(t) => Either::Right(t),
        }
    }
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

pub trait GenericDefExt: Copy {
    fn params0(self, db: &dyn HirDatabase) -> Vec<hir::GenericParam>;
    fn to_assoc_item(self) -> Option<AssocItem>;
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

    fn to_assoc_item(self) -> Option<AssocItem> {
        match self {
            GenericDef::Function(f) => Some(AssocItem::Function(f)),
            GenericDef::TypeAlias(a) => Some(AssocItem::TypeAlias(a)),
            GenericDef::Const(c) => Some(AssocItem::Const(c)),
            _ => None,
        }
    }
}

pub trait AssocItemExt: Copy {
    fn to_generic_def(self) -> GenericDef;
}

impl AssocItemExt for AssocItem {
    fn to_generic_def(self) -> GenericDef {
        match self {
            AssocItem::Function(f) => GenericDef::Function(f),
            AssocItem::TypeAlias(a) => GenericDef::TypeAlias(a),
            AssocItem::Const(c) => GenericDef::Const(c),
        }
    }
}

pub trait HasTraitBase: Copy {
    fn get_trait_base(self, db: &dyn HirDatabase) -> Option<Self>;
}

trait AssocTraitItemExt: Sized + Copy + hir::HasName {
    fn try_from_trait_item(hir: hir::AssocItem) -> Option<Self>;
    fn container(self, db: &dyn HirDatabase) -> hir::ItemContainer;
}

impl AssocTraitItemExt for hir::AssocItem {
    fn try_from_trait_item(hir: hir::AssocItem) -> Option<Self> {
        Some(hir)
    }
    fn container(self, db: &dyn HirDatabase) -> ItemContainer {
        match self {
            hir::AssocItem::Function(f) => f.container(db),
            hir::AssocItem::Const(c) => c.container(db),
            hir::AssocItem::TypeAlias(a) => a.container(db),
        }
    }
}

impl AssocTraitItemExt for hir::Function {
    fn try_from_trait_item(hir: hir::AssocItem) -> Option<Self> {
        hir.as_function()
    }
    fn container(self, db: &dyn HirDatabase) -> ItemContainer {
        // false positive see https://github.com/rust-lang/rust-clippy/issues/12629
        #[expect(clippy::needless_borrow)]
        (&self).container(db)
    }
}

impl AssocTraitItemExt for hir::Const {
    fn try_from_trait_item(hir: hir::AssocItem) -> Option<Self> {
        hir.as_const()
    }
    fn container(self, db: &dyn HirDatabase) -> ItemContainer {
        // false positive see https://github.com/rust-lang/rust-clippy/issues/12629
        #[expect(clippy::needless_borrow)]
        (&self).container(db)
    }
}

impl AssocTraitItemExt for hir::TypeAlias {
    fn try_from_trait_item(hir: hir::AssocItem) -> Option<Self> {
        hir.as_type_alias()
    }
    fn container(self, db: &dyn HirDatabase) -> ItemContainer {
        // false positive see https://github.com/rust-lang/rust-clippy/issues/12629
        #[expect(clippy::needless_borrow)]
        (&self).container(db)
    }
}

impl<T: AssocTraitItemExt> HasTraitBase for T {
    fn get_trait_base(self, db: &dyn HirDatabase) -> Option<Self> {
        let hir::ItemContainer::Impl(impl_) = self.container(db) else {
            return None;
        };
        let trait_ = impl_.trait_(db)?;
        let item = (trait_.items(db).into_iter()).find(|x| x.name(db) == self.name(db))?;
        T::try_from_trait_item(item)
    }
}

impl HasTraitBase for hir::TypeParam {
    fn get_trait_base(self, db: &dyn HirDatabase) -> Option<Self> {
        let parent = self.parent(db).to_assoc_item()?;
        let base = parent.get_trait_base(db)?;
        let Some(&hir::GenericParam::TypeParam(t)) =
            base.to_generic_def().params0(db).get(self.param_index())
        else {
            return None;
        };
        Some(t)
    }
}
