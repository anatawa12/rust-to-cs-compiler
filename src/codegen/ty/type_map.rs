use crate::codegen::ty::TyFromType;
use hir::Type;
use hir_ty::next_solver::Ty;
use std::cell::RefCell;
use std::collections::HashMap;

type Inner<'db> = HashMap<Ty<'db>, Ty<'db>>;

pub(crate) struct TypeMap<'db> {
    inner: RefCell<Inner<'db>>,
}

impl<'db> TypeMap<'db> {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner::new()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    pub(super) fn get(&self, ty: Ty<'db>) -> Option<Ty<'db>> {
        self.inner.borrow().get(&ty).copied()
    }

    pub fn modify(&self) -> NewTypeMapScope<'db, '_> {
        NewTypeMapScope::new(self)
    }
}

pub(crate) struct NewTypeMapScope<'db, 'a> {
    type_map: &'a TypeMap<'db>,
    original_map: Inner<'db>,
}

impl<'db, 'a> NewTypeMapScope<'db, 'a> {
    pub fn new(code_gen: &'a TypeMap<'db>) -> Self {
        Self {
            type_map: code_gen,
            original_map: code_gen.inner.borrow().clone(),
        }
    }

    pub fn insert(&mut self, old: Type<'db>, new: Type<'db>) {
        use crate::codegen::ty::TyFromType;
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
