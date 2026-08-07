use crate::DebugDisplay;
use crate::internal::TyFromType;
use hir::HasContainer;
use hir_def::GenericDefId;
use hir_ty::GenericPredicates;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{
    AliasTy, ClauseKind, DbInterner, TermId, TraitAssocTermId, TraitAssocTyId, Ty, TyKind,
};
use rustc_type_ir::inherent::IntoKind;
use rustc_type_ir::{AliasTyKind, PredicatePolarity};

pub trait GenericDefExt: Copy {
    /// Back resolves A::B from C when there is A::B == C (A: Trait<B = C>)
    fn back_resolve_projection<'db>(
        self,
        ty: hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Vec<hir::Type<'db>>;
}

impl GenericDefExt for hir::GenericDef {
    fn back_resolve_projection<'db>(
        self,
        ty: hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Vec<hir::Type<'db>> {
        let Ok(generic_def_id) = self.try_into() else {
            return vec![];
        };
        let clauses = GenericPredicates::query_all(db, generic_def_id).skip_binder();
        let interner = DbInterner::new_with(db, ty.env(db).krate);
        clauses
            .filter_map(|clause| {
                let ClauseKind::Projection(projection) = clause.kind().skip_binder() else {
                    return None;
                };
                (projection.term.expect_type() == ty.ns_ty().skip_binder()).then_some(projection)
            })
            .map(|x| {
                let self_ty = x.self_ty();
                let TraitAssocTermId(TermId::TypeAliasId(alias_id)) = x.def_id() else {
                    unreachable!();
                };
                let hir::ItemContainer::Trait(trait_) =
                    hir::TypeAlias::from(alias_id).container(db)
                else {
                    unreachable!();
                };
                let trait_id = trait_.into();

                let trait_clause = GenericPredicates::query_all(db, generic_def_id)
                    .skip_binder()
                    .filter_map(|clause| {
                        let ClauseKind::Trait(trait_pred) = clause.kind().skip_binder() else {
                            return None;
                        };
                        Some(trait_pred.trait_ref)
                    })
                    .filter(|trait_ref| trait_ref.self_ty() == self_ty)
                    .flat_map(|trait_ref| {
                        [trait_ref].into_iter().chain(GenericPredicates::query_all(db, GenericDefId::TraitId(trait_ref.def_id.0))
                            .iter_instantiated(interner, trait_ref.args)
                            .filter_map(move |clause| match clause.kind().skip_binder() {
                                ClauseKind::Trait(trait_)
                                if trait_.self_ty() == trait_ref.self_ty()
                                    && trait_.polarity == PredicatePolarity::Positive => Some(trait_.trait_ref),
                                _ => None
                            }))
                    })
                    .find(|trait_ref| trait_ref.def_id.0 == trait_id)
                    .unwrap_or_else(|| {
                        tracing::error!(
                            "Project clause found but no trait clause for the projected type found!\ntrait = {trait:?}\nalias={alias:?}\nclauses = {clauses:?}",
                            trait = trait_.debug_display(db),
                            alias = hir::TypeAlias::from(alias_id).debug_display(db),
                            clauses = GenericPredicates::query_all(db, generic_def_id).skip_binder().collect::<Vec<_>>(),
                        );
                        panic!()
                    });

                Ty::new(
                    interner,
                    TyKind::Alias(
                        AliasTy::new_from_args(
                            interner,
                            AliasTyKind::Projection {
                                def_id: TraitAssocTyId(alias_id),
                            },
                            trait_clause.args,
                        )
                            .with_replaced_self_ty(interner, self_ty),
                    ),
                )
            })
            .map(|x| ty.derived(x))
            .collect()
    }
}
