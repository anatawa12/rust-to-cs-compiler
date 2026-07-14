use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{DbInterner, GenericArgKind, GenericArgs, SolverDefId, Ty, TyKind};
use rustc_type_ir::inherent::IntoKind;
use rustc_type_ir::{AliasTyKind, Interner};

pub struct TypeToString<'db> {
    pub interner: DbInterner<'db>,
    pub db: &'db dyn HirDatabase,
}

impl TypeToString<'_> {
    #[allow(dead_code)]
    pub(super) fn ty_to_str<'a>(&'a self, ty: Ty<'a>) -> impl std::fmt::Debug + 'a {
        std::fmt::from_fn(move |f| {
            match ty.kind() {
                TyKind::Array(inner, count) => f
                    .debug_tuple("Array")
                    .field(&self.ty_to_str(inner))
                    .field(&count)
                    .finish(),
                TyKind::Slice(inner) => f
                    .debug_tuple("Slice")
                    .field(&self.ty_to_str(inner))
                    .finish(),
                TyKind::RawPtr(inner, mutability) => f
                    .debug_tuple("RawPtr")
                    .field(&self.ty_to_str(inner))
                    .field(&mutability)
                    .finish(),
                TyKind::Ref(reg, inner, mutability) => f
                    .debug_tuple("Ref")
                    .field(&reg)
                    .field(&self.ty_to_str(inner))
                    .field(&mutability)
                    .finish(),
                TyKind::Alias(alias) => f
                    .debug_tuple("Alias")
                    .field(&std::fmt::from_fn(move |f| match alias.kind {
                        AliasTyKind::Projection { def_id } => f
                            .debug_struct("Projection")
                            .field("def_id", &self.def_id_to_str(def_id))
                            .finish(),
                        AliasTyKind::Inherent { def_id } => f
                            .debug_struct("Inherent")
                            .field("def_id", &self.def_id_to_str(def_id))
                            .finish(),
                        AliasTyKind::Opaque { def_id } => f
                            .debug_struct("Opaque")
                            .field("def_id", &self.def_id_to_str(def_id))
                            .field("interned", &self.interner.type_of(def_id))
                            .finish(),
                        AliasTyKind::Free { def_id } => f
                            .debug_struct("Free")
                            .field("def_id", &self.def_id_to_str(def_id))
                            .finish(),
                    }))
                    .field(&self.args_to_str(alias.args))
                    .finish(),
                TyKind::Param(param) => f
                    .debug_struct("Param")
                    .field("id", &param.id)
                    .field("index", &param.index)
                    .finish(),

                //TyKind::FnDef(_, _) => {}
                //TyKind::FnPtr(_, _) => {}
                //TyKind::UnsafeBinder(_) => {}
                //TyKind::Dynamic(_, _) => {}
                //TyKind::Closure(_, _) => {}
                //TyKind::CoroutineClosure(_, _) => {}
                //TyKind::Coroutine(_, _) => {}
                //TyKind::CoroutineWitness(_, _) => {}
                //TyKind::Never => {}
                //TyKind::Tuple(_) => {}
                //TyKind::Bound(_, _) => {}
                //TyKind::Placeholder(_) => {}
                //TyKind::Infer(_) => {}
                //TyKind::Error(_) => {}
                m => std::fmt::Debug::fmt(&m, f),
            }
        })
    }

    pub(super) fn args_to_str<'a>(&'a self, args: GenericArgs<'a>) -> impl std::fmt::Debug + 'a {
        std::fmt::from_fn(move |f| {
            let mut list = f.debug_list();
            for arg in args.as_slice() {
                list.entry(&std::fmt::from_fn(move |f| match arg.kind() {
                    GenericArgKind::Type(ty) => {
                        f.debug_tuple("Type").field(&self.ty_to_str(ty)).finish()
                    }
                    _ => std::fmt::Debug::fmt(&arg, f),
                }));
            }
            list.finish()
        })
    }

    pub(super) fn def_id_to_str<'a>(&'a self, def_id: SolverDefId) -> impl std::fmt::Debug + 'a {
        std::fmt::from_fn(move |f| match def_id {
            SolverDefId::InternedOpaqueTyId(id) => f
                .debug_tuple("InternedOpaqueTyId")
                .field(&id)
                .field(&id.loc(self.db))
                .field(&id.loc(self.db).predicates(self.db))
                .finish(),

            _ => std::fmt::Debug::fmt(&def_id, f),
        })
    }
}
