use crate::codegen::CodeGenerator;
use crate::codegen::simple_extensions::{TraitExt, TypeExt, TypeParamExt};
use crate::codegen::ty::generic_types;
use crate::codegen::ty::trait_assoc_types::collect_assoc_type_params;
use hir::db::HirDatabase;
use ra_internal::*;

pub enum CsTypeParamSource {
    TypeParam(usize),
    AliasOfParam(usize, Vec<hir::TypeAlias>),
}

impl CsTypeParamSource {
    pub fn index(&self) -> usize {
        match *self {
            CsTypeParamSource::TypeParam(idx) => idx,
            CsTypeParamSource::AliasOfParam(idx, _) => idx,
        }
    }
}

impl<'db> CodeGenerator<'db> {
    pub fn trait_generic_params_cs_sources(&self, trait_: hir::Trait) -> Vec<CsTypeParamSource> {
        let db = self.db;

        let params = hir::GenericDef::from(trait_).params(db);
        let mut type_params = self.generic_params_cs_sources(&params);

        if self.with_self_in_cs(trait_) {
            type_params.insert(0, CsTypeParamSource::TypeParam(0));
        }

        for alias in trait_.assoc_types_for_cs(db) {
            type_params.push(CsTypeParamSource::AliasOfParam(0, vec![alias]));
        }

        type_params
    }

    pub fn generic_params_cs_sources(
        &self,
        params: &[hir::GenericParam],
    ) -> Vec<CsTypeParamSource> {
        let db = self.db;

        let mut type_params = Vec::new();

        for (index, param) in generic_types(params).enumerate() {
            if param.is_ignored(db) {
                continue;
            }

            if !self.special_type_param(param).is_special_impl() {
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

pub fn resolve_cs_type_param_source<'db>(
    source: &CsTypeParamSource,
    generic_types: &[hir::Type<'db>],
    db: &'db dyn HirDatabase,
) -> hir::Type<'db> {
    match *source {
        CsTypeParamSource::TypeParam(i) => generic_types[i].clone(),
        CsTypeParamSource::AliasOfParam(i, ref alias) => {
            generic_types[i].new_associated_type(alias, db)
        }
    }
}

pub fn map_type_param_source<'db>(
    params: &[CsTypeParamSource],
    instances: &[hir::Type<'db>],
    db: &'db dyn HirDatabase,
) -> impl Iterator<Item = hir::Type<'db>> {
    (params.iter()).map(|x| resolve_cs_type_param_source(x, instances, db))
}
