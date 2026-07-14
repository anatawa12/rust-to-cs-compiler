use crate::internal::TyFromType;
use hir_def::GenericDefId;
use hir_ty::ParamEnvAndCrate;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{DbInterner, GenericArgs};
use rustc_type_ir::inherent::IntoKind;

pub trait TraitExt {
    fn predicate_types<'db>(self, db: &'db dyn HirDatabase)
    -> impl Iterator<Item = hir::Type<'db>>;
}

impl TraitExt for hir::Trait {
    fn predicate_types<'db>(
        self,
        db: &'db dyn HirDatabase,
    ) -> impl Iterator<Item = hir::Type<'db>> {
        let generic_def_id = GenericDefId::try_from(hir::GenericDef::from(self)).unwrap();

        let env = ParamEnvAndCrate {
            param_env: db.trait_environment(generic_def_id.into()),
            krate: self.module(db).krate(db).base(),
        };
        let interner = DbInterner::new_with(db, env.krate);

        hir_ty::GenericPredicates::query_explicit(db, generic_def_id)
            .iter_identity()
            .map(move |predicate| match predicate.kind().skip_binder() {
                rustc_type_ir::ClauseKind::Trait(trait_pred) => trait_pred.trait_ref.args,
                rustc_type_ir::ClauseKind::Projection(proj_pred) => proj_pred.projection_term.args,
                _ => GenericArgs::empty(interner),
            })
            .flat_map(move |args| {
                args.iter()
                    .skip(1)
                    .filter_map(|x| {
                        variant_or_none!(x.kind(), hir_ty::next_solver::GenericArgKind::Type)
                    })
                    .map(move |ty| hir::Type::from_ty_env(ty, env))
            })
    }
}
