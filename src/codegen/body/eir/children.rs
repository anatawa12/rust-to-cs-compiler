use crate::codegen::body::eir::{LowerToEir, LowerToEirCtx};
use syntax::ast;

// Children
#[derive(Clone)]
pub struct ChildrenContainer<T>(Box<[T]>);

#[derive(Clone)]
pub struct Children<'a, T>(&'a [T], usize);

impl<T> ChildrenContainer<T> {
    pub fn iterator(&self) -> Children<'_, T> {
        Children(&self.0, 0)
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.0.iter_mut()
    }

    pub fn into_iter(self) -> std::vec::IntoIter<T> {
        self.0.into_iter()
    }
}

impl<'a, T> Iterator for Children<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.get(self.1).inspect(|_| self.1 += 1)
    }
}

impl<T> FromIterator<T> for ChildrenContainer<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        ChildrenContainer(iter.into_iter().collect::<Box<[T]>>())
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
