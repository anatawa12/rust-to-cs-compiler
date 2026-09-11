use super::eir::*;
use crate::codegen::constructable::ConstructableDef;
use crate::new_eir_node;
use hir::HasContainer;
use std::ops::ControlFlow;

def_transformer!(
    /// Implements basic transformation of EIR nodes
    pub struct ExpandMacroLikeFunctions;
);

impl MutatingEirVisitor for ExpandMacroLikeFunctions<'_, '_> {
    type Break = std::convert::Infallible;

    fn visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        let db = self.db;

        if let Expr::CallExpr(call_expr) = expr
            && let callee = call_expr.expr()
            && let Expr::PathExpr(path) = callee
            && let Some((hir::PathResolution::Def(def), _)) =
                self.eir_sem.resolve_path_with_subst(path.path())
            && let Some(ConstructableDef::EnumVariant(def)) = ConstructableDef::from_module_def(def)
            && self.lang_items.Cow() == Some(def.parent_enum(db))
        {
            replace_node!(
                expr as Expr::CallExpr(call_expr) = {
                    call_expr
                        .into_inner()
                        .arg_list
                        .into_inner()
                        .args
                        .into_iter()
                        .nth(0)
                        .unwrap()
                }
            );
            return self.visit_expr(expr);
        }

        if let Expr::MatchExpr(match_expr) = expr {
            let mut arms = std::mem::replace(
                &mut match_expr.as_mut().match_arm_list.as_mut().arms,
                Vec::new().into(),
            )
            .into_vec();
            let mut pat_visitor = EirFixCowPatterns::new(self.cg);

            arms.retain_mut(|arm| {
                !arm.as_mut()
                    .pat
                    .accept_mut(&mut pat_visitor)
                    .break_value()
                    .unwrap_or(false)
            });

            match_expr.as_mut().match_arm_list.as_mut().arms = arms.into();
        }

        expr.accept_children_mut(self)
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) -> ControlFlow<Self::Break> {
        let db = self.db;

        if let Stmt::ExprStmt(expr_stmt) = stmt
            && let expr = &mut expr_stmt.as_mut().expr
            && let Expr::CallExpr(call_expr) = expr
            && let Expr::PathExpr(path) = call_expr.expr()
            && let Some(hir::PathResolution::Def(hir::ModuleDef::Function(f))) =
                self.eir_sem.resolve_path(path.path())
            && f.name(db).symbol().as_str() == "push"
            && let hir::ItemContainer::Impl(impl_) = f.container(db)
            && let Some(hir::Adt::Struct(struct_)) = impl_.self_ty(db).as_adt()
            && let None = impl_.trait_(db)
            && (Some(struct_) == self.lang_items.OsString()
                || Some(struct_) == self.lang_items.String())
            && let arg_list = call_expr.arg_list()
            && let receiver = arg_list.args().nth(0).unwrap()
            && let Expr::PathExpr(receiver_path) = receiver
            && let Some(hir::PathResolution::Local(receiver_var)) =
                self.eir_sem.resolve_path(receiver_path.path())
            && let Some(hir::Adt::Struct(struct_of_reciver_var)) = receiver_var.ty(db).as_adt()
            && struct_of_reciver_var == struct_
        {
            replace_node!(
                expr as Expr::CallExpr(call_expr) = {
                    let method_call = call_expr.into_inner();
                    let mut args = method_call.arg_list.into_inner().args.into_iter();
                    let receiver = args.next().unwrap();
                    let operand = args.next().unwrap();
                    Expr::from(new_eir_node!(BinExpr {
                        lhs: receiver,
                        rhs: operand,
                        op_kind: BinaryOp::Assignment {
                            op: Some(ArithOp::Add)
                        },
                        node_info: method_call.node_info.cast(),
                    }))
                }
            );
        }

        stmt.accept_children_mut(self)
    }
}

def_transformer!(
    struct EirFixCowPatterns;
);

impl MutatingEirVisitor for EirFixCowPatterns<'_, '_> {
    type Break = bool;
    fn visit_pat(&mut self, pat: &mut Pat) -> ControlFlow<Self::Break> {
        if let Pat::TupleStructPat(tuple_pat) = pat
            && let Some(hir::PathResolution::Def(def)) =
                self.cg.eir_sem.resolve_path(tuple_pat.path())
            && let hir::ModuleDef::EnumVariant(variant) = def
            && (Some(variant) == self.cg.lang_items.CowBorrowed()
                || Some(variant) == self.cg.lang_items.CowOwned())
        {
            if Some(variant) == self.cg.lang_items.CowBorrowed() {
                replace_node!(
                    pat as Pat::TupleStructPat(tuple_pat) =
                        tuple_pat.into_inner().fields.into_iter().nth(0).unwrap()
                );
            } else {
                return ControlFlow::Break(true);
            }
        }

        pat.accept_children_mut(self)
    }
}
