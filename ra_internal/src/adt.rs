use crate::generic_def::GenericDefExt;
use crate::{DebugDisplay, TypeExt, TypeParamExt};
use hir::{GenericDef, HasCrate};
use hir_def::AdtId;
use hir_ty::db::HirDatabase;

pub trait AdtExt: Copy {
    fn impls(self, db: &dyn HirDatabase) -> &[hir::Impl];

    fn map_generics_to_impl_generics<'db>(
        self,
        impl_: hir::Impl,
        impl_self_args: &[Option<hir::Type<'db>>],
        db: &'db dyn HirDatabase,
    ) -> Option<Vec<Option<hir::Type<'db>>>>;
}

impl AdtExt for hir::Adt {
    fn impls(self, db: &dyn HirDatabase) -> &[hir::Impl] {
        #[salsa_macros::tracked(returns(ref))]
        fn impls(db: &dyn HirDatabase, adt: AdtId) -> Vec<hir::Impl> {
            let adt = hir::Adt::from(adt);
            hir::Impl::all_in_crate(db, adt.krate(db))
                .into_iter()
                .filter(|impl_| {
                    if let Some((impl_adt, _)) = impl_.self_ty(db).as_adt_with_args() {
                        impl_adt == adt
                    } else {
                        false
                    }
                })
                .collect()
        }

        impls(db, self.into())
    }

    fn map_generics_to_impl_generics<'db>(
        self,
        impl_: hir::Impl,
        impl_self_args: &[Option<hir::Type<'db>>],
        db: &'db dyn HirDatabase,
    ) -> Option<Vec<Option<hir::Type<'db>>>> {
        map_impl_to_adt(self, impl_, impl_self_args, db)
    }
}

fn map_impl_to_adt<'db>(
    adt: hir::Adt,
    impl_: hir::Impl,
    impl_self_args: &[Option<hir::Type<'db>>],
    db: &'db dyn HirDatabase,
) -> Option<Vec<Option<hir::Type<'db>>>> {
    let impl_params = GenericDef::from(impl_).params(db);
    let mut generic_args = vec![None; impl_params.len()];
    let adt_args = GenericDef::from(adt).params(db);
    for (i, arg) in impl_self_args.iter().enumerate() {
        if let Some(arg_as_impl_type_param) = arg.as_ref().and_then(|arg| arg.as_type_param(db)) {
            // Single generic argument is used for multiple type parameters
            if generic_args[arg_as_impl_type_param.param_index(db)].is_some() {
                return None;
            }
            generic_args[arg_as_impl_type_param.param_index(db)] = Some(
                variant_or_none!(adt_args[i], hir::GenericParam::TypeParam)
                    .unwrap()
                    .ty(db),
            )
        }
    }
    for (i, arg) in impl_params.iter().enumerate() {
        if generic_args[i].is_none() {
            generic_args[i] = match arg {
                hir::GenericParam::TypeParam(type_param) => {
                    let back_resolved =
                        hir::GenericDef::from(impl_).back_resolve_projection(type_param.ty(db), db);
                    if back_resolved.len() > 1 {
                        tracing::info!(
                            "multi back_resolved: {back_resolved:?}",
                            back_resolved = back_resolved
                                .iter()
                                .map(|ty| ty.debug_display(db))
                                .collect::<Vec<_>>()
                        );
                    }
                    if back_resolved.is_empty() {
                        Some(hir::Type::error(db, adt.krate(db)))
                    } else {
                        Some({ back_resolved }.swap_remove(0))
                    }
                }
                _ => None,
            };
        }
    }
    Some(generic_args)
}
