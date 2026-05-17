/// Generates C# expressions and statements from Rust HIR bodies.
use std::collections::HashMap;

use super::{CodeGenerator, names, output::Code};
use crate::codegen::ty::Constructable;
use hir::next_solver::GenericArgs;
use hir::{InFile, Local, StructKind, Variant};
use itertools::Either;
use syntax::ast::{self, AstNode as _, HasArgList as _, HasLoopBody as _, RangeItem as _};
use syntax::ast::{BinaryOp, RangeOp, UnaryOp};

/// Generates C# for the body of a single function.
pub struct BodyGen<'g, 'db> {
    cg: &'g CodeGenerator<'db>,
    /// Mapping from Local to the C# local name allocated for it.
    locals: HashMap<Local, String>,
    /// Counter per original Rust name for uniqueness.
    name_counts: HashMap<String, usize>,
    /// Whether we're inside an async fn (controls .GetAwaiter()/.GetResult() vs await).
    is_async: bool,

    // internals
    match_index: usize,
}

impl<'g, 'db> std::ops::Deref for BodyGen<'g, 'db> {
    type Target = CodeGenerator<'db>;

    fn deref(&self) -> &Self::Target {
        self.cg
    }
}

impl<'g, 'db> BodyGen<'g, 'db> {
    pub fn new(cg: &'g CodeGenerator<'db>, is_async: bool) -> Self {
        Self {
            cg,
            locals: HashMap::new(),
            name_counts: HashMap::new(),
            is_async,
            match_index: 0,
        }
    }

    pub fn expr_location_ast(&self, expr: &impl ast::AstNode) -> String {
        let node = expr.syntax();
        self.location_with_file(InFile::new(self.sem.hir_file_for(node), node.clone()))
    }

    fn alloc_binding_ast(&mut self, local: &Local) -> String {
        let rust_name = local.name(self.db).as_str().to_string();
        let count = self.name_counts.entry(rust_name.clone()).or_insert(0);
        let cs_name = names::local_name(&rust_name, *count);
        *count += 1;
        self.locals.insert(local.clone(), cs_name.clone());
        cs_name
    }

    fn binding_name_ast(&self, local: Local) -> String {
        if let Some(local) = self.locals.get(&local) {
            return local.clone();
        }

        if let Either::Left(pat) = local.primary_source(self.db).source.value
            && let Some(new_local) = self.sem.to_def(&pat)
            && local != new_local
        {
            self.binding_name_ast(new_local)
        } else {
            format!("/* unbound {:?} */unknown", local)
        }
    }

    fn label_name(&self, l: ast::Lifetime) -> String {
        names::camel(l.text().as_str())
    }

    /// Emit the full function body block.
    pub fn emit_function_body(&mut self, f: ast::Fn, out: &mut Code) {
        let params = f.param_list().unwrap();
        if let Some(self_param) = params.self_param() {
            self.locals
                .insert(self.sem.to_def(&self_param).unwrap(), "this".into());
        }
        for param in params.params() {
            match param.pat().unwrap() {
                ast::Pat::IdentPat(ident) if let Some(local) = self.sem.to_def(&ident) => {
                    self.alloc_binding_ast(&local);
                }
                _ => {
                    // TODO
                }
            }
        }
        if let Some(body) = f.body() {
            self.emit_expr_as_stmt_ast(out, ast::Expr::BlockExpr(body), true);
        } else {
            out.wln("throw new System.NotImplementedException(\"builtin-derive\");");
        }
    }

