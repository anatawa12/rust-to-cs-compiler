use super::eir::*;
use crate::new_eir_node;
use std::mem::{replace, take};
use std::ops::ControlFlow;
use std::ops::ControlFlow::Break;
use std::ops::ControlFlow::Continue;
use tracing::trace;

def_transformer!(
    pub struct ImplicitReturns;
);

impl MutatingEirVisitor for ImplicitReturns<'_, '_> {
    type Break = std::convert::Infallible;

    fn visit_fn(&mut self, f: &mut Fn) -> ControlFlow<Self::Break> {
        if let Some(ref mut body) = f.as_mut().body {
            let _ = body.accept_mut(&mut ImplicitReturnsImpl::new(self.cg));
        }
        f.accept_children_mut(self)
    }

    fn visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::BlockExpr(block_expr)
                if let Some(
                    BlockModifier::Async | BlockModifier::Gen | BlockModifier::AsyncGen,
                ) = block_expr.modifier() =>
            {
                let block_expr = block_expr.as_mut();
                visit_block_like(
                    &mut ImplicitReturnsImpl::new(self.cg),
                    &mut block_expr.statements,
                    &mut block_expr.tail_expr,
                )
            }
            Expr::ClosureExpr(closure) => {
                let body = &mut closure.as_mut().body;
                if let Expr::BlockExpr(block_expr) = body {
                    let _ = block_expr.accept_mut(&mut ImplicitReturnsImpl::new(self.cg));
                }
            }
            _ => {}
        }

        let _ = expr.accept_children_mut(self);

        if let Expr::MacroStmts(block) = expr {
            visit_statements(self, &mut block.as_mut().statements)
        }

        Continue(())
    }

    fn visit_block_expr(&mut self, block: &mut BlockExpr) -> ControlFlow<Self::Break> {
        let _ = block.accept_children_mut(self);

        visit_statements(self, &mut block.as_mut().statements);

        Continue(())
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) -> ControlFlow<Self::Break> {
        if let Stmt::ExprStmt(expr) = stmt
            && let Expr::ReturnExpr(return_expr) = &mut expr.as_mut().expr
        {
            let return_expr = return_expr.as_mut();
            if let Some(value) = &mut return_expr.expr {
                let emitted_return = value
                    .accept_mut(&mut ImplicitReturnsImpl::new(self.cg))
                    .break_value()
                    .unwrap_or_default();

                if emitted_return {
                    let value = replace(value, super::ReplaceDefault::replace_default());
                    expr.as_mut().expr = value;
                }
            }
        }

        stmt.accept_children_mut(self)
    }
}

fn visit_statements(visitor: &mut ImplicitReturns, statements: &mut ChildrenContainer<Stmt>) {
    let mut as_vec = replace(statements, vec![].into()).into_vec();

    let mut i = 0;
    while i < as_vec.len() {
        if let Stmt::ExprStmt(expr) = &mut as_vec[i]
            && let Expr::ReturnExpr(return_expr) = &mut expr.as_mut().expr
            && let Some(value) = &mut return_expr.as_mut().expr
            && visitor.eir_sem.type_of_expr(value).adjusted().is_unit()
        {
            trace!(
                "returning unit value(0) at {}",
                visitor.eir_sem.location(value)
            );
            let value = return_expr.as_mut().expr.take().unwrap();
            as_vec.insert(
                i,
                Stmt::ExprStmt(new_eir_node!(ExprStmt {
                    expr: value,
                    node_info: NodeInfo::None,
                })),
            );
        }

        i += 1;
    }

    *statements = as_vec.into();
}

def_transformer!(
    /// Note: Only returning exprs will be 'visited' by this transformer.
    ///
    /// Continue means the expression has a value.
    /// Break(true) means the inner expression has a return stmt in it.
    pub struct ImplicitReturnsImpl;
);

impl MutatingEirVisitor for ImplicitReturnsImpl<'_, '_> {
    type Break = bool;

    fn visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::RawCodeExpr(raw_code) if raw_code.divergent() => Break(true),
            Expr::BlockExpr(block) => self.visit_block_expr(block),
            Expr::MacroStmts(block) => {
                let block = block.as_mut();
                visit_block_like(self, &mut block.statements, &mut block.tail_expr);
                Break(true)
            }
            Expr::ForExpr(_) => Break(true),
            Expr::WhileExpr(_) => Break(true),
            Expr::LoopExpr(_) => Break(true),
            Expr::IfExpr(if_expr) => self.visit_if_expr(if_expr),
            Expr::MatchExpr(match_expr) => {
                let match_expr = match_expr.as_mut();
                for arm in match_expr.match_arm_list.as_mut().arms.iter_mut() {
                    let emitted_return = (arm.as_mut().expr.accept_mut(self))
                        .break_value()
                        .unwrap_or_default();
                    if !emitted_return {
                        let expr = &mut arm.as_mut().expr;
                        replace_node!(
                            expr as inner = Expr::ReturnExpr(new_eir_node!(ReturnExpr {
                                expr: Some(inner),
                                node_info: NodeInfo::None,
                            }))
                        )
                    }
                }
                Break(true)
            }
            Expr::ReturnExpr(_) => Break(true),
            _ => Continue(()),
        }
    }

    fn visit_block_expr(&mut self, block: &mut BlockExpr) -> ControlFlow<Self::Break> {
        if matches!(
            block.modifier(),
            Some(BlockModifier::Const | BlockModifier::Unsafe | BlockModifier::Label(_)) | None
        ) {
            let block = block.as_mut();
            visit_block_like(self, &mut block.statements, &mut block.tail_expr);
            Break(true)
        } else {
            Continue(())
        }
    }

    fn visit_if_expr(&mut self, if_expr: &mut IfExpr) -> ControlFlow<Self::Break> {
        let if_expr = if_expr.as_mut();
        let _ = if_expr.then_branch.accept_mut(self);
        if let Some(else_branch) = &mut if_expr.else_branch {
            let _ = else_branch.accept_mut(self);
        }
        Break(true)
    }
}

fn visit_block_like(
    visitor: &mut ImplicitReturnsImpl<'_, '_>,
    statements_stmts: &mut ChildrenContainer<Stmt>,
    tail_expr: &mut Option<Expr>,
) {
    let mut statements = replace(statements_stmts, vec![].into()).into_vec();
    if let Some(mut tail_expr) = take(tail_expr) {
        let emitted_return = tail_expr
            .accept_mut(visitor)
            .break_value()
            .unwrap_or_default();
        if emitted_return {
            statements.push(Stmt::from(new_eir_node!(ExprStmt {
                expr: tail_expr,
                node_info: NodeInfo::None,
            })));
        } else {
            trace!(
                "generating return at {}",
                visitor.eir_sem.location(&tail_expr)
            );
            statements.push(Stmt::from(new_eir_node!(ExprStmt {
                expr: Expr::ReturnExpr(new_eir_node!(ReturnExpr {
                    expr: Some(tail_expr),
                    node_info: NodeInfo::None,
                })),
                node_info: NodeInfo::None,
            })));
        }
    } else {
        statements.push(Stmt::from(new_eir_node!(ExprStmt {
            expr: Expr::ReturnExpr(new_eir_node!(ReturnExpr {
                expr: None,
                node_info: NodeInfo::None,
            })),
            node_info: NodeInfo::None,
        })));
    }
    *statements_stmts = statements.into();
}
