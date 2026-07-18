use crate::codegen::simple_extensions::*;
use crate::codegen::ty::ignored_trait;
use hir::HirDisplay;
use hir::db::HirDatabase;
use ra_internal::{DebugDisplay, TypeExt};
use std::ops::Not;

pub fn collect_assoc_type_params<'db>(
    param: hir::TypeParam,
    type_: hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> impl Iterator<Item = hir::Type<'db>> + 'db {
    let traits = param.trait_bounds_with_args(db);
    collect_assoc_type_params_impl(param, traits, type_, db)
}

fn collect_assoc_type_params_impl<'db>(
    param: hir::TypeParam,
    traits: Vec<(hir::Trait, Vec<hir::Type<'db>>)>,
    outer_instance: hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> impl Iterator<Item = hir::Type<'db>> + 'db {
    let span = tracing::info_span!(
        "collect_assoc_type_params_impl",
        assoc_ty = %outer_instance.debug_display(db),
    );
    traits
        .into_iter()
        .filter(|&(trait_, _)| !ignored_trait(trait_, db))
        .flat_map(move |(trait_, _)| trait_.assoc_types_for_cs(db))
        .flat_map(move |alias| {
            let _scope = tracing::info_span!(
                parent: &span,
                "alias",
                alias = alias.name(db).as_str(),
            )
            .entered();
            let Some(instance) = outer_instance.normalize_trait_assoc_type(db, &[], alias) else {
                let target = alias.module(db).krate(db).to_display_target(db);
                panic!("unable to resolve {alias:?} of {type}",
                       alias = alias.name(db).as_str(),
                       type = outer_instance.display(db, target),
                );
            };
            if let Some((param_instance, alias_instance)) = instance.as_associated_type()
                && param_instance == outer_instance
                && alias_instance == alias
            {
                let traits = param
                    .trait_bounds_of_nested_type_with_args(&instance, db)
                    .expect_left("generic params assoc types must not be a projection");
                Some(
                    std::iter::once(instance.clone()).chain(
                        traits
                            .is_empty()
                            .not()
                            .then(|| {
                                Box::new(collect_assoc_type_params_impl(
                                    param, traits, instance, db,
                                ))
                                    as Box<dyn Iterator<Item = hir::Type<'db>> + 'db>
                            })
                            .into_iter()
                            .flatten(),
                    ),
                )
            } else {
                None
            }
        })
        .flatten()
}
