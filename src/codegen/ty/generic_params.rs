use crate::codegen::CodeGenerator;
use crate::codegen::simple_extensions::{TraitExt, TypeExt};
use crate::codegen::ty::{
    CsTypeParamSource, generic_types, ignored_trait, is_omit_trait_assoc_type,
};
use hir::db::HirDatabase;
use hir::{HirDisplay, sym};
use itertools::Either;
use ra_internal::*;
use std::ops::Not;

impl<'db> CodeGenerator<'db> {
    pub fn trait_generic_params_cs_sources(&self, trait_: hir::Trait) -> Vec<CsTypeParamSource> {
        let db = self.db;

        let params = hir::GenericDef::from(trait_).params(db);
        let mut type_params = self.generic_params_cs_sources(&params);

        if self.with_self_in_cs(trait_) {
            type_params.insert(0, CsTypeParamSource::TypeParam(0));
        }

        for alias in trait_.assoc_types(db) {
            if is_omit_trait_assoc_type(db, alias) {
                continue;
            }
            type_params.push(CsTypeParamSource::AliasOfParam(0, vec![alias]));
        }

        type_params
    }

    pub fn generic_params_cs_sources(
        &self,
        params: &[hir::GenericParam],
    ) -> Vec<CsTypeParamSource> {
        self.generic_params_cs_sources_impl(params, false)
    }

    pub fn generic_params_cs_sources_impl(
        &self,
        params: &[hir::GenericParam],
        assoc_only: bool,
    ) -> Vec<CsTypeParamSource> {
        let db = self.db;

        let mut type_params = Vec::new();

        for (index, param) in generic_types(params).enumerate() {
            if param.is_implicit(db) && param.name(db) == sym::Self_ {
                continue;
            }

            if param.is_unstable(db) {
                continue;
            }

            if param
                .default(db)
                .and_then(|x| x.as_adt())
                .map(|x| x.name(db))
                .as_ref()
                .map(|x| x.as_str())
                == Some("RandomState")
            {
                continue;
            }

            if !assoc_only && !self.special_type_param(param).is_special_impl() {
                type_params.push(CsTypeParamSource::TypeParam(index));
            }

            for instance in collect_assoc_type_params(param, param.ty(db), db) {
                let (param_instance, aliases) = instance.as_assoc_of_type_param(db).unwrap();

                assert_eq!(param_instance, param);

                type_params.push(CsTypeParamSource::AliasOfParam(index, aliases.clone()));
            }
        }

        type_params
    }
}

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
        .flat_map(move |(trait_, _)| trait_.assoc_types(db))
        .flat_map(move |alias| {
            let _scope = tracing::info_span!(
                parent: &span,
                "alias",
                alias = alias.name(db).as_str(),
            )
            .entered();
            if is_omit_trait_assoc_type(db, alias) {
                return None;
            }
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
                match param.trait_bounds_of_nested_type_with_args(&instance, db) {
                    Either::Right(_projected) => {
                        panic!("Projection should be resolved by normalize_trait_assoc_type")
                    }
                    Either::Left(traits) => Some(
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
                    ),
                }
            } else {
                None
            }
        })
        .flatten()
}
