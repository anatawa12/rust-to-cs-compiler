use super::eir::*;
use crate::codegen::CodeGenerator;
use std::ops::ControlFlow;

/// The struct implements translating EIR for C# generation
pub struct EirTransformer<'a, 'db> {
    cg: &'a CodeGenerator<'db>,
}

impl<'g, 'db> std::ops::Deref for EirTransformer<'g, 'db> {
    type Target = CodeGenerator<'db>;

    fn deref(&self) -> &Self::Target {
        self.cg
    }
}

impl<'g, 'db> EirTransformer<'g, 'db> {
    pub fn new(cg: &'g CodeGenerator<'db>) -> Self {
        Self { cg }
    }
}

macro_rules! replace_node {
    ($expr: ident as $($path: ident)::+($casted: ident) = $inner: expr) => {{
        let $($path)::+($casted) = std::mem::replace($expr, Expr::from(new_eir_node!(RawCodeExpr {
            code: code!(""),
            divergent: false,
            node_info: NodeInfo::None
        }))) else {
            panic!("")
        };
        *$expr = $inner;
    }};
}

impl MutatingEirVisitor for EirTransformer<'_, '_> {
    type Break = std::convert::Infallible;

    fn visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        let db = self.db;

        expr.accept_children_mut(self)
    }
}
