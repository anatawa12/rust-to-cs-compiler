use crate::DebugDisplay;
use crate::internal::{ParsedProjection, parse_bounds_for};
use hir_def::TypeParamId;
use hir_ty::GenericPredicates;
use hir_ty::db::HirDatabase;
use itertools::Either;

pub trait TypeParamExt {
    fn trait_bounds_with_args_self(
        self,
        db: &'_ dyn HirDatabase,
    ) -> Vec<(hir::Trait, Vec<hir::Type<'_>>)>;
    fn trait_bounds_of_nested_type_with_args_self<'db>(
        self,
        t: &hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(hir::Trait, Vec<hir::Type<'db>>)>, hir::Type<'db>>;
    fn param_index(self, db: &dyn HirDatabase) -> usize;
}

impl TypeParamExt for hir::TypeParam {
    fn trait_bounds_with_args_self(
        self,
        db: &'_ dyn HirDatabase,
    ) -> Vec<(hir::Trait, Vec<hir::Type<'_>>)> {
        match self.trait_bounds_of_nested_type_with_args_self(&self.ty(db), db) {
            Either::Left(traits) => traits,
            Either::Right(_) => unreachable!(),
        }
    }

    fn trait_bounds_of_nested_type_with_args_self<'db>(
        self,
        t: &hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(hir::Trait, Vec<hir::Type<'db>>)>, hir::Type<'db>> {
        tracing::trace!(
            bounds = ?GenericPredicates::query_all(db, TypeParamId::from(self).parent())
                .iter_identity()
                .collect::<Vec<_>>(),
            param = %self.debug_display(db),
            type = %t.debug_display(db),
            "trait_bounds_of_nested_type_with_args",
        );

        match parse_bounds_for(
            GenericPredicates::query_all(db, TypeParamId::from(self).parent())
                .iter_identity()
                .collect::<Vec<_>>(),
            t,
            db,
        ) {
            ParsedProjection::Projection(t) => Either::Right(t),
            ParsedProjection::NoBounds => Either::Left(vec![]),
            ParsedProjection::Traits(traits) => Either::Left(traits),
        }
    }

    fn param_index(self, db: &dyn HirDatabase) -> usize {
        self.parent(db).lifetime_params(db).len()
            + TypeParamId::from(self).local_id().into_raw().into_u32() as usize
    }
}
