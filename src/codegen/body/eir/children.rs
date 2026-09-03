use crate::codegen::body::eir::{LowerToEir, LowerToEirCtx};
use std::rc::Rc;
use syntax::ast;

// Children
pub(in super::super) struct ChildrenContainer<T>(Rc<[T]>);

#[derive(Clone)]
pub struct Children<T>(Rc<[T]>, usize);

impl<T: Clone> ChildrenContainer<T> {
    pub fn iterator(&self) -> Children<T> {
        Children(self.0.clone(), 0)
    }
}

impl<T: Clone> Iterator for Children<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.get(self.1).inspect(|_| self.1 += 1).cloned()
    }
}

impl<T> FromIterator<T> for ChildrenContainer<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        ChildrenContainer(iter.into_iter().collect::<Rc<[T]>>())
    }
}

impl<Ast, Eir> LowerToEir for ast::AstChildren<Ast>
where
    Ast: ast::AstNode,
    Ast: LowerToEir<Eir = Eir>,
{
    type Eir = ChildrenContainer<Eir>;

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> ChildrenContainer<Eir> {
        self.map(|x| ctx.lower(x)).collect()
    }
}

impl<T> From<Vec<T>> for ChildrenContainer<T> {
    fn from(vec: Vec<T>) -> Self {
        ChildrenContainer(vec.into())
    }
}