    /// Emit an expression as a statement (with semicolon if needed).
    fn emit_expr_as_stmt_ast(&mut self, out: &mut Code, expr: ast::Expr, is_tail: bool) {
        match expr {
            ast::Expr::BlockExpr(block_expr) => {
                let statements = block_expr.statements();
                let tail = block_expr.tail_expr();
                self.emit_block_contents(out, statements, tail, is_tail);
            }
            ast::Expr::ReturnExpr(ret_expr) => {
                let val = ret_expr
                    .expr()
                    .map(|e| self.emit_expr_str_ast(&e))
                    .unwrap_or_else(|| "0".into());
                out.w("return ").w(val).wln(";");
            }
            ast::Expr::IfExpr(if_expr) => {
                let condition = if_expr.condition().unwrap();
                let then_branch = if_expr.then_branch().unwrap();
                let else_branch = if_expr.else_branch();

                let cond = self.emit_expr_str_ast(&condition);
                out.w("if (").w(cond).wln(") {");
                out.indent();
                self.emit_expr_as_stmt_ast(out, then_branch.into(), false);
                out.dedent();
                match else_branch {
                    Some(ast::ElseBranch::IfExpr(else_if)) => {
                        out.w("} else ");
                        self.emit_expr_as_stmt_ast(out, else_if.into(), is_tail);
                    }
                    Some(ast::ElseBranch::Block(else_e)) => {
                        out.w("} else {");
                        out.wln("");
                        out.indent();
                        self.emit_expr_as_stmt_ast(out, else_e.into(), is_tail);
                        out.dedent();
                        out.wln("}");
                    }
                    None => {}
                }
            }
            ast::Expr::LoopExpr(loop_expr) => {
                let label = loop_expr.label();
                let body = loop_expr.loop_body().unwrap();

                let label_str = label
                    .map(|l| {
                        // TODO?: Should we strip '\''?
                        format!("{}: ", self.label_name(l.lifetime().unwrap()))
                    })
                    .unwrap_or_default();
                out.wln(&format!("{}while (true) {{", label_str));
                out.indent();
                self.emit_expr_as_stmt_ast(out, body.into(), false);
                out.dedent();
                out.wln("}");
            }
            // TODO? While and For
            ast::Expr::MatchExpr(match_expr) => {
                // TODO: replace with switch implementation
                let arms = match_expr.match_arm_list().unwrap();
                let match_expr = match_expr.expr().unwrap();
                let scrutinee = self.emit_expr_str_ast(&match_expr);
                self.match_index += 1;
                out.w("var __match_")
                    .w(self.match_index)
                    .w(" = ")
                    .w(scrutinee)
                    .wln(";");
                let tmp = format!("__match_{:?}", self.match_index).into();
                for (i, arm) in arms.arms().enumerate() {
                    let kw = if i == 0 { "if" } else { "else if" };
                    // Emit pattern check
                    let pat_check = self.emit_pat_check_ast(&tmp, &arm.pat().unwrap());
                    let guard_str = arm
                        .guard()
                        .map(|g| {
                            code!(
                                " && (",
                                self.emit_expr_str_ast(&g.condition().unwrap()),
                                ")"
                            )
                        })
                        .unwrap_or_default();
                    if pat_check == "true".into() && guard_str.is_empty() {
                        out.wln("{");
                    } else {
                        out.w(kw).w(" (").w(pat_check).w(guard_str).wln(") {");
                    }
                    out.indent();
                    // Bind pattern variables
                    self.emit_pat_bindings_ast(out, &tmp, &arm.pat().unwrap());
                    // Emit arm body
                    self.emit_expr_as_stmt_ast(out, arm.expr().unwrap(), is_tail);
                    out.dedent();
                    out.w("}");
                    out.wln("");
                }
            }
            ast::Expr::BreakExpr(break_expr) => {
                let label = break_expr.lifetime();
                let break_expr = break_expr.expr();

                let label_str = label
                    .map(|l| format!(" {}", self.label_name(l)))
                    .unwrap_or_default();
                if let Some(e) = break_expr {
                    let val = self.emit_expr_str_ast(&e);
                    out.w("/* break ").w(val).wln(" */"); // TODO: break-with-value
                } else {
                    out.wln(&format!("break{};", label_str));
                }
            }
            ast::Expr::ContinueExpr(continue_expr) => {
                let label = continue_expr.lifetime();
                let label_str = label
                    .map(|l| format!(" {}", self.label_name(l)))
                    .unwrap_or_default();
                out.wln(&format!("continue{};", label_str));
            }
            _ => {
                // Generic expression: emit as expression statement
                let s = self.emit_expr_str_ast(&expr);
                if !s.is_empty() && s != "()".into() {
                    out.w(s).wln(";");
                }
            }
        }
    }

    fn emit_block_contents(
        &mut self,
        out: &mut Code,
        statements: impl IntoIterator<Item = ast::Stmt>,
        tail: Option<ast::Expr>,
        is_tail: bool,
    ) {
        for stmt in statements {
            match stmt {
                ast::Stmt::LetStmt(let_stmt) => {
                    let pat = let_stmt.pat().unwrap();
                    let _type_ref = let_stmt.ty();
                    let initializer = let_stmt.initializer();
                    let else_branch = let_stmt.let_else().and_then(|x| x.block_expr());

                    let bindings = self.collect_bindings_in_pat_ast(&pat);

                    //*
                    if let Some(init) = initializer {
                        let init_str = self.emit_expr_str_ast(&init);
                        if bindings.len() == 1 {
                            let bid = bindings[0];
                            let cs_name = self.alloc_binding_ast(&bid);
                            let cs_type = self.rust_type_to_cs(&bid.ty(self.db));
                            out.w("var ")
                                .w(cs_name)
                                .w(" = new r2CsRuntime.Slot<")
                                .w(cs_type)
                                .w(">(")
                                .w(init_str)
                                .wln(");");
                        } else if bindings.is_empty() {
                            out.w(init_str).wln(";");
                        } else {
                            self.match_index += 1;
                            let tmp = format!("__tmp_{:?}", self.match_index);
                            out.w("var ").w(&tmp).w(" = ").w(init_str).wln(";");
                            for b in &bindings {
                                let cs_type = self.rust_type_to_cs(&b.ty(self.db));
                                let cs_name = self.alloc_binding_ast(&b);
                                let rust_name = b.name(self.db).as_str().to_string();
                                out.wln(format!(
                                    "var {} = new r2CsRuntime.Slot<{}>({}.{});",
                                    cs_name,
                                    cs_type,
                                    tmp,
                                    names::field_name(&rust_name)
                                ));
                            }
                        }
                    } else {
                        for b in &bindings {
                            let cs_type = self.rust_type_to_cs(&b.ty(self.db));
                            let cs_name = self.alloc_binding_ast(&b);
                            out.wln(format!(
                                "var {} = new r2CsRuntime.Slot<{}>(default!);",
                                cs_name, cs_type
                            ));
                        }
                    }

                    if let Some(_else_e) = else_branch {
                        out.wln("// let-else not fully supported");
                    }
                }
                ast::Stmt::ExprStmt(expr_stmt) => {
                    self.emit_expr_as_stmt_ast(out, expr_stmt.expr().unwrap(), false);
                }
                ast::Stmt::Item(_) => {}
            }
        }

        if let Some(tail_expr) = tail {
            let tail_str = self.emit_expr_str_ast(&tail_expr);
            if is_tail {
                if tail_str != "()".into() && !tail_str.is_empty() {
                    out.w("return ").w(&tail_str).wln(";");
                }
            } else if tail_str != "()".into() && !tail_str.is_empty() {
                out.w(&tail_str).wln(";");
            }
        }
    }

