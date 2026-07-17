use crate::internal::{ParsedProjection, parse_bounds_for};
use hir_def::TypeParamId;
use hir_ty::GenericPredicates;
use hir_ty::db::HirDatabase;
use itertools::Either;

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
}

impl TypeParamExt for hir::TypeParam {
    fn trait_bounds_with_args(
        self,
        db: &'_ dyn HirDatabase,
    ) -> Vec<(hir::Trait, Vec<hir::Type<'_>>)> {
        match self.trait_bounds_of_nested_type_with_args(&self.ty(db), db) {
            Either::Left(traits) => traits,
            Either::Right(_) => unreachable!(),
        }
    }

    fn trait_bounds_of_nested_type_with_args<'db>(
        self,
        t: &hir::Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(hir::Trait, Vec<hir::Type<'db>>)>, hir::Type<'db>> {
        match parse_bounds_for(
            GenericPredicates::query_explicit(db, TypeParamId::from(self).parent())
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
}
