//! Extensions for the `hir` crate types without any non-hir crate types

use crate::codegen::ty::{generic_types, is_omit_trait_assoc_type};
use hir::db::HirDatabase;
use hir::{
    Adt, AssocItem, AssocItemContainer, GenericDef, GenericParam, HasContainer, HasCrate,
    ItemContainer, Trait, Type, TypeParam, sym,
};
use ide_db::base_db::salsa_macros;
use itertools::Either;
use ra_internal::adt::AdtExt;
use ra_internal::function::FunctionExt;
use ra_internal::generic_def::GenericDefExt as _;
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
        self.items_with_supertraits(db)
            .iter()
            .any(|item| match *item {
                hir::AssocItem::Function(f) => {
                    !f.has_self_param(db) && !f.is_explicit_sized_self(db)
                }
                hir::AssocItem::Const(_) => false,
                hir::AssocItem::TypeAlias(_) => false,
            })
    }
}

pub trait TypeExt<'db> {
    #[allow(dead_code)]
    fn expect_adt_with_args(&self) -> (Adt, Vec<Option<Type<'db>>>);
    fn expect_adt_of(&self, adt: Adt) -> Vec<Type<'db>>;

    /// Returns Some if this type is `<SomeParam as Trait>::AssociatedType` or nested it
    #[allow(clippy::type_complexity)]
    fn as_assoc_of_type_param(
        &self,
        db: &dyn HirDatabase,
    ) -> Option<(
        hir::TypeParam,
        Vec<(hir::TypeAlias, Vec<Option<hir::Type<'db>>>)>,
    )>;
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
    ) -> Option<(
        hir::TypeParam,
        Vec<(hir::TypeAlias, Vec<Option<hir::Type<'db>>>)>,
    )> {
        let mut cur = self.clone();
        let mut aliases = vec![];

        while let Some((self_ty, generics, alias)) = cur.as_associated_type() {
            aliases.push((alias, generics));
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
        self.trait_bounds_of_nested_type_with_args(&self.ty(db), db)
            .expect_left("type param itself must not become projection")
    }
    #[tracing::instrument(skip_all, fields(t = %t.debug_display(db)))]
    fn trait_bounds_of_nested_type_with_args<'db>(
        self,
        t: &hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(hir::Trait, Vec<hir::Type<'db>>)>, hir::Type<'db>> {
        match self.trait_bounds_of_nested_type_with_args_self(t, db) {
            Either::Left(mut traits) => {
                if let Some(base) = self.get_trait_base(db) {
                    // get_trait_base returns some => the type param is of associated item of impl implements trait
                    let self_def = self.parent(db);
                    let AssocItemContainer::Impl(impl_) =
                        self_def.to_assoc_item().unwrap().container(db)
                    else {
                        unreachable!();
                    };
                    let trait_ref = impl_.trait_ref(db).unwrap();

                    // Back-instantiate the T to the generic params of the item in trait.
                    // The t is expected to be associated type of self (generic of impl item) so
                    // making error types for impl generics is valid
                    let t_as_trait = t.instantiate(
                        self_def,
                        &generic_types(&GenericDef::from(impl_).params0(db))
                            .map(|_| Type::error(db, impl_.krate(db)))
                            .chain(
                                generic_types(&base.parent(db).params0(db))
                                    .map(|param| param.ty(db)),
                            )
                            .collect::<Vec<_>>(),
                        db,
                    );

                    match base.trait_bounds_of_nested_type_with_args_self(&t_as_trait, db) {
                        Either::Left(mut traits_base) => {
                            if !traits_base.is_empty() {
                                let generic_args =
                                    self_def.type_args_maps_trait_to_this_impl(db, &trait_ref);

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

                // In cae of ADTs, there are some cases all impls (except for few 'wrapper' impls)
                // use same bounds for the type. We add them
                if let GenericDef::Adt(adt) = self.parent(db) {
                    let _scop = tracing::info_span!("adt", adt = %adt.debug_display(db)).entered();
                    let self_def = self.parent(db);

                    let mut impl_bounds = vec![];
                    let adt_as_def = self.parent(db);
                    let mut bounds_set = None;

                    let lang_items = LangItems::new(db, adt.krate(db));
                    for &impl_ in adt.impls(db) {
                        if let Some((impl_adt, impl_self_args)) =
                            impl_.self_ty(db).as_adt_with_args()
                            && !impl_.is_builtin_derive()
                            && impl_adt == adt
                            && impl_.trait_(db).is_none_or(|trait_| {
                                Some(trait_) != lang_items.Debug()
                                    && Some(trait_) != lang_items.Clone()
                                    && Some(trait_) != lang_items.Default()
                            })
                            && let impl_as_def = GenericDef::from(impl_)
                            && adt_as_def.params0(db).len() == impl_self_args.len()
                            && let Some(impl_args_maps_adt) =
                                adt.map_generics_to_impl_generics(impl_, &impl_self_args, db)
                            && let Some(type_param) = impl_self_args[self.param_index(db)]
                                .as_ref()
                                .unwrap()
                                .as_type_param(db)
                        {
                            let t_as_impl = t.instantiate(
                                self_def,
                                &impl_self_args.into_iter().flatten().collect::<Vec<_>>(),
                                db,
                            );

                            match type_param.trait_bounds_of_nested_type_with_args(&t_as_impl, db) {
                                Either::Right(projection) => {
                                    impl_bounds.push(Either::Right(projection));
                                }
                                Either::Left(mut this_impl_bounds) => {
                                    if !this_impl_bounds.is_empty() {
                                        // impl types.
                                        let impl_args_maps_adt = impl_args_maps_adt
                                            .into_iter()
                                            .flatten()
                                            .collect::<Vec<_>>();
                                        for (_, args) in &mut this_impl_bounds {
                                            for ty in args {
                                                *ty = ty.instantiate(
                                                    impl_as_def,
                                                    &impl_args_maps_adt,
                                                    db,
                                                );
                                            }
                                        }
                                    }
                                    let bounds_set = bounds_set.get_or_insert_with(|| {
                                        traits
                                            .iter()
                                            .map(|&(t, ref types)| (t, TyEq::wrap(types)))
                                            .collect::<HashSet<_>>()
                                    });
                                    this_impl_bounds.retain(|&(t, ref types)| {
                                        !bounds_set.contains(&(t, TyEq::wrap(types)))
                                    });
                                    if !this_impl_bounds.is_empty()
                                        && !(this_impl_bounds.iter())
                                            .all(|&(t, _)| Some(t) == lang_items.Default())
                                    {
                                        impl_bounds.push(Either::Left(this_impl_bounds));
                                    }
                                }
                            }
                        }
                    }
                    if !impl_bounds.is_empty() && impl_bounds.iter().all(|x| x.is_left()) {
                        let first = (impl_bounds[0].as_ref().unwrap_left().iter())
                            .map(|&(t, ref types)| (t, TyEq::wrap(types)))
                            .collect::<HashSet<_>>();
                        if impl_bounds.iter().skip(1).all(|bound| {
                            let as_set = (bound.as_ref().unwrap_left().iter())
                                .map(|&(t, ref types)| (t, TyEq::wrap(types)))
                                .collect::<HashSet<_>>();
                            as_set == first
                        }) {
                            traits.extend(impl_bounds.swap_remove(0).unwrap_left())
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

    /// Only valid if this is associated item of an impl of a trait.
    fn type_args_maps_trait_to_this_impl<'db>(
        &self,
        db: &'db dyn HirDatabase,
        trait_ref: &hir::TraitRef<'db>,
    ) -> Vec<hir::Type<'db>>;
}

impl GenericDefExt for GenericDef {
    fn params0(self, db: &dyn HirDatabase) -> Vec<hir::GenericParam> {
        if let GenericDef::Function(f) = self
            && let ItemContainer::Impl(impl_) = f.container(db)
            && impl_.is_builtin_derive()
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

    fn type_args_maps_trait_to_this_impl<'db>(
        &self,
        db: &'db dyn HirDatabase,
        trait_ref: &hir::TraitRef<'db>,
    ) -> Vec<hir::Type<'db>> {
        (trait_ref.generic_types(db).flatten())
            .chain(generic_types(&self.params0(db)).map(|param| param.ty(db)))
            .collect::<Vec<_>>()
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
            base.to_generic_def().params0(db).get(self.param_index(db))
        else {
            return None;
        };
        Some(t)
    }
}