    pub fn emit_expr_str_ast(&mut self, expr: &ast::Expr) -> Code {
        match expr {
            //Expr::Missing => "/* missing */default!".into(),
            ast::Expr::Literal(lit) => self.emit_literal_ast(&lit),
            ast::Expr::PathExpr(path) => {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.sem.resolve_path(&path.path().unwrap())
                })) {
                    Ok(None) => {
                        eprintln!(
                            "Unresolved Path at {loc}",
                            loc = self.expr_location_ast(expr),
                        );
                        fcode!("/* {} */", path.syntax().text().to_string())
                    }
                    Ok(Some(hir::PathResolution::Def(hir::ModuleDef::Function(f)))) => {
                        self.fn_path_cs(f).into()
                    }
                    Ok(Some(hir::PathResolution::Def(hir::ModuleDef::Adt(adt)))) => {
                        // TODO: Generic Args
                        fcode!("new {}", self.rust_type_to_cs(&adt.ty(self.db)))
                    }
                    Ok(Some(hir::PathResolution::SelfType(impl_))) => {
                        eprintln!("ImplSelf at {}", self.expr_location_ast(expr));
                        "(ImplSelf)".into()
                    }
                    Ok(Some(hir::PathResolution::Local(local))) => {
                        self.binding_name_ast(local).into()
                    }
                    //Ok(Some(hir::PathResolution::Def(hir::ModuleDef::Function(function)))) => { // handled above
                    //    self.fn_path_cs(function).into()
                    //}
                    Ok(Some(hir::PathResolution::Def(hir::ModuleDef::Const(const_)))) => {
                        self.const_path_cs(const_).into()
                    }
                    Ok(Some(hir::PathResolution::Def(hir::ModuleDef::Static(static_)))) => {
                        let mut path = self.module_class_cs(static_.module(self.db));
                        path.push('.');
                        path.push_str(static_.name(self.db).as_str());
                        path.into()
                    }
                    //Ok(Some(hir::PathResolution::Def(hir::ModuleDef::Adt(hir::Adt::Enum(enum_))))) => {} // handled above
                    /*
                    Ok(Some(hir::PathResolution::Def(hir::ModuleDef::EnumVariant(variant)))) => {
                        // TODO: Generic Args
                        fcode!(
                            "new {}",
                            self.enum_variant_cs1(
                                variant,
                                GenericArgs::error_for_item(
                                    self.interner,
                                    hir_def::EnumVariantId::from(variant).into()
                                )
                            )
                        )
                    }
                     */
                    Ok(Some(hir::PathResolution::Def(hir::ModuleDef::EnumVariant(variant)))) => {
                        // TODO: Generic Args
                        match variant.kind(self.db) {
                            StructKind::Unit => {
                                fcode!(
                                    "{}.instance",
                                    self.enum_variant_cs1(
                                        variant,
                                        GenericArgs::empty(self.interner) // GenericArgs::error_for_item(self.interner,hir_def::EnumVariantId::from(variant).into())
                                    )
                                )
                            }
                            StructKind::Tuple => {
                                fcode!(
                                    "new {}",
                                    self.enum_variant_cs1(
                                        variant,
                                        GenericArgs::empty(self.interner) //GenericArgs::error_for_item(self.interner,hir_def::EnumVariantId::from(variant).into())
                                    )
                                )
                            }
                            kind => {
                                eprintln!(
                                    "Unexpected struct kind and infer for enum variant path at {loc}\n\
                                        \tkind: {kind:?}",
                                    loc = self.expr_location_ast(path),
                                    //type = self.new_type(infer).display(
                                    //    self.db,
                                    //    self.krate.to_display_target(self.db)
                                    //)
                                );

                                fcode!(
                                    "new /* unexpected struct kind and infer */ {}",
                                    self.enum_variant_cs(variant)
                                )
                            }
                        }
                    }
                    Ok(Some(hir::PathResolution::TypeParam(param))) => {
                        eprintln!("GenericParam at {}", self.expr_location_ast(expr));
                        "(GenericParam)".into()
                    }

                    //Ok(Some(hir::PathResolution::Def(hir::ModuleDef::EnumVariant(variant)))) => {}
                    Ok(Some(resolved)) => {
                        eprintln!(
                            "Path at {loc}: {resolved:?}",
                            loc = self.expr_location_ast(expr),
                        );
                        fcode!("/* {} */", path.syntax().text().to_string())
                    }
                    /*

                    Some((
                        ResolveValueResult::Partial(
                            ty @ (TypeNs::AdtId(_) | TypeNs::BuiltinType(_) | TypeNs::SelfType(_)),
                            i,
                        ),
                        _,
                    )) if i == path.segments().len() - 1 => {
                        let name = path.segments().last().unwrap().name;

                        let ty = match ty {
                            TypeNs::AdtId(adt) => hir::Adt::from(adt).ty(self.db),
                            TypeNs::BuiltinType(builtin) => {
                                hir::BuiltinType::from(builtin).ty(self.db)
                            }
                            TypeNs::SelfType(s) => hir::Impl::from(s).self_ty(self.db),
                            _ => unreachable!(),
                        };
                        for impl_ in hir::Impl::all_for_type(self.db, ty) {
                            if impl_.trait_(self.db).is_none()
                                && let Some(&item) = impl_
                                    .items(self.db)
                                    .iter()
                                    .find(|i| i.name(self.db).as_ref() == Some(name))
                            {
                                return match item {
                                    AssocItem::Function(f) => self.fn_path_cs(f).into(),
                                    AssocItem::Const(c) => self.const_path_cs(c).into(),
                                    AssocItem::TypeAlias(a) => {
                                        self.rust_type_to_cs(&a.ty(self.db)).into()
                                    }
                                };
                            }
                        }

                        eprintln!(
                            "Unresolved Partial path {:?} at {} ({path:?})",
                            self.source(expr_id),
                            self.expr_location(expr_id)
                        );
                        "(PartialPath)".into()
                    }
                    Some((ResolveValueResult::Partial(p, x), inf)) => {
                        eprintln!(
                            "Partial path {:?} ({p:?}, {x:?}, {inf:?}) at {} ({path:?})",
                            self.source(expr_id),
                            self.expr_location(expr_id)
                        );
                        "(PartialPath)".into()
                    }
                    None => {
                        eprintln!("Failed to resolve path at {}", self.expr_location(expr_id));
                        "(UnknownPath)".into()
                    }
                     */
                    Err(panic) => {
                        eprintln!(
                            "Path at {loc}: {panic:?}",
                            loc = self.expr_location_ast(expr),
                        );
                        std::panic::resume_unwind(panic);
                    }
                }
            }
            ast::Expr::FieldExpr(field_expr) => {
                let name = field_expr.name_ref().unwrap();
                let receiver_part = field_expr.expr().unwrap();
                let receiver = self.emit_expr_str_ast(&receiver_part);
                match self.sem.resolve_field(&field_expr) {
                    None => {
                        eprintln!("Unresolved field at {}", self.expr_location_ast(field_expr));
                        code!(receiver, "./* unresolved field */.value")
                    }
                    Some(Either::Left(field)) => {
                        code!(receiver, ".", self.field_name(&field), ".value")
                    }
                    Some(Either::Right(field)) => {
                        code!(receiver, ".", field.index + 1)
                    }
                }
            }
            ast::Expr::MethodCallExpr(method_call) => {
                let receiver = self.emit_expr_str_ast(&method_call.receiver().unwrap());
                match self.sem.resolve_method_call(method_call) {
                    Some(resolved) => {
                        let method_cs = self.function_name(&resolved);
                        let args = method_call
                            .arg_list()
                            .unwrap()
                            .args()
                            .map(|a| self.emit_expr_str_ast(&a));
                        code!(receiver, ".", method_cs, "(", join(args, ", "), ")")
                    }
                    None => {
                        eprintln!(
                            "Unresolved method call at {}",
                            self.expr_location_ast(method_call)
                        );
                        let method_cs = method_call.name_ref().unwrap().text().as_str().to_string();
                        let args = method_call
                            .arg_list()
                            .unwrap()
                            .args()
                            .map(|a| self.emit_expr_str_ast(&a));
                        code!(receiver, ".", method_cs, "(", join(args, ", "), ")")
                    }
                }
            }
            ast::Expr::CallExpr(call_expr) => {
                let callee_str = self.emit_expr_str_ast(&call_expr.expr().unwrap());
                let args_str = call_expr
                    .arg_list()
                    .unwrap()
                    .args()
                    .map(|a| self.emit_expr_str_ast(&a));
                code!(callee_str, "(", join(args_str, ", "), ")")
            }
            ast::Expr::AwaitExpr(await_expr) => {
                let inner_str = self.emit_expr_str_ast(&await_expr.expr().unwrap());
                code!("(await ", inner_str, ")")
            }
            ast::Expr::BinExpr(bin_expr) => {
                let lhs_code = self.emit_expr_str_ast(&bin_expr.lhs().unwrap());
                let rhs_code = self.emit_expr_str_ast(&bin_expr.rhs().unwrap());
                let op_str = match bin_expr.op_kind() {
                    None => "/* op= */ =".to_string(),
                    Some(BinaryOp::ArithOp(a)) => format!("{:?}", a)
                        .to_lowercase()
                        .replace("add", "+")
                        .replace("sub", "-")
                        .replace("mul", "*")
                        .replace("div", "/")
                        .replace("rem", "%")
                        .replace("shl", "<<")
                        .replace("shr", ">>")
                        .replace("bitxor", "^")
                        .replace("bitor", "|")
                        .replace("bitand", "&"),
                    Some(BinaryOp::CmpOp(c)) => {
                        use hir_def::hir::{CmpOp, Ordering};
                        match c {
                            CmpOp::Eq { negated: false } => "==".to_string(),
                            CmpOp::Eq { negated: true } => "!=".to_string(),
                            CmpOp::Ord {
                                ordering: Ordering::Less,
                                strict: true,
                            } => "<".to_string(),
                            CmpOp::Ord {
                                ordering: Ordering::Less,
                                strict: false,
                            } => "<=".to_string(),
                            CmpOp::Ord {
                                ordering: Ordering::Greater,
                                strict: true,
                            } => ">".to_string(),
                            CmpOp::Ord {
                                ordering: Ordering::Greater,
                                strict: false,
                            } => ">=".to_string(),
                        }
                    }
                    Some(BinaryOp::LogicOp(l)) => {
                        use hir_def::hir::LogicOp;
                        match l {
                            LogicOp::And => "&&".to_string(),
                            LogicOp::Or => "||".to_string(),
                        }
                    }
                    Some(BinaryOp::Assignment { op: None }) => "=".to_string(),
                    Some(BinaryOp::Assignment { op: Some(a) }) => format!("{:?}=", a)
                        .to_lowercase()
                        .replace("add", "+=")
                        .replace("sub", "-=")
                        .replace("mul", "*=")
                        .replace("div", "/=")
                        .replace("rem", "%="),
                };
                code!("(", lhs_code, " ", op_str, " ", rhs_code, ")")
            }
            //Expr::Assignment { target, value } => {
            //    let target_str = self.emit_pat_as_lvalue(*target);
            //    let value_str = self.emit_expr_str(*value);
            //    code!(target_str, " = ", value_str)
            //}
            ast::Expr::PrefixExpr(prefix_expr) => {
                let inner_str = self.emit_expr_str_ast(&prefix_expr.expr().unwrap());
                match prefix_expr.op_kind().unwrap() {
                    UnaryOp::Deref => code!("(*", inner_str, ")"),
                    UnaryOp::Not => code!("!(", inner_str, ")"),
                    UnaryOp::Neg => code!("-(", inner_str, ")"),
                }
            }
            ast::Expr::RefExpr(ref_expr) => self.emit_expr_str_ast(&ref_expr.expr().unwrap()),
            //Expr::Box { expr: inner } => self.emit_expr_str(*inner),
            ast::Expr::CastExpr(cast_expr) => {
                code!(
                    "(/* cast */) ",
                    self.emit_expr_str_ast(&cast_expr.expr().unwrap())
                )
            }
            ast::Expr::IfExpr(if_expr) => match if_expr.else_branch() {
                Some(ast::ElseBranch::Block(else_block)) => {
                    let cond = self.emit_expr_str_ast(&if_expr.condition().unwrap());
                    let then_s = self.emit_expr_str_ast(&if_expr.then_branch().unwrap().into());
                    let else_s = self.emit_expr_str_ast(&else_block.into());
                    code!("(", cond, " ? ", then_s, " : ", else_s, ")")
                }
                Some(ast::ElseBranch::IfExpr(if_expr)) => {
                    let cond = self.emit_expr_str_ast(&if_expr.condition().unwrap());
                    let then_s = self.emit_expr_str_ast(&if_expr.then_branch().unwrap().into());
                    let else_s = self.emit_expr_str_ast(&if_expr.into());
                    code!("(", cond, " ? ", then_s, " : ", else_s, ")")
                }
                None => {
                    let cond = self.emit_expr_str_ast(&if_expr.condition().unwrap());
                    let then_s = self.emit_expr_str_ast(&if_expr.then_branch().unwrap().into());
                    code!("(", cond, " ? ", then_s, " : ValueTuple)")
                }
            },
            ast::Expr::BlockExpr(block_expr) => {
                if block_expr.statements().count() == 0 {
                    if let Some(t) = block_expr.tail_expr() {
                        return self.emit_expr_str_ast(&t);
                    }
                    return "default!".into();
                }
                "/* block expr */ default!".into()
            }
            ast::Expr::TupleExpr(tuple_expr) => {
                if tuple_expr.fields().next().is_none() {
                    "/* unit */0".into()
                } else {
                    let parts = tuple_expr.fields().map(|e| self.emit_expr_str_ast(&e));
                    code!("(", join(parts, ", "), ")")
                }
            }
            ast::Expr::RecordExpr(record_expr) => {
                let c = match self.sem.resolve_variant(record_expr.clone()) {
                    None => {
                        eprintln!(
                            "Struct construction without type in {}",
                            self.expr_location_ast(expr)
                        );

                        return fcode!(
                            "(TypelessConstruction/*{:?}*/)",
                            record_expr.syntax().text().to_string()
                        );
                    }
                    Some(hir::Variant::Struct(struct_ty)) => Constructable::Struct(struct_ty),
                    Some(hir::Variant::EnumVariant(variant)) => Constructable::EnumVariant(variant),
                    Some(hir::Variant::Union(_)) => {
                        eprintln!("Union unsupported at {}", self.expr_location_ast(expr));

                        return fcode!(
                            "(UnionConstruction/*{:?}*/)",
                            record_expr.syntax().text().to_string()
                        );
                    }
                };
                let type_name = self.constructable_name_cs(c);

                let field_inits = record_expr
                    .record_expr_field_list()
                    .unwrap()
                    .fields()
                    .map(|f| {
                        let cs_f = names::field_name(f.field_name().unwrap().text().as_str());
                        let field = c.fields(self.db).into_iter().find(|x| {
                            x.name(self.db).as_str() == f.field_name().unwrap().text().as_str()
                        });
                        let cs_ty = field
                            .map(|f| self.rust_type_to_cs(&f.ty(self.db).to_type(self.db)))
                            .unwrap_or_else(|| "object /*unknown field type*/".into());
                        let val = self.emit_expr_str_ast(&f.expr().unwrap());
                        code!(cs_f, " = new r2CsRuntime.Slot<", cs_ty, ">(", val, ")")
                    });
                code!(
                    "new ",
                    type_name,
                    "() {\n",
                    indent,
                    join(field_inits, ", \n"),
                    "\n",
                    dedent,
                    "}"
                )
            }
            ast::Expr::IndexExpr(index_expr) => {
                let base_str = self.emit_expr_str_ast(&index_expr.base().unwrap());
                let idx_str = self.emit_expr_str_ast(&index_expr.index().unwrap());
                code!(base_str, "[", idx_str, "]")
            }
            ast::Expr::RangeExpr(range_expr) => {
                let start_code = range_expr
                    .start()
                    .map(|e| self.emit_expr_str_ast(&e))
                    .unwrap_or_default();
                let end_code = range_expr
                    .end()
                    .map(|e| self.emit_expr_str_ast(&e))
                    .unwrap_or_default();
                let dots = match range_expr.op_kind().unwrap() {
                    RangeOp::Exclusive => "..",
                    RangeOp::Inclusive => "..=",
                };
                code!(
                    "/* range ",
                    start_code,
                    dots,
                    end_code,
                    " */ new s_Range(",
                    start_code,
                    ", ",
                    end_code,
                    ")"
                )
            }
            ast::Expr::ArrayExpr(array) => match array.kind() {
                ast::ArrayExprKind::Repeat {
                    initializer,
                    repeat,
                } => {
                    let val = self.emit_expr_str_ast(&initializer.unwrap());
                    let len = self.emit_expr_str_ast(&repeat.unwrap());
                    code!("new object[", len, "] /* fill ", val, "*/")
                }
                ast::ArrayExprKind::ElementList(elements) => {
                    let parts = elements.map(|e| self.emit_expr_str_ast(&e));
                    code!("new object[] { ", join(parts, ", "), " }")
                }
            },
            ast::Expr::ClosureExpr(closure) => {
                let params: Vec<String> = closure
                    .param_list()
                    .unwrap()
                    .params()
                    .enumerate()
                    .map(|(i, p)| {
                        let bindings = self.collect_bindings_in_pat_ast(&p.pat().unwrap());
                        if bindings.len() == 1 {
                            names::local_name(bindings[0].name(self.db).as_str(), 0)
                        } else {
                            format!("__cp{}", i)
                        }
                    })
                    .collect();
                for p in closure.param_list().unwrap().params() {
                    let bindings = self.collect_bindings_in_pat_ast(&p.pat().unwrap());
                    for b in bindings {
                        self.alloc_binding_ast(&b); // TODO
                    }
                }
                let body_str = self.emit_expr_str_ast(&closure.body().unwrap());
                code!("(", join(params, ", "), ") => ", body_str)
            }
            ast::Expr::ReturnExpr(return_expr) => {
                let val = return_expr
                    .expr()
                    .map(|e| self.emit_expr_str_ast(&e))
                    .unwrap_or_else(|| "0 /* unit */".into());
                code!(
                    "/* return-expr */ throw r2CsRuntime.Helpers.Returns<object>(",
                    val,
                    ")"
                )
            }
            ast::Expr::LetExpr(let_expr) => {
                let val = self.emit_expr_str_ast(&let_expr.expr().unwrap());
                let check = self.emit_pat_check_ast(&val, &let_expr.pat().unwrap());
                check
            }
            /*
            Expr::Unsafe {
                statements, tail, ..
            } => {
                if statements.is_empty() {
                    if let &Some(t) = tail {
                        return self.emit_expr_str(t);
                    }
                    return "default!".into();
                }
                "/* unsafe block */ default!".into()
            }
             */
            ast::Expr::MatchExpr(match_expr) => "/* block-expr-value */ default!".into(),
            ast::Expr::BreakExpr(break_expr) => break_expr
                .expr()
                .map(|e| self.emit_expr_str_ast(&e))
                .unwrap_or_else(|| "default!".into()),
            ast::Expr::ContinueExpr(_) => "default!".into(),
            ast::Expr::BecomeExpr(become_expr) => {
                self.emit_expr_str_ast(&become_expr.expr().unwrap())
            }
            ast::Expr::YieldExpr(yield_expr) => yield_expr
                .expr()
                .map(|e| self.emit_expr_str_ast(&e))
                .unwrap_or_else(|| "default!".into()),
            //&Expr::Const(inner) => self.emit_expr_str(inner),
            ast::Expr::UnderscoreExpr(_) => "_".into(),
            ast::Expr::ParenExpr(paran) => {
                code!("(", self.emit_expr_str_ast(&paran.expr().unwrap()), ")")
            }
            ast::Expr::OffsetOfExpr(_) | ast::Expr::AsmExpr(_) => {
                "/* asm/offsetof */ default!".into()
            }
            ast::Expr::LoopExpr(_)
            | ast::Expr::ForExpr(_)
            | ast::Expr::MacroExpr(_)
            | ast::Expr::WhileExpr(_)
            | ast::Expr::FormatArgsExpr(_) => {
                // TODO
                "/* loop */ default!".into()
            }

            ast::Expr::YeetExpr(_) => {
                panic!("yeet not supported at {}", self.expr_location_ast(expr))
            }
            ast::Expr::TryExpr(try_expr) => {
                code!(
                    "Try(",
                    self.emit_expr_str_ast(&try_expr.expr().unwrap()),
                    ")"
                )
            }
        }
    }

    /// Emit a tuple-struct or enum-variant constructor call.
    fn emit_constructor_call(&mut self, variant_id: Variant, args: &[ast::Expr]) -> Code {
        match variant_id {
            Variant::Struct(s) => {
                let cs_name = names::struct_name(s.name(self.db).as_str());
                let fields = s.fields(self.db);
                if fields.is_empty() || args.is_empty() {
                    return fcode!("new {}()", cs_name);
                }
                let inits = self.build_positional_field_inits(&fields, args);
                code!(format("new {cs_name}()"), "{", join(inits, ", "), "}")
            }
            Variant::EnumVariant(v) => {
                let cs_name = names::variant_name(v.name(self.db).as_str());
                let fields = v.fields(self.db);
                if fields.is_empty() || args.is_empty() {
                    return fcode!("new {}()", cs_name);
                }
                let inits = self.build_positional_field_inits(&fields, args);
                code!(format("new {cs_name}()"), "{", join(inits, ", "), "}")
            }
            Variant::Union(_) => "default! /* union ctor */".into(),
        }
    }

    fn build_positional_field_inits(
        &mut self,
        fields: &[hir::Field],
        args: &[ast::Expr],
    ) -> Vec<Code> {
        fields
            .iter()
            .zip(args.iter())
            .map(|(field, arg)| {
                let field_ty = field.ty(self.db).to_type(self.db);
                let cs_ty = {
                    let t = self.rust_type_to_cs(&field_ty);
                    if t == "void" { "object".to_string() } else { t }
                };
                let f_name = names::field_name(field.name(self.db).as_str());
                let val = self.emit_expr_str_ast(arg);
                code!(f_name, " = new r2CsRuntime.Slot<", cs_ty, ">(", val, ")")
            })
            .collect()
    }

    fn emit_literal_ast(&self, lit: &ast::Literal) -> Code {
        match lit.kind() {
            ast::LiteralKind::Bool(b) => b.to_string().into(),
            ast::LiteralKind::IntNumber(v) => v.value().unwrap().to_string().into(),
            ast::LiteralKind::FloatNumber(v) => v.to_string().into(),
            ast::LiteralKind::Char(c) => fcode!("'{}'", c.value().unwrap().escape_default()),
            ast::LiteralKind::String(s) => fcode!("\"{}\"", s.value().unwrap().escape_default(),),
            ast::LiteralKind::Byte(b) => b.value().unwrap().to_string().into(),
            ast::LiteralKind::ByteString(_) => "/* byte string */ new byte[] {}".into(),
            ast::LiteralKind::CString(_) => "/* cstring */ \"\"".into(),
        }
    }

    /// Emit a pattern as a condition check against a scrutinee expression.
    fn emit_pat_check_ast(&mut self, scrutinee: &Code, pat: &ast::Pat) -> Code {
        match pat {
            ast::Pat::WildcardPat(w) => "true".into(),
            ast::Pat::IdentPat(ident_pat)
                if let Some(const_ref) = self.sem.resolve_bind_pat_to_const(ident_pat) =>
            {
                // TODO
                code!(
                    "/*TODO: const pat*/",
                    scrutinee,
                    " is ",
                    format!("{:?}", const_ref)
                )
            }
            ast::Pat::IdentPat(ident_pat) => {
                if let Some(sub) = ident_pat.pat() {
                    self.emit_pat_check_ast(scrutinee, &sub)
                } else {
                    "true".into()
                }
            }
            ast::Pat::TupleStructPat(tuple_struct) => {
                // TODO
                let p = tuple_struct.path().unwrap();
                let segs: Vec<String> = p
                    .segments()
                    .map(|s| s.name_ref().unwrap().text().as_str().to_string())
                    .collect();
                let variant_name = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                let cs_variant = names::variant_name(&variant_name);
                code!(scrutinee, " is ", cs_variant)
            }
            ast::Pat::PathPat(path) => {
                let path = path.path().unwrap();
                let segs: Vec<String> = path
                    .segments()
                    .map(|s| s.name_ref().unwrap().text().as_str().to_string())
                    .collect();
                if segs.len() > 1 {
                    let variant = segs.last().unwrap();
                    let cs_variant = names::variant_name(variant);
                    code!(scrutinee, " is ", cs_variant)
                } else {
                    "true".into()
                }
            }
            ast::Pat::RecordPat(record_pat) => {
                //Pat::Record { path, args, .. } => {
                let segs: Vec<String> = record_pat
                    .path()
                    .unwrap()
                    .segments()
                    .map(|s| s.name_ref().unwrap().text().as_str().to_string())
                    .collect();
                let variant = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                let cs_variant = names::variant_name(&variant);
                code!(scrutinee, " is ", cs_variant)
            }
            ast::Pat::LiteralPat(literal_pat) => {
                let lit_str =
                    self.emit_expr_str_ast(&ast::Expr::Literal(literal_pat.literal().unwrap()));
                code!(scrutinee, " == ", lit_str)
            }
            ast::Pat::OrPat(or_pat) => {
                let parts: Vec<Code> = or_pat
                    .pats()
                    .map(|p| self.emit_pat_check_ast(scrutinee, &p))
                    .collect();
                code!("(", join(parts, " || "), ")")
            }
            ast::Pat::TuplePat(tuple_pat) => {
                let checks: Vec<Code> = tuple_pat
                    .fields()
                    .enumerate()
                    .map(|(i, p)| {
                        let sub_scrutinee = code!(scrutinee, ".Item", (i + 1).to_string());
                        self.emit_pat_check_ast(&sub_scrutinee, &p)
                    })
                    .collect();
                let combined: Vec<Code> = checks.into_iter().collect();
                if combined.is_empty() {
                    "true".into()
                } else {
                    code!("(", join(combined, " && "), ")")
                }
            }
            _ => "true".into(),
        }
    }

    /// Emit variable binding statements for a pattern matched against a scrutinee.
    fn emit_pat_bindings_ast(&mut self, out: &mut Code, scrutinee: &Code, pat: &ast::Pat) {
        match pat {
            ast::Pat::IdentPat(ident_pat)
                if let Some(_) = self.sem.resolve_bind_pat_to_const(ident_pat) =>
            {
                // nothing to do
            }
            ast::Pat::IdentPat(ident_pat) => {
                let Some(local) = self.sem.to_def(ident_pat) else {
                    panic!("Ident Pat has no Local")
                };

                let cs_name = self.alloc_binding_ast(&local);
                let cs_type = self.rust_type_to_cs(&local.ty(self.db));
                out.w("var ")
                    .w(cs_name)
                    .w(" = new r2CsRuntime.Slot<")
                    .w(cs_type)
                    .w(">(")
                    .w(scrutinee)
                    .wln(");");
                if let Some(sub) = ident_pat.pat() {
                    self.emit_pat_bindings_ast(out, scrutinee, &sub);
                }
            }
            ast::Pat::TupleStructPat(tuple_pat) => {
                if let p = tuple_pat.path().unwrap() {
                    let segs: Vec<String> = p
                        .segments()
                        .map(|s| s.name_ref().unwrap().text().as_str().to_string())
                        .collect();
                    let variant = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                    let cs_variant = names::variant_name(&variant);
                    let tmp = format!("__ts_{}", cs_variant);
                    out.w("var ")
                        .w(&tmp)
                        .w(" = (")
                        .w(cs_variant)
                        .w(") ")
                        .w(scrutinee)
                        .wln(";");
                    for (i, sub_pat) in tuple_pat.fields().enumerate() {
                        let sub_scrutinee = format!("{}.f_{}.value", tmp, i);
                        self.emit_pat_bindings_ast(out, &sub_scrutinee.into(), &sub_pat);
                    }
                }
            }
            ast::Pat::RecordPat(tuple_pat) => {
                if let p = tuple_pat.path().unwrap() {
                    let segs: Vec<String> = p
                        .segments()
                        .map(|s| s.name_ref().unwrap().text().as_str().to_string())
                        .collect();
                    let variant = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                    let cs_variant = names::variant_name(&variant);
                    let tmp = format!("__ts_{}", cs_variant);
                    out.w("var ")
                        .w(&tmp)
                        .w(" = (")
                        .w(cs_variant)
                        .w(") ")
                        .w(scrutinee)
                        .wln(";");
                    for (i, sub_pat) in tuple_pat
                        .record_pat_field_list()
                        .unwrap()
                        .fields()
                        .enumerate()
                    {
                        let sub_scrutinee = format!("{}.f_{}.value", tmp, i);
                        self.emit_pat_bindings_ast(
                            out,
                            &sub_scrutinee.into(),
                            &sub_pat.pat().unwrap(),
                        );
                    }
                }
            }

            ast::Pat::TupleStructPat(tuple_pat) => {
                for (i, sub_pat) in tuple_pat.fields().enumerate() {
                    let sub_scrutinee = code!(scrutinee, ".Item", i + 1);
                    self.emit_pat_bindings_ast(out, &sub_scrutinee, &sub_pat);
                }
            }
            _ => {}
        }
    }

    /// Emit a pattern as an lvalue (for assignment).
    fn emit_pat_as_lvalue_ast(&self, pat: &ast::Pat) -> String {
        match pat {
            ast::Pat::IdentPat(ident_pat) if let Some(local) = self.sem.to_def(ident_pat) => {
                let cs_name = self
                    .locals
                    .get(&local)
                    .cloned()
                    .unwrap_or_else(|| "/* unbound */unknown".to_string());
                format!("{}.value", cs_name)
            }
            ast::Pat::WildcardPat(_) => "_".to_string(),
            _ => "/* complex lvalue */unknown".to_string(),
        }
    }

    /// Collect all binding IDs in a pattern.
    fn collect_bindings_in_pat_ast(&self, pat: &ast::Pat) -> Vec<Local> {
        let mut result = Vec::new();
        self.collect_bindings_recursive_ast(pat, &mut result);
        result
    }

    fn collect_bindings_recursive_ast(&self, pat: &ast::Pat, result: &mut Vec<Local>) {
        match pat {
            ast::Pat::IdentPat(ident_pat) if let Some(local) = self.sem.to_def(ident_pat) => {
                result.push(local);
                if let Some(sub) = ident_pat.pat() {
                    self.collect_bindings_recursive_ast(&sub, result);
                }
            }
            _ => {
                for child in pat.syntax().children().filter_map(ast::Pat::cast) {
                    self.collect_bindings_recursive_ast(&child, result);
                }
            }
        }
    }

    fn find_local_by_rust_name(&self, rust_name: &str) -> Option<String> {
        for (local, cs_name) in &self.locals {
            if local.name(self.db).as_str() == rust_name {
                return Some(cs_name.to_string());
            }
        }
        None
    }
}
