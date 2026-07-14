mod resolve_assoc;
mod type_to_string;

use super::internal::{TyExt, TyFromType};
use crate::ty::type_to_string::TypeToString;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{
    AliasTy, ClauseKind, DbInterner, ErrorGuaranteed, GenericArgs, SolverDefId, Ty, TyKind,
};
use rustc_type_ir::AliasTyKind;
use rustc_type_ir::inherent::IntoKind;

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
        self.ns_ty().impl_trait_bounds(db).map(|it| {
            it.into_iter()
                .filter_map(|pred| match pred.kind().skip_binder() {
                    ClauseKind::Trait(trait_ref) => Some((
                        hir::Trait::from(trait_ref.def_id().0),
                        trait_ref
                            .trait_ref
                            .args
                            .iter()
                            .map(|arg| Some(self.derived(arg.ty()?)))
                            .collect(),
                    )),
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
