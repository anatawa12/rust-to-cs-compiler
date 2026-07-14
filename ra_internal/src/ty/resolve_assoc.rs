use crate::debug::DebugDisplay;
use crate::internal::{ParsedProjection, TyFromType, parse_bounds_for};
use crate::ty::TypeExt;
use ::hir;
use hir_def::resolver::HasResolver;
use hir_def::signatures::TypeAliasSignature;
use hir_def::*;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::*;
use hir_ty::{GenericPredicates, ImplTraitId};
use rustc_type_ir::inherent::{GenericArg as _, IntoKind};
use rustc_type_ir::{AliasTyKind, Interner};
use tracing::{debug, trace};

/// This tries to resolve `<impl SomeTrait<Assoc = SomeType> as SomeTrait>::Assoc` to a simpler type
pub(super) fn resolve_associated_type<'db>(
    assoc_ty: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> hir::Type<'db> {
    trace!("resolve_assoc_of_impl: {}", assoc_ty.debug_display(db));
    resolve_assoc_of_impl_impl(assoc_ty, db)
}

#[tracing::instrument]
fn resolve_assoc_of_impl_impl<'db>(
    assoc_type: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> hir::Type<'db> {
    let mut alias_list = vec![];

    let mut self_type_slot;
    let self_type = {
        let mut cur = assoc_type;
        while let Some((self_type, alias_id)) = cur.as_associated_type() {
            alias_list.push(alias_id);
            self_type_slot = self_type;
            cur = &self_type_slot;
        }
        cur
    };

    if alias_list.is_empty() {
        return assoc_type.clone();
    }

    alias_list.reverse();

    let interner = DbInterner::new_with(db, assoc_type.env().krate);

    let self_ty = self_type.ns_ty();
    match self_ty.kind() {
        TyKind::Alias(self_ty_alias) => {
            let AliasTyKind::Opaque { def_id } = self_ty_alias.kind else {
                panic!(
                    "Tries to assoc but not assoc (self is not opaque): {assoc_ty}\nkind: {kind:?}",
                    assoc_ty = assoc_type.debug_display(db),
                    kind = self_ty_alias.kind,
                );
                // return assoc_type.clone();
            };
            let bounds = def_id
                .expect_opaque_ty()
                .predicates(db)
                .iter_instantiated_copied(interner, self_ty_alias.args.as_slice());
            let resolver = match def_id.expect_opaque_ty().loc(db) {
                ImplTraitId::ReturnTypeImplTrait(f, _) => f.resolver(db),
                ImplTraitId::TypeAliasImplTrait(a, _) => a.resolver(db),
            };

            match parse_bounds_for(bounds, assoc_type, &resolver, db) {
                ParsedProjection::NoBounds => assoc_type.clone(),
                ParsedProjection::Projection(ty) => ty,
                ParsedProjection::Traits(traits) => {
                    eprintln!(
                        "type: {assoc_ty_rs:?}, traits: {traits:?}",
                        assoc_ty_rs = assoc_type.ty_string(db),
                        traits = traits
                            .iter()
                            .map(|x| x.0.debug_display(db).to_string())
                            .collect::<Vec<_>>()
                    );
                    assoc_type.clone()
                }
            }
        }
        TyKind::Param(param) => {
            let bounds = GenericPredicates::query_explicit(db, param.id.parent()).iter_identity();
            let resolver = param.id.parent().resolver(db);
            match parse_bounds_for(bounds, assoc_type, &resolver, db) {
                ParsedProjection::NoBounds => assoc_type.clone(),
                ParsedProjection::Projection(ty) => ty,
                ParsedProjection::Traits(_) => {
                    /*
                    unimplemented!(
                        "{assoc_ty:?}: {assoc_ty_rs}",
                        assoc_ty_rs = assoc_ty.display(self.db, self.display_target()),
                    )
                     */
                    assoc_type.clone()
                }
            }
        }
        TyKind::Adt(adt, _args) => {
            let interner =
                DbInterner::new_with(db, hir::Adt::from(adt.def_id()).module(db).krate(db).base());
            let alias_id = TypeAliasId::from(alias_list[0]);
            let rest_alias = &alias_list[1..];

            let mut resolved_impl_assoc = None;

            let trait_ = match alias_id.lookup(db).container {
                ItemContainerId::TraitId(t) => t,
                container => {
                    panic!("Projection type alias is defined in non-trait: {container:?}");
                }
            };
            let trait_assoc_data = TypeAliasSignature::of(db, alias_id);

            // Finds impl block that implements the trait, and find TypeAlias associated type with same name.
            interner.for_each_relevant_impl(trait_.into(), self_ty, |impl_def_id| {
                let AnyImplId::ImplId(impl_id) = impl_def_id else {
                    // Builtin derive traits don't have type/consts assoc items.
                    return;
                };

                let Some(alias) = impl_id
                    .impl_items(db)
                    .items
                    .iter()
                    .filter(|(impl_assoc_name, _)| *impl_assoc_name == trait_assoc_data.name)
                    .find_map(|(_, impl_assoc_id)| {
                        variant_or_none!(*impl_assoc_id, AssocItemId::TypeAliasId)
                    })
                    .or_else(|| {
                        if trait_assoc_data.ty.is_some() {
                            Some(alias_id)
                        } else {
                            None
                        }
                    })
                else {
                    return;
                };

                assert!(interner.has_item_definition(alias.into()));

                let ty = db.ty(alias.into());
                let args = create_impl_generic_args_for(self_ty, impl_id, interner, db);
                assert!(interner.check_args_compatible(AnyImplId::ImplId(impl_id).into(), args));
                resolved_impl_assoc = Some(ty.skip_binder());
            });

            trace!(
                "resolved?: {assoc_ty} => {resolved:?}\n{resolved1:?}",
                assoc_ty = assoc_type.debug_display(db),
                resolved = resolved_impl_assoc
                    .map(|x| assoc_type.derived(x).debug_display(db).to_string()),
                resolved1 = resolved_impl_assoc
                    .map(|x| format!("{:?}", assoc_type.derived(x).ty_string(db))),
            );

            // internerが大事な役割果たしてそう。for_each_relevant_implでtraitと型がらimpl一覧取れたりする
            // fetch_eligible_assoc_item of SolverContextがimpl => typeの解決につかえる
            // Intener::has_item_definition が true なとき、ちゃんと定義されてて、
            // EvalCtxt::translate_args Intener::check_args_compatible や　Intener::type_of (db::ty) で最終的に解決する

            let inner = resolved_impl_assoc.expect("No impl found for assoc type trait");

            if rest_alias.is_empty() {
                assoc_type.derived(inner)
            } else {
                // we only have resolved the innermost impl block. We continue resolving remaining assoc types.
                resolve_assoc_of_impl_impl(
                    &assoc_type
                        .derived(inner)
                        .new_associated_type(rest_alias, db),
                    db,
                )
            }
        }
        TyKind::Error(_) => assoc_type.clone(),
        _ => {
            eprintln!(
                "Unsupported assoc type resolution but not assoc (self is not alias): {assoc_type:?}"
            );
            hir::Type::error(db, assoc_type.env().krate.into())
        }
    }
}

