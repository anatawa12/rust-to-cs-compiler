mod resolve_assoc;
mod type_to_string;

use super::internal::{TyExt, TyFromType};
use crate::DebugDisplay;
use crate::ty::type_to_string::TypeToString;
use hir::HasContainer;
use hir_def::HasModule;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{
    AliasTy, Clause, ClauseKind, DbInterner, EarlyBinder, ErrorGuaranteed, GenericArg, GenericArgs,
    SolverDefId, TraitRef, Ty, TyKind,
};
use rustc_type_ir::inherent::{GenericArg as _, IntoKind};
use rustc_type_ir::{AliasTyKind, Upcast};

pub trait TypeExt<'db> {
    fn error(db: &'db dyn HirDatabase, krate: hir::Crate) -> Self;
    fn is_error(&self) -> bool;
    fn as_impl_traits_with_params(
        &self,
        db: &'db dyn HirDatabase,
    ) -> Option<Vec<(hir::Trait, Vec<Option<hir::Type<'db>>>)>>;

    /// This tries to resolve `<impl SomeTrait<Assoc = SomeType> as SomeTrait>::Assoc` to a simpler type
    fn resolve_associated_type(&self, db: &'db dyn HirDatabase) -> hir::Type<'db>;

    /// Returns Some if the type is `<SomeT as SomeTrait>::AssociatedType`
    fn as_associated_type(&self) -> Option<(hir::Type<'db>, hir::TypeAlias)>;

    /// Returns `self::{$alias[N]}` associated type
    fn new_associated_type(
        &self,
        aliases: &[hir::TypeAlias],
        db: &'db dyn HirDatabase,
    ) -> hir::Type<'db>;

    /// Returns if this type is a array type. This doesn't require the length to be known unlike [`Type::as_array`]
    fn as_array_unsized(&self, db: &'db dyn HirDatabase) -> Option<hir::Type<'db>>;

    fn instantiate(
        &self,
        def: hir::GenericDef,
        args: &[hir::Type<'db>],
        db: &'db dyn HirDatabase,
    ) -> hir::Type<'db>;

    // you should add members above this line. below this line is the debug utility
    /// Debug display the type with much information as possible
    fn ty_string(&self, db: &'db dyn HirDatabase) -> impl std::fmt::Debug;
}

impl<'db> TypeExt<'db> for hir::Type<'db> {
    fn error(db: &'db dyn HirDatabase, krate: hir::Crate) -> Self {
        Self::from_ty_env(
            Ty::new(DbInterner::new_no_crate(db), TyKind::Error(ErrorGuaranteed)),
            crate::internal::empty_param_env(krate.base()),
        )
    }

    fn is_error(&self) -> bool {
        matches!(self.ns_ty().kind(), TyKind::Error(_))
    }

    fn as_impl_traits_with_params(
        &self,
        db: &'db dyn HirDatabase,
    ) -> Option<Vec<(hir::Trait, Vec<Option<hir::Type<'db>>>)>> {
        impl_trait_bounds(self.ns_ty(), db).map(|it| {
            it.into_iter()
                .filter_map(|pred| match pred.kind().skip_binder() {
                    ClauseKind::Trait(trait_ref) => {
                        let trait_ref = trait_ref.trait_ref;

                        let types = trait_ref
                            .args
                            .as_slice()
                            .iter()
                            ./*flat_*/map(|arg| Some(self.derived(arg.as_type()?)))
                            .collect::<Vec<_>>();
                        Some((hir::Trait::from(trait_ref.def_id.0), types))
                    }
                    _ => None,
                })
                .collect()
        })
    }

    fn resolve_associated_type(&self, db: &'db dyn HirDatabase) -> hir::Type<'db> {
        resolve_assoc::resolve_associated_type(self, db)
    }

    fn as_associated_type(&self) -> Option<(hir::Type<'db>, hir::TypeAlias)> {
        self.ns_ty()
            .as_associated_type()
            .map(|(ty, _, alias_id)| (self.derived(ty), hir::TypeAlias::from(alias_id)))
    }

    fn new_associated_type(
        &self,
        aliases: &[hir::TypeAlias],
        db: &'db dyn HirDatabase,
    ) -> hir::Type<'db> {
        let interner = DbInterner::new_with(db, self.env().krate);
        let self_ty = self.ns_ty();
        let mut ty = self_ty;
        for &alias in aliases {
            let alias_id = alias.into();
            ty = Ty::new(
                interner,
                TyKind::Alias(
                    AliasTy::new_from_args(
                        interner,
                        AliasTyKind::Projection {
                            def_id: SolverDefId::TypeAliasId(alias_id),
                        },
                        GenericArgs::error_for_item(interner, alias_id.into()),
                    )
                    .with_replaced_self_ty(interner, ty),
                ),
            );
        }
        self.derived(ty)
    }

    fn as_array_unsized(&self, _db: &'db dyn HirDatabase) -> Option<hir::Type<'db>> {
        if let TyKind::Array(inner, _size) = self.ns_ty().kind() {
            Some(self.derived(inner))
        } else {
            None
        }
    }

    fn instantiate(
        &self,
        def: hir::GenericDef,
        args: &[hir::Type<'db>],
        db: &'db dyn HirDatabase,
    ) -> hir::Type<'db> {
        let parent_def = match match def {
            hir::GenericDef::Function(f) => Some(f.container(db)),
            hir::GenericDef::Adt(_) => None,
            hir::GenericDef::Trait(_) => None,
            hir::GenericDef::TypeAlias(a) => Some(a.container(db)),
            hir::GenericDef::Impl(_) => None,
            hir::GenericDef::Const(c) => Some(c.container(db)),
            hir::GenericDef::Static(s) => Some(s.container(db)),
        } {
            Some(hir::ItemContainer::Trait(trait_)) => Some(hir::GenericDef::Trait(trait_)),
            Some(hir::ItemContainer::Impl(impl_)) => Some(hir::GenericDef::Impl(impl_)),
            Some(hir::ItemContainer::Module(_)) => None,
            Some(hir::ItemContainer::ExternBlock(_)) => None,
            Some(hir::ItemContainer::Crate(_)) => None,
            None => None,
        };
        let params = parent_def
            .map(|def| def.params(db))
            .unwrap_or_default()
            .into_iter()
            .chain(def.params(db))
            .collect::<Vec<_>>();
        let mut def_args = Vec::with_capacity(params.len());
        let mut type_iter = args.iter();
        let interner = DbInterner::new_no_crate(db);

        if matches!(def, hir::GenericDef::Trait(_))
            || matches!(parent_def, Some(hir::GenericDef::Trait(_)))
        {
            def_args.push(GenericArg::from(type_iter.next().unwrap().ns_ty()))
        }

        for &x in &params {
            match x {
                hir::GenericParam::TypeParam(_) => def_args.push(GenericArg::from(
                    type_iter
                        .next()
                        .map(|x| x.ns_ty())
                        .unwrap_or_else(|| hir::Type::error(db, def.module(db).krate(db)).ns_ty()),
                )),
                hir::GenericParam::ConstParam(_) => {
                    def_args.push(hir_ty::next_solver::Const::error(interner).into())
                }
                hir::GenericParam::LifetimeParam(_) => {
                    def_args.push(hir_ty::next_solver::Region::error(interner).into())
                }
            }
        }
        tracing::trace!(
            "instantiate: {} with {:?} ({params:?}) based on {def:?}",
            self.debug_display(db),
            args.iter()
                .map(|x| std::fmt::from_fn(move |f| std::fmt::Display::fmt(
                    &x.debug_display(db),
                    f
                )))
                .collect::<Vec<_>>()
        );

        if args.is_empty() {
            self.clone()
        } else {
            self.derived(EarlyBinder::bind(self.ns_ty()).instantiate(interner, def_args.as_slice()))
        }
    }

    fn ty_string(&self, db: &'db dyn HirDatabase) -> impl std::fmt::Debug {
        std::fmt::from_fn(move |f| {
            std::fmt::Debug::fmt(
                &TypeToString {
                    db,
                    interner: DbInterner::new_with(db, self.env().krate),
                }
                .ty_to_str(self.ns_ty()),
                f,
            )
        })
    }
}

fn impl_trait_bounds<'db>(ty: Ty<'db>, db: &'db dyn HirDatabase) -> Option<Vec<Clause<'db>>> {
    if let TyKind::Coroutine(coroutine_id, _args) = ty.kind() {
        // impl_trait_bounds returns TraitRef without self type specified
        let interner = DbInterner::new_no_crate(db);

        let owner = coroutine_id.0.loc(db).owner;
        let krate = owner.krate(db);
        if let Some(future_trait) = hir_def::lang_item::lang_items(db, krate).Future {
            // This is only used by type walking.
            // Parameters will be walked outside, and projection predicate is not used.
            // So just provide the Future trait.
            let impl_bound = TraitRef::new_from_args(
                interner,
                future_trait.into(),
                GenericArgs::new_from_slice(&[GenericArg::from(ty)]),
            )
            .upcast(interner);
            Some(vec![impl_bound])
        } else {
            None
        }
    } else {
        ty.impl_trait_bounds(db)
    }
}
