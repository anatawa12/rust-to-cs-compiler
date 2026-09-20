use crate::codegen::CodeGenerator;
use crate::codegen::body::eir;
use crate::codegen::body::eir::{BlockExpr, ChildrenContainer, EirNode, ElseBranch, Expr};
use crate::new_eir_node;

trait ReplaceDefault {
    fn replace_default() -> Self;
}

impl ReplaceDefault for eir::Expr {
    fn replace_default() -> Self {
        eir::Expr::from(new_eir_node!(eir::RawCodeExpr {
            code: code!(""),
            divergent: false,
            node_info: eir::NodeInfo::None
        }))
    }
}

impl ReplaceDefault for eir::Pat {
    fn replace_default() -> Self {
        eir::Pat::from(new_eir_node!(eir::WildcardPat {
            node_info: eir::NodeInfo::None
        }))
    }
}

impl ReplaceDefault for eir::Stmt {
    fn replace_default() -> Self {
        eir::Stmt::from(new_eir_node!(eir::ExprStmt {
            expr: eir::Expr::from(new_eir_node!(eir::RawCodeExpr {
                code: code!(""),
                divergent: false,
                node_info: eir::NodeInfo::None,
            })),
            node_info: eir::NodeInfo::None,
        }))
    }
}

impl<T> ReplaceDefault for ChildrenContainer<T> {
    fn replace_default() -> Self {
        vec![].into()
    }
}

macro_rules! replace_node {
    ($expr: ident as $casted: pat = $inner: expr) => {{
        #[allow(irrefutable_let_patterns)]
        let $casted = std::mem::replace(
            $expr,
            crate::codegen::body::transformer::ReplaceDefault::replace_default(),
        ) else {
            panic!("")
        };
        *$expr = $inner;
    }};
}

macro_rules! def_transformer {
    (
        $(#[$meta: meta])*
        $vis: vis struct $name: ident;
    ) => {
        $(#[$meta])*
        $vis struct $name<'a, 'db> {
            cg: &'a crate::codegen::CodeGenerator<'db>,
        }

        impl<'g, 'db> std::ops::Deref for $name<'g, 'db> {
            type Target = crate::codegen::CodeGenerator<'db>;

            fn deref(&self) -> &Self::Target {
                self.cg
            }
        }

        impl<'g, 'db> $name<'g, 'db> {
            pub fn new(cg: &'g crate::codegen::CodeGenerator<'db>) -> Self {
                Self { cg }
            }
        }

    };
}

fn take_eir<T: ReplaceDefault>(value: &mut T) -> T {
    std::mem::replace(value, ReplaceDefault::replace_default())
}

fn has_stmt_in_expr(expr: &Expr) -> bool {
    fn is_block_like_expression_block(block: &BlockExpr) -> bool {
        block.statements().next().is_some()
    }
    match expr {
        Expr::BlockExpr(block) => is_block_like_expression_block(block),
        Expr::IfExpr(if_expr) => {
            let mut if_expr = if_expr;
            loop {
                if is_block_like_expression_block(if_expr.then_branch()) {
                    return true;
                }
                match if_expr.else_branch() {
                    Some(ElseBranch::IfExpr(else_if_expr)) => if_expr = else_if_expr,
                    Some(ElseBranch::Block(block)) => {
                        return is_block_like_expression_block(block);
                    }
                    None => break false,
                }
            }
        }
        Expr::MatchExpr(match_expr) => match_expr
            .match_arm_list()
            .arms()
            .any(|arm| has_stmt_in_expr(arm.expr())),
        _ => false,
    }
}

mod block_expr;
mod expand_macro_like_functions;
mod implicit_returns;
mod match_or_pattern_definition;

pub fn transform(cg: &CodeGenerator, node: &mut impl EirNode) {
    macro_rules! accept {
        ($($path: ident)::+) => {
            let std::ops::ControlFlow::Continue(()) = node.accept_mut(&mut $($path)::+::new(cg));
        };
    }

    accept!(implicit_returns::ImplicitReturns);
    accept!(expand_macro_like_functions::ExpandMacroLikeFunctions);
    accept!(block_expr::ExpandBlockExpr);

    accept!(match_or_pattern_definition::MatchOrPatternDefinitions);
}