/// Instantiates Generic paramaeters for the `target` impl block based on the `self_ty`.
///
/// For example, if there is `impl<T> Trait for T` with self_ty being `String`, this creates GenericArgs `<String>`.
/// For example, if there is `impl<T> Struct<T>` with self_ty being `Struct<i32>`, this creates GenericArgs `<i32>`.
#[tracing::instrument()]
fn create_impl_generic_args_for<'db>(
    self_ty: Ty<'db>,
    target: ImplId,
    interner: DbInterner<'db>,
    db: &'db dyn HirDatabase,
) -> GenericArgs<'db> {
    let impl_self_ty = db.impl_self_ty(target).instantiate_identity();
    let (self_adt, self_args) = self_ty.as_adt().unwrap();
    match impl_self_ty.kind() {
        TyKind::Param(param_ty) => {
            debug!("translate_args: param: {param_ty:?}");

            GenericArgs::for_item(interner, target.into(), |_i, arg, _| match arg {
                GenericParamId::TypeParamId(ty_arg) if ty_arg == param_ty.id => {
                    Term::from(self_ty).into()
                }
                _ => GenericArg::error_from_id(interner, arg),
            })
        }
        TyKind::Adt(impl_adt_def, impl_adt_args) => {
            assert_eq!(impl_adt_def.def_id(), self_adt);
            trace!("translate_args: adt: {impl_adt_def:?} {impl_adt_args:?} {self_args:?}");

            let mut type_map = std::collections::HashMap::new();

            for (impl_arg, self_arg) in impl_adt_args.as_slice().iter().zip(self_args.as_slice()) {
                if let Some(impl_arg) = impl_arg.ty() {
                    let self_arg = self_arg.expect_ty();

                    match impl_arg.kind() {
                        TyKind::Param(param)
                            if param.id.parent() == GenericDefId::ImplId(target) =>
                        {
                            type_map.insert(GenericParamId::TypeParamId(param.id), self_arg);
                        }
                        _ => {
                            panic!("unsupported type");
                        }
                    }
                }
            }

            GenericArgs::for_item(interner, target.into(), |_i, arg, _| {
                if let Some(ty) = type_map.get(&arg) {
                    Term::from(*ty).into()
                } else {
                    GenericArg::error_from_id(interner, arg)
                }
            })
        }
        impl_self_ty => panic!("Unexpected impl_self_ty: {impl_self_ty:?}"),
    }
}
