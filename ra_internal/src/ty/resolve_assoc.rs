use crate::debug::DebugDisplay;
use crate::internal::{ParsedProjection, TyFromType, parse_bounds_for};
use crate::ty::TypeExt;
use ::hir;
use hir_def::*;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::*;
use rustc_type_ir::AliasTyKind;
use rustc_type_ir::inherent::{GenericArg as _, IntoKind};
use tracing::{debug, trace};

/// This tries to resolve `<impl SomeTrait<Assoc = SomeType> as SomeTrait>::Assoc` to a simpler type
pub(super) fn resolve_associated_type<'db>(
    assoc_ty: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> hir::Type<'db> {
    trace!("resolve_assoc_of_impl: {}", assoc_ty.debug_display(db));
    resolve_assoc_of_impl_impl(assoc_ty, db)
}

#[tracing::instrument(skip_all, fields(assoc_type = %assoc_type.debug_display(db)))]
fn resolve_assoc_of_impl_impl<'db>(
    assoc_type: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> hir::Type<'db> {
    let mut alias_list = vec![];

    let mut self_type_slot;
    let self_type = {
        let mut cur = assoc_type;
        while let Some((self_type, args, alias_id)) = cur.as_associated_type() {
            alias_list.push((alias_id, args));
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

            match parse_bounds_for(bounds, assoc_type, db) {
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
        TyKind::Param(_) => assoc_type.clone(),
        self_ty => {
            panic!("{self_ty:?} = {}", assoc_type.debug_display(db));
        }
    }
}

/// Instantiates Generic paramaeters for the `target` impl block based on the `self_ty`.
///
/// For example, if there is `impl<T> Trait for T` with self_ty being `String`, this creates GenericArgs `<String>`.
/// For example, if there is `impl<T> Struct<T>` with self_ty being `Struct<i32>`, this creates GenericArgs `<i32>`.
#[allow(dead_code)]
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
                            panic!(
                                "unsupported type: adt = {adt}, impl for = {impl_for:?}",
                                impl_for = hir::Impl::from(target)
                                    .trait_(db)
                                    .as_ref()
                                    .map(|x| x.debug_display(db)),
                                adt = hir::Adt::from(impl_adt_def.def_id()).debug_display(db),
                            );
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
