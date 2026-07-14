use crate::codegen::CodeGenerator;
use crate::codegen::ty::{collect_assoc_type_params, includes_type_in_type};
use hir::db::HirDatabase;
use itertools::Either;
use ra_internal::*;
use std::ops::Not;
use tracing::trace;

pub use hir::MethodViolationCode;
use hir::sym;

impl<'db> CodeGenerator<'db> {
    pub fn with_self_in_cs(&self, trait_: hir::Trait) -> bool {
        if let Some(violations) = self.dyn_compatibility_all_violations_alt(trait_)
            && !violations.iter().all(|v| self.allowed_violation(v))
        {
            return true;
        }

        false
    }

    fn allowed_violation(&self, v: &DynCompatibilityViolation) -> bool {
        match v {
            DynCompatibilityViolation::SizedSelf => true, // interfaces are sized
            DynCompatibilityViolation::SelfReferential => false,
            DynCompatibilityViolation::Method(_f, v) => match v {
                MethodViolationCode::Generic => true, // Generic interface method is native in C#
                MethodViolationCode::ReferencesImplTraitInTrait => true, // return place impl trait
                MethodViolationCode::AsyncFn => true, // async fns are return place impl trait

                MethodViolationCode::StaticMethod => false, // TODO: Static method helper system
                MethodViolationCode::ReferencesSelfInput => false,
                MethodViolationCode::ReferencesSelfOutput => false,
                MethodViolationCode::WhereClauseReferencesSelf => false,
                MethodViolationCode::UndispatchableReceiver => false,
            },
            DynCompatibilityViolation::AssocConst(_) => false,
            //DynCompatibilityViolation::GAT(_) => false,
            //DynCompatibilityViolation::HasNonCompatibleSuperTrait(_) => false,
        }
    }

    fn dyn_compatibility_all_violations_alt(
        &self,
        trait_: hir::Trait,
    ) -> Option<Vec<DynCompatibilityViolation>> {
        let _scope = tracing::info_span!(
            "dyn_compatibility_all_violations_alt",
            trait = %trait_.debug_display(self.db),
        )
        .entered();
        let db = self.db;
        let mut violations: Vec<DynCompatibilityViolation> = vec![];
        let mut cb = |violation: DynCompatibilityViolation| violations.push(violation);

        if trait_
            .all_supertraits(db)
            .iter()
            .any(|&x| Some(x) == self.lang_items.Sized())
        {
            cb(DynCompatibilityViolation::SizedSelf);
        }

        if predicates_reference_self(self, self.db, trait_) {
            cb(DynCompatibilityViolation::SelfReferential);
        }

        // TODO: check for SelfReferential

        for assoc_item in trait_.items_with_supertraits(db) {
            // remove associated items with Self: Sized, but does not exclude Trait : Sized
            if generics_require_sized_self(assoc_item, db) {
                trace!(
                    "ignored item {assoc_item} of {trait} since guarded by Sized",
                    assoc_item = assoc_item.name(db).unwrap().as_str(),
                    trait = trait_.debug_display(self.db),
                );
                continue;
            }

            match assoc_item {
                hir::AssocItem::Const(it) => cb(DynCompatibilityViolation::AssocConst(it)),
                hir::AssocItem::Function(it) => {
                    let _scope = tracing::info_span!(
                        "dyn_compatibility_all_violations_alt for fn",
                        trait = %trait_.debug_display(self.db),
                        f = %it.debug_display(self.db),
                    )
                    .entered();
                    self.virtual_call_violations_for_method(it, &mut |mvc| {
                        cb(DynCompatibilityViolation::Method(it, mvc))
                    })
                }
                hir::AssocItem::TypeAlias(_it) => {}
            }
        }

        fn predicates_reference_self<'db>(
            this: &CodeGenerator<'db>,
            db: &'db dyn HirDatabase,
            trait_: hir::Trait,
        ) -> bool {
            trait_.predicate_types(db).any(|type_| {
                includes_type_in_type(&type_, db, &|ty| {
                    if let Some(param) = ty.as_type_param(this.db) {
                        // Generic parameter Self
                        *param.name(this.db).symbol() == sym::Self_
                    } else {
                        false
                    }
                })
            })
        }

        fn generics_require_sized_self(assoc_item: hir::AssocItem, db: &dyn HirDatabase) -> bool {
            let includes_sized = assoc_item.includes_self_sized_bounds(db);
            if includes_sized {
                trace!(
                "ignored item {assoc_item} of {trait} since guarded by Sized",
                    assoc_item = assoc_item.name(db).unwrap().as_str(),
                    trait = assoc_item.container(db).debug_display(db),
                );
            }
            includes_sized
        }

        violations.is_empty().not().then_some(violations)
    }

    fn virtual_call_violations_for_method<F>(&self, func: hir::Function, cb: &mut F)
    where
        F: FnMut(MethodViolationCode),
    {
        let db = self.db;

        if !func.has_self_param(db) {
            cb(MethodViolationCode::StaticMethod);
        }

        if func.is_async(db) {
            cb(MethodViolationCode::AsyncFn);
        }

        let is_self_ty = |ty: &hir::Type<'db>| {
            if let Some(param) = ty.as_type_param(self.db) {
                // Generic parameter Self
                *param.name(self.db).symbol() == sym::Self_
            } else {
                false
            }
        };

        if func
            .params_without_self(db)
            .iter()
            .any(|x| includes_type_in_type(x.ty(), db, &is_self_ty))
        {
            cb(MethodViolationCode::ReferencesSelfInput);
        }

        if includes_type_in_type(&func.ret_type(db), db, &is_self_ty) {
            cb(MethodViolationCode::ReferencesSelfOutput);
        }

        let params = hir::GenericDef::from(func).params(db);

        if self.includes_type_in_generic_params_cs_constraints(&params, &is_self_ty) {
            cb(MethodViolationCode::WhereClauseReferencesSelf);
        }
    }

    pub fn includes_type_in_generic_params_cs_constraints(
        &self,
        params: &[hir::GenericParam],
        cond: &impl Fn(&hir::Type<'db>) -> bool,
    ) -> bool {
        (params.iter())
            .filter_map(|&x| variant_or_none!(x, hir::GenericParam::TypeParam))
            .any(|param| self.includes_type_in_generic_param_cs_constraints(param, cond))
    }

    pub fn includes_type_in_generic_param_cs_constraints(
        &self,
        param: hir::TypeParam,
        cond: &impl Fn(&hir::Type<'db>) -> bool,
    ) -> bool {
        let db = self.db;

        let param_type = param.ty(db);
        collect_assoc_type_params(param, param_type.clone(), db).any(|_assoc_ty| {
            includes_type_in_type(&param_type, db, &cond)
                || match param.trait_bounds_of_nested_type_with_args(&param_type, db) {
                    Either::Right(projection) => includes_type_in_type(&projection, db, &cond),
                    Either::Left(traits) => traits.iter().any(|&(trait_, ref args)| {
                        self.map_type_param_source(
                            &self.trait_generic_params_cs_sources_impl(trait_, true),
                            args,
                        )
                        .any(|t| includes_type_in_type(&t, db, &cond))
                    }),
                }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynCompatibilityViolation {
    SizedSelf,
    SelfReferential,
    Method(hir::Function, MethodViolationCode),
    AssocConst(hir::Const),
    //GAT(hir::TypeAlias),
    // This doesn't exist in rustc, but added for better visualization
    //HasNonCompatibleSuperTrait(hir::Trait),
}
