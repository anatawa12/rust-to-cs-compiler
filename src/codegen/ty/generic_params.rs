use crate::codegen::CodeGenerator;
use crate::codegen::simple_extensions::*;
use crate::codegen::ty::trait_assoc_types::collect_assoc_type_params;
use crate::codegen::ty::{CsTypeOption, generic_types};
use hir::db::HirDatabase;
use ra_internal::type_alias::TypeAliasExt;
use ra_internal::*;

#[allow(clippy::enum_variant_names)]
pub enum CsTypeParamSource {
    TypeParam(usize),
    TraitStaticTypeParam(usize),
    AliasOfParam(usize, Vec<hir::TypeAlias>),
    TraitStaticAliasOfParam(usize, Vec<hir::TypeAlias>),
}

impl CsTypeParamSource {
    pub fn index(&self) -> usize {
        match *self {
            CsTypeParamSource::TypeParam(idx) => idx,
            CsTypeParamSource::TraitStaticTypeParam(idx) => idx,
            CsTypeParamSource::AliasOfParam(idx, _) => idx,
            CsTypeParamSource::TraitStaticAliasOfParam(idx, _) => idx,
        }
    }

    pub fn is_static_container(&self) -> bool {
        match *self {
            CsTypeParamSource::TypeParam(_) => false,
            CsTypeParamSource::AliasOfParam(_, _) => false,

            CsTypeParamSource::TraitStaticTypeParam(_) => true,
            CsTypeParamSource::TraitStaticAliasOfParam(_, _) => true,
        }
    }
}

impl<'db> CodeGenerator<'db> {
    pub fn generic_params_cs_sources(&self, def: hir::GenericDef) -> Vec<CsTypeParamSource> {
        let db = self.db;

        let mut type_params = Vec::new();

        if let hir::GenericDef::Trait(trait_) = def
            && self.with_self_in_cs(trait_)
        {
            type_params.insert(0, CsTypeParamSource::TypeParam(0));
        }

        for (index, param) in generic_types(&def.params0(db)).enumerate() {
            if param.is_ignored(db) {
                continue;
            }

            if !self.special_type_param(param).is_special_impl() {
                type_params.push(CsTypeParamSource::TypeParam(index));
                if (param.trait_bounds_with_args(db).iter()).any(|&(t, _)| t.needs_statics(db)) {
                    type_params.push(CsTypeParamSource::TraitStaticTypeParam(index));
                }
            }

            for instance in collect_assoc_type_params(param, param.ty(db), db) {
                let (param_instance, aliases) = instance.as_assoc_of_type_param(db).unwrap();

                assert_eq!(param_instance, param);

                type_params.push(CsTypeParamSource::AliasOfParam(index, aliases.clone()));

                if param
                    .trait_bounds_of_nested_type_with_args(&instance, db)
                    .expect_left("generic params assoc types must not be a projection")
                    .iter()
                    .any(|&(t, _)| t.needs_statics(db))
                {
                    type_params.push(CsTypeParamSource::TraitStaticAliasOfParam(
                        index,
                        aliases.clone(),
                    ));
                }
            }
        }

        if let hir::GenericDef::Trait(trait_) = def {
            for alias in trait_.assoc_types_for_cs(db) {
                type_params.push(CsTypeParamSource::AliasOfParam(0, vec![alias]));

                if alias.bounds(db).iter().any(|&(t, _)| t.needs_statics(db)) {
                    type_params.push(CsTypeParamSource::TraitStaticAliasOfParam(0, vec![alias]));
                }
            }
        }

        type_params
    }

    pub fn map_cs_type_param_source(
        &self,
        params: &[CsTypeParamSource],
        instances: &[hir::Type<'db>],
    ) -> impl Iterator<Item = String> {
        (params.iter()).map(|x| {
            self.rust_type_to_cs_options(
                &resolve_cs_type_param_source(x, instances, self.db),
                CsTypeOption::default().static_container(x.is_static_container()),
            )
        })
    }
}

pub fn resolve_cs_type_param_source<'db>(
    source: &CsTypeParamSource,
    generic_types: &[hir::Type<'db>],
    db: &'db dyn HirDatabase,
) -> hir::Type<'db> {
    match *source {
        CsTypeParamSource::TypeParam(i) | CsTypeParamSource::TraitStaticTypeParam(i) => {
            generic_types[i].clone()
        }
        CsTypeParamSource::AliasOfParam(i, ref alias)
        | CsTypeParamSource::TraitStaticAliasOfParam(i, ref alias) => {
            generic_types[i].new_associated_type(alias, db)
        }
    }
}
