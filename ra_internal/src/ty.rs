mod resolve_assoc;
mod type_to_string;

use super::internal::{TyExt, TyFromType};
use crate::DebugDisplay;
use crate::ty::type_to_string::TypeToString;
use hir::HasContainer;
use hir_def::{GenericParamId, HasModule, TypeAliasId};
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{
    AliasTy, Clause, ClauseKind, DbInterner, EarlyBinder, ErrorGuaranteed, GenericArg, GenericArgs,
    SolverDefId, TraitRef, Ty, TyKind,
};
use rustc_type_ir::inherent::{GenericArg as _, IntoKind};
use rustc_type_ir::{AliasTyKind, Interner, Upcast};
use std::hash::Hash;

type BoundsProvider<'db> =
    fn(&hir::Type<'db>, &'db dyn HirDatabase) -> Vec<(hir::Trait, Vec<hir::Type<'db>>)>;

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
    fn as_associated_type(
        &self,
    ) -> Option<(hir::Type<'db>, Vec<Option<hir::Type<'db>>>, hir::TypeAlias)>;

    /// Returns `self::{$alias[N]}` associated type
    fn new_associated_type(
        &self,
        aliases: &[hir::TypeAlias],
        db: &'db dyn HirDatabase,
        bounds_provider: BoundsProvider<'db>,
    ) -> hir::Type<'db>;

    /// Returns if this type is a array type. This doesn't require the length to be known unlike [`Type::as_array`]
    fn as_array_unsized(&self, db: &'db dyn HirDatabase) -> Option<hir::Type<'db>>;

    fn instantiate(
        &self,
        def: hir::GenericDef,
        args: &[hir::Type<'db>],
        db: &'db dyn HirDatabase,
    ) -> hir::Type<'db>;

    /// In some functions like [hir::Function::ret_type_with_args], type env krate
    /// can be incorrect and fails to [hir::Type::normalize_trait_assoc_type]. This can fix by replacing krate part
    fn with_crate(&self, krate: hir::Crate, db: &'db dyn HirDatabase) -> hir::Type<'db>;

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

    fn as_associated_type(
        &self,
    ) -> Option<(hir::Type<'db>, Vec<Option<hir::Type<'db>>>, hir::TypeAlias)> {
        self.ns_ty()
            .as_associated_type()
            .map(|(ty, args, alias_id)| {
                (
                    self.derived(ty),
                    args.into_iter()
                        .map(|x| x.ty().map(|ty| self.derived(ty)))
                        .collect::<Vec<_>>(),
                    hir::TypeAlias::from(alias_id),
                )
            })
    }

    fn new_associated_type(
        &self,
        aliases: &[hir::TypeAlias],
        db: &'db dyn HirDatabase,
        bounds_provider: BoundsProvider<'db>,
    ) -> hir::Type<'db> {
        let interner = DbInterner::new_with(db, self.env().krate);
        let self_ty = self.ns_ty();
        let env = self.env();
        let mut clauses = env.param_env.clauses().to_vec();
        let mut ty = self_ty;
        for &alias in aliases {
            let alias_id = TypeAliasId::from(alias);
            let hir::ItemContainer::Trait(container) = alias.container(db) else {
                unreachable!();
            };
            if let traits = bounds_provider(&self.derived(ty), db)
                .into_iter()
                .filter(|&(trait_, _)| trait_ == container)
                .collect::<Vec<_>>()
                && !traits.is_empty()
            {
                assert!(traits.len() == 1);
                let (_trait, trait_args) = { traits }.swap_remove(0);
                let mut parsed_iter = trait_args.iter().map(|x| x.ns_ty());
                let generic_args = GenericArgs::for_item(interner, alias_id.into(), |_, y, _| {
                    if let GenericParamId::TypeParamId(_) = y {
                        if let Some(ty) = parsed_iter.next() {
                            ty.into()
                        } else {
                            GenericArg::error_from_id(interner, y)
                        }
                    } else {
                        GenericArg::error_from_id(interner, y)
                    }
                });
                ty = Ty::new(
                    interner,
                    TyKind::Alias(AliasTy::new_from_args(
                        interner,
                        AliasTyKind::Projection {
                            def_id: SolverDefId::TypeAliasId(alias_id),
                        },
                        generic_args,
                    )),
                );
                // We need to find bounds from impl
                clauses.extend(
                    interner
                        .item_self_bounds(alias_id.into())
                        .iter_instantiated(interner, generic_args),
                );
            } else {
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
                )
            }
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

    fn with_crate(&self, krate: hir::Crate, db: &'db dyn HirDatabase) -> Self {
        let mut env = self.env();
        let ty = self.ns_ty();
        if env.krate.transitive_rev_deps(db).contains(&krate.base()) {
            env.krate = krate.base();
        }
        hir::Type::from_ty_env(ty, env)
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

#[repr(transparent)]
pub struct TyEq<'db>(hir::Type<'db>);

impl PartialEq for TyEq<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.ns_ty() == other.0.ns_ty()
    }
}

impl Eq for TyEq<'_> {}

impl Hash for TyEq<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.ns_ty().hash(state);
    }
}

mod ty_eq {
    use crate::TyEq;
    use std::slice;

    pub trait WrapWithTyEq {
        type Wrapped;
        fn wrap(self) -> Self::Wrapped;
    }

    macro_rules! impl_wrap {
        ($(for<$($l: lifetime),+>)? |$self: ident: $self_ty: ty| -> $wrapped_ty: ty $out: block) => {
            impl$(<$($l),+>)? WrapWithTyEq for $self_ty {
                type Wrapped = $wrapped_ty;
                fn wrap($self: $self_ty) -> Self::Wrapped {
                    $out
                }
            }
        };
    }

    impl_wrap!(for<'db> |self: hir::Type<'db>| -> TyEq<'db> {
        #[expect(clippy::init_numbered_fields)] // I don't know why but TyEq(self) is not accepted
        TyEq { 0: self }
    });
    impl_wrap!(for<'db> |self: *const hir::Type<'db>| -> *const TyEq<'db> {
        self as *const TyEq<'db>
    });
    impl_wrap!(for<'db> |self: *mut hir::Type<'db>| -> *mut TyEq<'db> { self as *mut TyEq<'db> });
    impl_wrap!(for<'db> |self: Vec<hir::Type<'db>>| -> Vec<TyEq<'db>> {
        let (ptr, len, cap) = self.into_raw_parts();
        unsafe { Vec::from_raw_parts(TyEq::wrap(ptr), len, cap) }
    });
    impl_wrap!(
        for<'db, 'a> |self: &'a Vec<hir::Type<'db>>| -> &'a [TyEq<'db>] {
            TyEq::wrap(self.as_slice())
        }
    );
    impl_wrap!(
        for<'db, 'a> |self: &'a [hir::Type<'db>]| -> &'a [TyEq<'db>] {
            let (ptr, len) = (self.as_ptr(), self.len());
            unsafe { slice::from_raw_parts(TyEq::wrap(ptr), len) }
        }
    );
    impl_wrap!(
        for<'db, 'a> |self: &'a mut [hir::Type<'db>]| -> &'a mut [TyEq<'db>] {
            let (ptr, len) = (self.as_mut_ptr(), self.len());
            unsafe { slice::from_raw_parts_mut(TyEq::wrap(ptr), len) }
        }
    );
}

impl<'db> TyEq<'db> {
    pub fn wrap<P: ty_eq::WrapWithTyEq>(ty: P) -> P::Wrapped {
        ty.wrap()
    }
}
