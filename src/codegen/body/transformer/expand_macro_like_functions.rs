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
        fn should_flatten_enum(
            visitor: &ExpandMacroLikeFunctions<'_, '_>,
            expr: &Expr,
            def: hir::EnumVariant,
        ) -> bool {
            let db = visitor.db;
            if visitor.lang_items.Cow() == Some(def.parent_enum(db)) {
                return true;
            }
            if visitor.lang_items.Either() == Some(def.parent_enum(db))
                && let Some(ty) = visitor.eir_sem.type_of_expr_opt(expr)
                && let original = ty.original()
                && (visitor.trait_first_traits.iter())
                    .any(|&trait_| original.impls_trait(db, trait_, &[]))
            {
                return true;
            }

            false
        }

        if let Expr::CallExpr(call_expr) = expr
            && let callee = call_expr.expr()
            && let Expr::PathExpr(path) = callee
            && let Some((hir::PathResolution::Def(def), _)) =
                self.eir_sem.resolve_path_with_subst(path.path())
            && let Some(ConstructableDef::EnumVariant(def)) = ConstructableDef::from_module_def(def)
            && should_flatten_enum(self, expr, def)
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
            // is a call expr
            && let Expr::CallExpr(call_expr) = expr
            // is non-trait method on some ADT
            && let Expr::PathExpr(path) = call_expr.expr()
            && let Some(hir::PathResolution::Def(hir::ModuleDef::Function(f))) =
            self.eir_sem.resolve_path(path.path())
            && let hir::ItemContainer::Impl(impl_) = f.container(db)
            && let Some(f_adt) = impl_.self_ty(db).as_adt()
            && let None = impl_.trait_(db)
            // is `variable.call()`
            && let arg_list = call_expr.arg_list()
            && let receiver = arg_list.args().nth(0).unwrap()
            && let Expr::PathExpr(receiver_path) = receiver
            && let Some(hir::PathResolution::Local(receiver_var)) =
                self.eir_sem.resolve_path(receiver_path.path())
            && let Some(receiver_adt) = receiver_var.ty(db).as_adt()
            // there is no type change because of deref coleace
            && receiver_adt == f_adt
        {
            if f.name(db).symbol().as_str() == "push"
                && (Some(f_adt) == self.lang_items.OsString().map(hir::Adt::Struct)
                    || Some(f_adt) == self.lang_items.String().map(hir::Adt::Struct))
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
            } else if f.name(db).symbol().as_str() == "clear"
                && (Some(f_adt) == self.lang_items.OsString().map(hir::Adt::Struct)
                    || Some(f_adt) == self.lang_items.String().map(hir::Adt::Struct))
            {
                replace_node!(
                    expr as Expr::CallExpr(call_expr) = {
                        let method_call = call_expr.into_inner();
                        let mut args = method_call.arg_list.into_inner().args.into_iter();
                        let receiver = args.next().unwrap();
                        Expr::from(new_eir_node!(BinExpr {
                            lhs: receiver,
                            rhs: Expr::from(new_eir_node!(RawCodeExpr {
                                code: r##""""##.into(),
                                divergent: false,
                                node_info: NodeInfo::None,
                            })),
                            op_kind: BinaryOp::Assignment { op: None },
                            node_info: method_call.node_info.cast(),
                        }))
                    }
                );
            }
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
