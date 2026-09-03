use std::marker::PhantomData;
use syntax::ast;

pub(super) trait CommonInfoFrom<T> {
    fn common_info_from(_: T) -> Self;
}

pub(super) trait ExprInfoCast<T, M> {
    fn cast(&self) -> T;
}

pub struct DummyInfo<T>(PhantomData<T>);

impl<T, E> CommonInfoFrom<T> for DummyInfo<E> {
    fn common_info_from(_: T) -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for DummyInfo<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T, U> ExprInfoCast<DummyInfo<U>, ()> for DummyInfo<T> {
    fn cast(&self) -> DummyInfo<U> {
        DummyInfo(PhantomData)
    }
}

#[derive(Clone)]
pub enum ExprInfo<T> {
    Ast(T),
    None,
}

impl<T> CommonInfoFrom<T> for ExprInfo<T> {
    fn common_info_from(ast: T) -> Self {
        Self::Ast(ast)
    }
}

impl CommonInfoFrom<ast::CallExpr> for ExprInfo<ast::Expr> {
    fn common_info_from(ast: ast::CallExpr) -> Self {
        Self::Ast(ast.into())
    }
}

impl CommonInfoFrom<ast::AwaitExpr> for ExprInfo<ast::Expr> {
    fn common_info_from(ast: ast::AwaitExpr) -> Self {
        Self::Ast(ast.into())
    }
}

mod expr_info_cast {
    use super::ExprInfo;
    use super::ExprInfoCast;
    use std::convert::Infallible;
    use std::marker::PhantomData;

    pub struct ViaFrom<T, U>(PhantomData<(T, U)>);
    pub struct ViaInfallible<T>(PhantomData<T>);

    pub trait ExprInfoCastDispatch<Target, Mechanism> {
        fn cast_impl(&self) -> Target;
    }

    impl<T, U> ExprInfoCastDispatch<ExprInfo<T>, ViaFrom<T, U>> for ExprInfo<U>
    where
        T: From<U>,
        U: Clone,
    {
        fn cast_impl(&self) -> ExprInfo<T> {
            match self {
                ExprInfo::Ast(ast) => ExprInfo::Ast(T::from(ast.clone())),
                ExprInfo::None => ExprInfo::None,
            }
        }
    }

    impl<T> ExprInfoCastDispatch<ExprInfo<T>, ViaInfallible<T>> for ExprInfo<Infallible> {
        fn cast_impl(&self) -> ExprInfo<T> {
            match self {
                ExprInfo::Ast(ast) => match *ast {},
                ExprInfo::None => ExprInfo::None,
            }
        }
    }

    impl<T, U, M> ExprInfoCast<ExprInfo<T>, M> for ExprInfo<U>
    where
        ExprInfo<U>: ExprInfoCastDispatch<ExprInfo<T>, M>,
    {
        fn cast(&self) -> ExprInfo<T> {
            self.cast_impl()
        }
    }
}
