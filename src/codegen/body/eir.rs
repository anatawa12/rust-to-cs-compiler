//! EIR: Expression IR
//!
//! Intermediate representation of expressions and statements.
//! (Since Rust block is an expression, all expressions may include statements.)
//!
//! Originally, the r2cs compiler was written in words of expression, but we have found some
//! ir transformation is hard to implement with AST because of macro boundary. Therefore, we
//! introduced the EIR.
//!
//! This EIR is similar to THIR in rustc but this is a much higher-level representation.
//! For example, EIR has while, for and loop but HIR has loop only and others are desugured.
//!
//! Most of EIR nodes are backed by AST node and holds that for accessing types of expression,
//! but non-AST-backed EIR nodes are there.

#[macro_use]
mod eir_macros;

// needs new_eir_node
mod children;
mod expr_info;
mod macro_expansion;
mod nodes;

use crate::codegen::{CodeGenerator, output};
use ast::HasAttrs as _;
use syntax::ast;
use syntax::ast::ArrayExprKind;
use syntax::ast::{HasArgList, HasLoopBody, HasName, RangeItem};

pub use ast::BinaryOp;
pub use ast::CmpOp;
pub use ast::Item;
pub use ast::Label;
pub use ast::Lifetime;
pub use ast::LiteralKind;
pub use ast::Ordering;
pub use ast::RangeOp;
pub use ast::UnaryOp;

//pub use children::Children;
pub(super) use children::ChildrenContainer;
pub use expr_info::*;
pub use nodes::*;

pub struct LowerToEirCtx<'g, 'db> {
    pub cg: &'g CodeGenerator<'db>,
}

impl LowerToEirCtx<'_, '_> {
    pub fn lower<T: LowerToEir>(&self, expr: T) -> T::Eir {
        LowerToEir::lower_to_eir(expr, self)
    }
}

pub trait LowerToEir {
    type Eir;
    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Self::Eir;
}

impl<T: LowerToEir> LowerToEir for Option<T> {
    type Eir = Option<T::Eir>;
    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Option<T::Eir> {
        self.map(|t| t.lower_to_eir(ctx))
    }
}

impl LowerToEir for Option<syntax::SyntaxToken> {
    type Eir = bool;

    fn lower_to_eir(self, _: &LowerToEirCtx) -> bool {
        self.is_some()
    }
}

pub trait HasAttrs {
    fn attrs(&self) -> impl Iterator<Item = ast::Attr>;
}

impl HasAttrs for Expr {
    fn attrs(&self) -> impl Iterator<Item = ast::Attr> {
        match self.expr_info() {
            ExprInfo::Ast(ast) => Some(ast.attrs()).into_iter().flatten(),
            ExprInfo::None => None.into_iter().flatten(),
        }
    }
}

impl HasAttrs for MatchArm {
    fn attrs(&self) -> impl Iterator<Item = ast::Attr> {
        match self.expr_info() {
            ExprInfo::Ast(ast) => Some(ast.attrs()).into_iter().flatten(),
            ExprInfo::None => None.into_iter().flatten(),
        }
    }
}

impl HasAttrs for RecordExprField {
    fn attrs(&self) -> impl Iterator<Item = ast::Attr> {
        match self.expr_info() {
            ExprInfo::Ast(ast) => Some(ast.attrs()).into_iter().flatten(),
            ExprInfo::None => None.into_iter().flatten(),
        }
    }
}

impl<Ast, Eir> LowerToEir for Vec<Ast>
where
    Ast: ast::AstNode,
    Ast: LowerToEir<Eir = Eir>,
{
    type Eir = Vec<Eir>;

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Vec<Eir> {
        self.into_iter().map(|x| ctx.lower(x)).collect()
    }
}

macro_rules! lower_identity {
    ($ty: ty) => {
        impl LowerToEir for $ty {
            type Eir = Self;

            fn lower_to_eir(self, _: &LowerToEirCtx) -> $ty {
                self
            }
        }
    };
}

lower_identity!(UnaryOp);
lower_identity!(BinaryOp);
lower_identity!(RangeOp);
lower_identity!(BlockModifier);
lower_identity!(Lifetime);
lower_identity!(Label);
lower_identity!(LiteralKind);
lower_identity!(ast::Type);
lower_identity!(ast::Name);
