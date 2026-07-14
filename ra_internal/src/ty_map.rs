use crate::internal::TyFromType;
use hir::Type;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::Ty;
use std::cell::RefCell;
use std::collections::HashMap;

type Inner<'db> = HashMap<Ty<'db>, Ty<'db>>;

pub struct TyMap<'db> {
    inner: RefCell<Inner<'db>>,
}

impl<'db> Default for TyMap<'db> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'db> TyMap<'db> {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner::new()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    pub(super) fn get(&self, ty: Ty<'db>) -> Ty<'db> {
        self.inner.borrow().get(&ty).copied().unwrap_or(ty)
    }

    pub fn map_type_recursively(
        &self,
        ty: &Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Option<Type<'db>> {
        if self.is_empty() {
            None
        } else {
            let (ty, env) = (ty.ns_ty(), ty.env());
            let ty = hir_ty::next_solver::fold::fold_tys(
                hir_ty::next_solver::DbInterner::new_with(db, env.krate),
                ty,
                |ty| self.get(ty),
            );
            Some(Type::from_ty_env(ty, env))
        }
    }

    pub fn modify(&self) -> NewTypeMapScope<'db, '_> {
        NewTypeMapScope::new(self)
    }
}

pub struct NewTypeMapScope<'db, 'a> {
    type_map: &'a TyMap<'db>,
    original_map: Inner<'db>,
}

impl<'db, 'a> NewTypeMapScope<'db, 'a> {
    pub fn new(code_gen: &'a TyMap<'db>) -> Self {
        Self {
            type_map: code_gen,
            original_map: code_gen.inner.borrow().clone(),
        }
    }

    pub fn insert(&mut self, old: Type<'db>, new: Type<'db>) {
        self.type_map
            .inner
            .borrow_mut()
            .insert(old.ns_ty(), new.ns_ty());
    }
}
impl<'db, 'a> Drop for NewTypeMapScope<'db, 'a> {
    fn drop(&mut self) {
        *self.type_map.inner.borrow_mut() = std::mem::take(&mut self.original_map);
    }
}
