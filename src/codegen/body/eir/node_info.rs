use syntax::ast;

pub(super) trait CommonInfoFrom<T> {
    fn common_info_from(_: T) -> Self;
}

pub trait ExprInfoCast<T, M> {
    fn cast(&self) -> T;
}

#[derive(Clone)]
pub enum NodeInfo<T> {
    Ast(T),
    None,
}

impl<T> CommonInfoFrom<T> for NodeInfo<T> {
    fn common_info_from(ast: T) -> Self {
        Self::Ast(ast)
    }
}

impl CommonInfoFrom<ast::CallExpr> for NodeInfo<ast::Expr> {
    fn common_info_from(ast: ast::CallExpr) -> Self {
        Self::Ast(ast.into())
    }
}

impl CommonInfoFrom<ast::AwaitExpr> for NodeInfo<ast::Expr> {
    fn common_info_from(ast: ast::AwaitExpr) -> Self {
        Self::Ast(ast.into())
    }
}

impl CommonInfoFrom<ast::BinExpr> for NodeInfo<ast::Expr> {
    fn common_info_from(ast: ast::BinExpr) -> Self {
        Self::Ast(ast.into())
    }
}

mod node_info_cast {
    use super::ExprInfoCast;
    use super::NodeInfo;
    use std::convert::Infallible;
    use std::marker::PhantomData;
    use syntax::ast;

    pub struct ViaFrom<T, U>(PhantomData<(T, U)>);
    pub struct ViaInfallible<T>(PhantomData<T>);
    pub struct Manual;

    pub trait ExprInfoCastDispatch<Target, Mechanism> {
        fn cast_impl(&self) -> Target;
    }

    impl<T, U> ExprInfoCastDispatch<NodeInfo<T>, ViaFrom<T, U>> for NodeInfo<U>
    where
        T: From<U>,
        U: Clone,
    {
        fn cast_impl(&self) -> NodeInfo<T> {
            match self {
                NodeInfo::Ast(ast) => NodeInfo::Ast(T::from(ast.clone())),
                NodeInfo::None => NodeInfo::None,
            }
        }
    }

    impl<T> ExprInfoCastDispatch<NodeInfo<T>, ViaInfallible<T>> for NodeInfo<Infallible> {
        fn cast_impl(&self) -> NodeInfo<T> {
            match self {
                NodeInfo::Ast(ast) => match *ast {},
                NodeInfo::None => NodeInfo::None,
            }
        }
    }

    impl ExprInfoCastDispatch<NodeInfo<ast::NameOrNameRef>, Manual> for NodeInfo<ast::NameRef> {
        fn cast_impl(&self) -> NodeInfo<ast::NameOrNameRef> {
            match self {
                NodeInfo::Ast(ast) => NodeInfo::Ast(ast::NameOrNameRef::NameRef(ast.clone())),
                NodeInfo::None => NodeInfo::None,
            }
        }
    }

    impl ExprInfoCastDispatch<NodeInfo<ast::NameOrNameRef>, Manual> for NodeInfo<ast::Name> {
        fn cast_impl(&self) -> NodeInfo<ast::NameOrNameRef> {
            match self {
                NodeInfo::Ast(ast) => NodeInfo::Ast(ast::NameOrNameRef::Name(ast.clone())),
                NodeInfo::None => NodeInfo::None,
            }
        }
    }

    impl<T, U, M> ExprInfoCast<NodeInfo<T>, M> for NodeInfo<U>
    where
        NodeInfo<U>: ExprInfoCastDispatch<NodeInfo<T>, M>,
    {
        fn cast(&self) -> NodeInfo<T> {
            self.cast_impl()
        }
    }
}
