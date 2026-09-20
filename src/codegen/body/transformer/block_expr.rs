use crate::codegen::body::eir;
use crate::codegen::body::eir::*;
use crate::codegen::body::transformer::has_stmt_in_expr;
use crate::new_eir_node;
use hir::mir::BinOp;
use std::convert::Infallible;
use std::mem::{replace, take};
use std::ops::ControlFlow;

def_transformer!(
    /// This transformer transforms `$(let)? pattern = block_like_expression` to
    /// `$(let pattern;)? { block_stmts; pattern = last_expr; }`.
    ///
    /// This fixes the problem that C# doesn't have block expressions.
    ///
    /// This doesn't support non-assignment place block-like expressions,
    /// but it's unlikely to be used in practice.
    pub struct ExpandBlockExpr;
);

impl MutatingEirVisitor for ExpandBlockExpr<'_, '_> {
    type Break = Infallible;

    fn visit_stmt(&mut self, stmt: &mut Stmt) -> ControlFlow<Self::Break> {
        if let Stmt::LetStmt(let_stmt) = stmt
            && let_stmt.let_else().is_none()
            && let Some(initializer) = let_stmt.initializer()
            && has_stmt_in_expr(initializer)
            && let Some(place_expr) = pat_to_assignee_expr(let_stmt.pat())
        {
            replace_node!(
                stmt as Stmt::LetStmt(mut let_stmt) = {
                    let mut initializer = take(&mut let_stmt.as_mut().initializer).unwrap();
                    tail_expr_to_assignment(&mut initializer, &place_expr);
                    Stmt::ExprStmt(new_eir_node!(ExprStmt {
                        node_info: NodeInfo::None,
                        expr: Expr::MacroStmts(new_eir_node!(MacroStmts {
                            node_info: NodeInfo::None,
                            statements: vec![
                                Stmt::LetStmt(let_stmt),
                                Stmt::ExprStmt(new_eir_node!(ExprStmt {
                                    node_info: NodeInfo::None,
                                    expr: initializer,
                                })),
                            ].into(),
                            tail_expr: None,
                        }))
                    }))
                }
            );
        }
        stmt.accept_children_mut(self)
    }
}

fn pat_to_assignee_expr(pat: &Pat) -> Option<Expr> {
    match pat {
        Pat::IdentPat(ident) => {
            if let NodeInfo::Ast(ident) = ident.node_info() {
                Some(Expr::PathExpr(new_eir_node!(PathExpr {
                    node_info: NodeInfo::None,
                    path: Path::IdentPat(ident)
                })))
            } else {
                None
            }
        }
        Pat::PathPat(path) => Some(Expr::PathExpr(new_eir_node!(PathExpr {
            node_info: NodeInfo::None,
            path: path.path().clone()
        }))),
        Pat::SlicePat(slice) => {
            if slice.components().slice().is_some() {
                return None;
            }
            Some(Expr::ArrayElementListExpr(new_eir_node!(
                ArrayElementListExpr {
                    node_info: NodeInfo::None,
                    elements: slice
                        .components()
                        .prefix()
                        .map(pat_to_assignee_expr)
                        .collect::<Option<Vec<_>>>()?
                        .into(),
                }
            )))
        }
        Pat::TuplePat(tuple) => Some(Expr::TupleExpr(new_eir_node!(TupleExpr {
            node_info: NodeInfo::None,
            fields: tuple
                .fields()
                .map(pat_to_assignee_expr)
                .collect::<Option<Vec<_>>>()?
                .into(),
        }))),
        Pat::WildcardPat(_) => Some(Expr::UnderscoreExpr(new_eir_node!(UnderscoreExpr {
            node_info: NodeInfo::None,
        }))),

        // RecordPat and TupleStructPat are assignee expression but not supported by C#
        Pat::LiteralPat(_) => None,
        Pat::OrPat(_) => None,
        Pat::RangePat(_) => None,
        Pat::RecordPat(_) => None,
        Pat::RefPat(_) => None,
        Pat::RestPat(_) => None,
        Pat::TupleStructPat(_) => None,
    }
}

fn tail_expr_to_assignment(expr: &mut Expr, target: &Expr) {
    match expr {
        Expr::BlockExpr(block) => tail_expr_to_assignment_for_block(block, target),
        Expr::IfExpr(if_expr) => {
            let mut if_expr = if_expr;

            loop {
                tail_expr_to_assignment_for_block(&mut if_expr.as_mut().then_branch, target);
                match if_expr.as_mut().else_branch {
                    Some(ElseBranch::IfExpr(ref mut else_if_expr)) => if_expr = else_if_expr,
                    Some(ElseBranch::Block(ref mut block)) => {
                        tail_expr_to_assignment_for_block(block, target);
                        break;
                    }
                    None => break,
                }
            }
        }
        Expr::MatchExpr(match_expr) => {
            for arm in match_expr.as_mut().match_arm_list.as_mut().arms.as_mut() {
                let expr_place = &mut arm.as_mut().expr;
                replace_node!(
                    expr_place as expr = {
                        Expr::BinExpr(new_eir_node!(BinExpr {
                            lhs: target.clone(),
                            rhs: expr,
                            op_kind: BinaryOp::Assignment { op: None },
                            node_info: NodeInfo::None,
                        }))
                    }
                );
            }
        }
        _ => unreachable!(),
    }
}

fn tail_expr_to_assignment_for_block(block: &mut BlockExpr, target: &Expr) {
    if let Some(expr) = block.as_mut().tail_expr.take() {
        let mut statements = replace(&mut block.as_mut().statements, vec![].into()).into_vec();
        statements.push(Stmt::ExprStmt(new_eir_node!(ExprStmt {
            node_info: NodeInfo::None,
            expr: Expr::BinExpr(new_eir_node!(BinExpr {
                lhs: target.clone(),
                rhs: expr,
                op_kind: BinaryOp::Assignment { op: None },
                node_info: NodeInfo::None,
            })),
        })));
        block.as_mut().statements = statements.into();
    }
}
