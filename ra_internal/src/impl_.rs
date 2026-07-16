use crate::TypeExt;
use crate::internal::TyFromType;
use hir::{HasCrate, Type};
use hir_def::HasModule;
use hir_def::resolver::HasResolver;
use hir_ty::ParamEnvAndCrate;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{AnyImplId, DbInterner, GenericArg};
use rustc_type_ir::inherent::SliceLike;

pub trait ImplExt {
    fn self_ty_instantiated<'db>(
        self,
        db: &'db dyn HirDatabase,
        args: Vec<hir::Type<'db>>,
    ) -> hir::Type<'db>;
}

impl ImplExt for hir::Impl {
    fn self_ty_instantiated<'db>(
        self,
        db: &'db dyn HirDatabase,
        args: Vec<hir::Type<'db>>,
    ) -> hir::Type<'db> {
        match AnyImplId::from(self) {
            AnyImplId::ImplId(id) => {
                let resolver = id.resolver(db);
                let interner = DbInterner::new_no_crate(db);

                let params = hir::GenericDef::from(self).params(db);
                let mut def_args = Vec::with_capacity(params.len());
                let mut type_iter = args.into_iter();
                for x in params {
                    match x {
                        hir::GenericParam::TypeParam(_) => def_args.push(GenericArg::from(
                            type_iter
                                .next()
                                .unwrap_or_else(|| Type::error(db, self.krate(db)))
                                .ns_ty(),
                        )),
                        hir::GenericParam::ConstParam(_) => {
                            def_args.push(hir_ty::next_solver::Const::error(interner).into())
                        }
                        hir::GenericParam::LifetimeParam(_) => {
                            def_args.push(hir_ty::next_solver::Region::error(interner).into())
                        }
                    }
                }

                let ty = db
                    .impl_self_ty(id)
                    .instantiate(interner, def_args.as_slice());
                hir::Type::from_ty_resolver(ty, db, &resolver)
            }
            AnyImplId::BuiltinDeriveImplId(id) => {
                let loc = id.loc(db);
                let krate = loc.module(db).krate(db);
                let interner = DbInterner::new_with(db, krate);
                let env = ParamEnvAndCrate {
                    param_env: hir_ty::builtin_derive::param_env(interner, id),
                    krate,
                };
                let ty = hir_ty::builtin_derive::impl_trait(interner, id)
                    .instantiate_identity()
                    .self_ty();
                hir::Type::from_ty_env(ty, env)
            }
        }
    }
}
