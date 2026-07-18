use hir_ty::next_solver::AnyImplId;

pub trait ImplExt: Copy {
    fn is_builtin_derive(self) -> bool;
}

impl ImplExt for hir::Impl {
    fn is_builtin_derive(self) -> bool {
        match AnyImplId::from(self) {
            AnyImplId::ImplId(_) => false,
            AnyImplId::BuiltinDeriveImplId(_) => true,
        }
    }
}
