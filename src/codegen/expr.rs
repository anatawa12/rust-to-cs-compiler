/// Generates C# expressions and statements from Rust HIR bodies.
use std::collections::HashMap;

use super::{CodeGenerator, generic_args, names, output::Code};
use crate::codegen::ty::{Constructable, ConstructableDef, DebugDisplay, TypeExt};
use hir::{
    HasContainer, InFile, ItemContainer, Local, ModuleDef, PathResolution, StructKind, Variant,
};
use hir_ty::db::HirDatabase;
use hir_ty::display::HirDisplay;
use itertools::Either;
use rustc_type_ir::Upcast;
use syntax::ast::{
    self, ArithOp, AstNode as _, HasArgList as _, HasLoopBody as _, HasName, LogicOp,
    RangeItem as _,
};
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

    pub deferred: Vec<ItemInBody>,
}

/// Items emitting are deferred.
/// Caller must generate those items.
#[derive(Debug, Copy, Clone)]
pub enum ItemInBody {
    Function(hir::Function),
    Adt(hir::Adt),
    Impl(hir::Impl),
}

impl_from!(hir::Function, hir::Adt(hir::Enum, hir::Struct), hir::Impl for ItemInBody);

impl hir::HasContainer for ItemInBody {
    fn container(&self, db: &dyn HirDatabase) -> hir::ItemContainer {
        match self {
            ItemInBody::Function(i) => hir::HasContainer::container(i, db),
            ItemInBody::Adt(hir::Adt::Struct(i)) => hir::HasContainer::container(i, db),
            ItemInBody::Adt(hir::Adt::Enum(i)) => hir::HasContainer::container(i, db),
            ItemInBody::Adt(hir::Adt::Union(i)) => hir::HasContainer::container(i, db),
            ItemInBody::Impl(i) => hir::ItemContainer::Module(i.module(db)),
        }
    }
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
            deferred: Vec::new(),
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
        names::camel(&l.text().as_str()[1..])
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
            self.emit_expr_as_stmt_ast(out, ast::Expr::BlockExpr(body), true, true);
        } else {
            out.wln("throw new System.NotImplementedException(\"builtin-derive\");");
        }
    }

    /// Emit an expression as a statement (with semicolon if needed).
    fn emit_expr_as_stmt_ast(
        &mut self,
        out: &mut Code,
        expr: ast::Expr,
        is_tail: bool,
        returning: bool,
    ) {
        match expr {
            ast::Expr::BlockExpr(block_expr) => {
                let statements = block_expr.statements();
                let tail = block_expr.tail_expr();
                self.emit_block_contents(out, statements, tail, is_tail, returning);
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
                self.emit_expr_as_stmt_ast(out, then_branch.into(), is_tail, returning);
                out.dedent();
                match else_branch {
                    Some(ast::ElseBranch::IfExpr(else_if)) => {
                        out.w("} else ");
                        self.emit_expr_as_stmt_ast(out, else_if.into(), is_tail, returning);
                    }
                    Some(ast::ElseBranch::Block(else_e)) => {
                        out.w("} else {");
                        out.wln("");
                        out.indent();
                        self.emit_expr_as_stmt_ast(out, else_e.into(), is_tail, returning);
                        out.dedent();
                        out.wln("}");
                    }
                    None => {
                        out.wln("}");
                    }
                }
            }
            ast::Expr::LoopExpr(loop_expr) => {
                let label = loop_expr.label();
                let body = loop_expr.loop_body().unwrap();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(l.lifetime().unwrap())))
                    .unwrap_or_default();
                out.w(label_str).wln("while (true) {");
                out.indent();
                self.emit_expr_as_stmt_ast(out, body.into(), false, false);
                out.dedent();
                out.wln("}");
            }
            ast::Expr::WhileExpr(while_expr) => {
                let label = while_expr.label();
                let condition = while_expr.condition().unwrap();
                let body = while_expr.loop_body().unwrap();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(l.lifetime().unwrap())))
                    .unwrap_or_default();
                out.w(label_str)
                    .w("while (")
                    .w(self.emit_expr_str_ast(&condition))
                    .wln(") {");
                out.indent();
                self.emit_expr_as_stmt_ast(out, body.into(), false, false);
                out.dedent();
                out.wln("}");
            }
            ast::Expr::ForExpr(for_expr) => {
                let label = for_expr.label();
                let pat = for_expr.pat().unwrap();
                let iterable = for_expr.iterable().unwrap();
                let body = for_expr.loop_body().unwrap();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(l.lifetime().unwrap())))
                    .unwrap_or_default();
                let temp_name = {
                    let match_index = self.match_index;
                    self.match_index += 1;
                    format!("__temp_{}", match_index)
                };
                out.w(label_str)
                    .w("foreach (var ")
                    .w(&temp_name)
                    .w(" in ")
                    .w(self.emit_expr_str_ast(&iterable))
                    .wln(") {");
                out.indent();
                self.emit_let_stmt(out, &pat, code!(&temp_name), Self::emit_unreachable);
                self.emit_expr_as_stmt_ast(out, body.into(), false, false);
                out.dedent();
                out.wln("}");
            }
            // TODO? While and For
            ast::Expr::MatchExpr(match_expr) => {
                let arms = match_expr.match_arm_list().unwrap();
                let match_expr = match_expr.expr().unwrap();
                let scrutinee = self.emit_expr_str_ast(&match_expr);
                out.w("switch (").w(scrutinee).wln(") {");
                out.indent();

                for arm in arms.arms() {
                    // Emit pattern check
                    let pat_cs = self.emit_pattern_ast(&arm.pat().unwrap());

                    out.w("case ").w(pat_cs).w(" ");
                    if let Some(guard) = arm.guard() {
                        out.w("when ");
                        out.w(self.emit_expr_str_ast(&guard.condition().unwrap()));
                    }
                    out.wln(":{");
                    out.indent();
                    self.emit_expr_as_stmt_ast(out, arm.expr().unwrap(), is_tail, returning);
                    out.wln("break;");
                    out.dedent();
                    out.wln("}");
                }
                out.dedent();

                out.wln("}");
            }
            ast::Expr::BreakExpr(break_expr) => {
                let label = break_expr.lifetime();
                let break_expr = break_expr.expr();

                // TODO: Labelled break
                let label_str = label
                    .map(|l| format!(" /*{}*/", self.label_name(l)))
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
                // TODO: Labelled continue
                let label_str = label
                    .map(|l| format!(" /*{}*/", self.label_name(l)))
                    .unwrap_or_default();
                out.wln(&format!("continue{};", label_str));
            }

            ast::Expr::AwaitExpr(await_expr) => {
                let inner_str = self.emit_expr_str_ast(&await_expr.expr().unwrap());
                out.wln(code!("await ", inner_str, ";"));
            }
            _ => {
                // Generic expression: emit as expression statement
                let s = self.emit_expr_str_ast_inner(&expr, true);
                if !s.is_empty() && s != "()".into() {
                    out.w(s).wln(";");
                }
            }
        }
    }

    fn emit_let_stmt(
        &mut self,
        out: &mut Code,
        pat: &ast::Pat,
        value_cs: Code,
        else_gen: impl FnOnce(&mut BodyGen<'g, 'db>, &mut Code),
    ) {
        if let ast::Pat::IdentPat(ident_pat) = pat {
            let local = self.sem.to_def(ident_pat).unwrap();
            let cs_local_name = self.alloc_binding_ast(&local);
            out.w("var ").w(cs_local_name).w(" = ").w(value_cs).wln(";");
        } else {
            let pattern_cs = self.emit_pattern_ast(pat);

            out.w("if (!(").w(value_cs).w(" is ").w(pattern_cs).w(")) ");
            else_gen(self, out);
        }
    }

    fn emit_unreachable(&mut self, out: &mut Code) {
        out.wln("throw new Exception(\"unreachable\");");
    }

    fn emit_block_contents(
        &mut self,
        out: &mut Code,
        statements: impl IntoIterator<Item = ast::Stmt>,
        tail: Option<ast::Expr>,
        is_tail: bool,
        returning: bool,
    ) {
        for stmt in statements {
            match stmt {
                ast::Stmt::LetStmt(let_stmt) => {
                    let pat = let_stmt.pat().unwrap();
                    let _type_ref = let_stmt.ty();
                    let initializer = let_stmt.initializer();
                    let else_branch = let_stmt.let_else().and_then(|x| x.block_expr());

                    if let Some(init) = initializer {
                        let init_str = self.emit_expr_str_ast(&init);
                        if let Some(else_branch) = else_branch {
                            self.emit_let_stmt(out, &pat, init_str, |this, out| {
                                out.wln("{").indent();
                                this.emit_expr_as_stmt_ast(
                                    out,
                                    else_branch.into(),
                                    is_tail,
                                    returning,
                                );
                                out.dedent();
                                out.wln("}");
                            });
                        } else {
                            self.emit_let_stmt(out, &pat, init_str, Self::emit_unreachable);
                        }
                    } else {
                        let bindings = self.collect_bindings_in_pat_ast(&pat);
                        for b in &bindings {
                            let cs_type = self.rust_type_to_cs(&b.ty(self.db));
                            let cs_name = self.alloc_binding_ast(&b);
                            out.wln(format!("{} {} = (default!);", cs_type, cs_name));
                        }
                    }
                }
                ast::Stmt::ExprStmt(expr_stmt) => {
                    self.emit_expr_as_stmt_ast(out, expr_stmt.expr().unwrap(), false, returning);
                }
                ast::Stmt::Item(ast::Item::Fn(fn_)) => {
                    let f = self.sem.to_def(&fn_).unwrap();
                    out.wln(format!("// inner fn: {}", f.name(self.db).as_str()));
                    self.deferred.push(f.into());
                }
                ast::Stmt::Item(ast::Item::Enum(adt)) => {
                    let f = self.sem.to_def(&adt).unwrap();
                    out.wln(format!("// inner enum: {}", f.name(self.db).as_str()));
                    self.deferred.push(f.into());
                }
                ast::Stmt::Item(ast::Item::Struct(adt)) => {
                    let f = self.sem.to_def(&adt).unwrap();
                    out.wln(format!("// inner enum: {}", f.name(self.db).as_str()));
                    self.deferred.push(f.into());
                }
                ast::Stmt::Item(ast::Item::Impl(impl_)) => {
                    let f = self.sem.to_def(&impl_).unwrap();
                    out.wln("// impl");
                    self.deferred.push(f.into());
                }
                // TODO: impl
                // TODO: use, type alias: remove with comment?
                ast::Stmt::Item(item) => {
                    out.wln(format!("/* unsupported inner item: {:?} */", item));
                }
            }
        }

        if let Some(tail_expr) = tail {
            if returning {
                let tail_str = self.emit_expr_str_ast(&tail_expr);
                if is_tail {
                    if tail_str != "()".into() && !tail_str.is_empty() {
                        out.w("return ").w(&tail_str).wln(";");
                    }
                } else if tail_str != "()".into() && !tail_str.is_empty() {
                    out.w(&tail_str).wln(";");
                }
            } else {
                self.emit_expr_as_stmt_ast(out, tail_expr, true, false);
            }
        }
    }

    pub fn emit_expr_str_ast(&mut self, expr: &ast::Expr) -> Code {
        self.emit_expr_str_ast_inner(expr, false)
    }

    pub fn emit_expr_str_ast_inner(&mut self, expr: &ast::Expr, statement: bool) -> Code {
        match expr {
            //Expr::Missing => "/* missing */default!".into(),
            ast::Expr::Literal(lit) => self.emit_literal_ast(&lit),
            ast::Expr::PathExpr(path) => match self
                .sem
                .resolve_path_with_subst(&path.path().unwrap())
            {
                None => {
                    eprintln!(
                        "Unresolved Path at {loc}",
                        loc = self.expr_location_ast(expr),
                    );
                    fcode!("/* {} */", path.syntax().text().to_string())
                }
                Some((hir::PathResolution::Def(hir::ModuleDef::Function(f)), Some(ref args)))
                    if let Some(resolved_info) = resolve_into(f, args, self.db) =>
                {
                    code!(
                        self.rust_type_to_cs(&resolved_info.target_ty),
                        ".m_From/*convert from Into::into*/"
                    )
                }
                Some((hir::PathResolution::Def(hir::ModuleDef::Function(f)), args)) => self
                    .fn_path_cs1(f, &args.map(|x| x.types(self.db)).unwrap_or_default())
                    .into(),
                Some((hir::PathResolution::Def(module_def), _))
                    if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                {
                    let expr_type = self.sem.type_of_expr(expr).unwrap().original;
                    match def.kind(self.db) {
                        StructKind::Unit => {
                            let ty_args = expr_type.expect_adt_of(def.adt(self.db));
                            fcode!(
                                "{}.instance",
                                self.constructable_name_cs(&Constructable::new(def, ty_args))
                            )
                        }
                        StructKind::Tuple => {
                            let ret_ty = expr_type.as_callable(self.db).unwrap().return_type();
                            let ty_args = ret_ty.expect_adt_of(def.adt(self.db));
                            fcode!(
                                "{}.ctor",
                                self.constructable_name_cs(&Constructable::new(def, ty_args))
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

                            fcode!("new /* unexpected struct/enum kind and infer */ UnknownType")
                        }
                    }
                }
                Some((hir::PathResolution::SelfType(impl_), _)) => {
                    eprintln!("ImplSelf at {}", self.expr_location_ast(expr));
                    "(ImplSelf)".into()
                }
                Some((hir::PathResolution::Local(local), _)) => self.binding_name_ast(local).into(),
                Some((hir::PathResolution::Def(hir::ModuleDef::Const(const_)), _)) => {
                    self.const_path_cs(const_).into()
                }
                Some((hir::PathResolution::Def(hir::ModuleDef::Static(static_)), _)) => {
                    let mut path = self.module_class_cs(static_.module(self.db));
                    path.push('.');
                    path.push_str(static_.name(self.db).as_str());
                    path.into()
                }
                Some((hir::PathResolution::TypeParam(param), _)) => {
                    eprintln!("GenericParam at {}", self.expr_location_ast(expr));
                    "(GenericParam)".into()
                }

                Some((resolved, _)) => {
                    eprintln!(
                        "Path at {loc}: {resolved:?}",
                        loc = self.expr_location_ast(expr),
                    );
                    fcode!("/* {} */", path.syntax().text().to_string())
                }
            },
            ast::Expr::FieldExpr(field_expr) => {
                let name = field_expr.name_ref().unwrap();
                let receiver_part = field_expr.expr().unwrap();
                let receiver_type = self.sem.type_of_expr(&receiver_part).unwrap();
                let receiver = self.emit_expr_str_ast(&receiver_part);
                let adjuster = if let Some(adjusted) = receiver_type.adjusted
                    && std::iter::successors(receiver_type.original.remove_ref(), |c| {
                        c.remove_ref()
                    })
                    .all(|remove_ref| adjusted != remove_ref)
                {
                    tracing::trace!(
                        "adjusted type {original} to {adjusted} to access {name}",
                        original = receiver_type.original.debug_display(self.db),
                        adjusted = adjusted.debug_display(self.db),
                        name = field_expr.name_ref().unwrap().text().as_str(),
                    );
                    ".m_Deref()"
                } else {
                    ""
                };
                match self.sem.resolve_field(field_expr) {
                    None => {
                        eprintln!("Unresolved field at {}", self.expr_location_ast(field_expr));
                        code!(receiver, adjuster, "./* unresolved field */")
                    }
                    Some(Either::Left(field)) => {
                        code!(receiver, adjuster, ".", self.field_name(&field))
                    }
                    Some(Either::Right(field)) => {
                        code!(receiver, adjuster, ".", field.index + 1)
                    }
                }
            }
            ast::Expr::MethodCallExpr(method_call) => {
                let receiver = self.emit_expr_str_ast(&method_call.receiver().unwrap());

                match self.sem.resolve_method_call_fallback(method_call) {
                    Some((Either::Left(f), Some(args)))
                        if method_call.arg_list().unwrap().args().count() == 0
                            && let Some(resolved_info) = resolve_into(f, &args, self.db) =>
                    {
                        if Some(&resolved_info.target_ty) == resolved_info.self_ty.as_ref()
                            || self.rust_type_to_cs(&resolved_info.target_ty)
                                == self.rust_type_to_cs(resolved_info.self_ty.as_ref().unwrap())
                        {
                            code!("/*omit Into::into method*/(", receiver, ")")
                        } else {
                            code!(
                                self.rust_type_to_cs(&resolved_info.target_ty),
                                ".m_From/*convert from Into::into method*/(",
                                receiver,
                                ")"
                            )
                        }
                    }
                    Some((Either::Left(resolved), generics)) => {
                        let generics = generics.map(|generics| {
                            self.params1(
                                &self.extract_generic_args(resolved, generics),
                                resolved.into(),
                            )
                        });
                        //generic_args(String::new(), generics.into_iter().flatten().collect()),
                        let method_cs = self.function_name(resolved);

                        let args = method_call
                            .arg_list()
                            .unwrap()
                            .args()
                            .map(|a| self.emit_expr_str_ast(&a));
                        code!(receiver, ".", method_cs, "(", join(args, ", "), ")")
                    }
                    Some((Either::Right(_), _)) => {
                        eprintln!(
                            "method call resolved to field at {}",
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
                let callee = call_expr.expr().unwrap();
                let args = call_expr.arg_list().unwrap().args();
                if let ast::Expr::PathExpr(path) = &callee {
                    match self.sem.resolve_path_with_subst(&path.path().unwrap()) {
                        Some((PathResolution::Def(def), _))
                            if let Some(def) = ConstructableDef::from_module_def(def)
                                && def.adt(self.db).name(self.db).as_str() == "Cow" =>
                        {
                            // The Cow::Borrow() or Cow::Owned() would become raw value so omit
                            return self.emit_expr_str_ast(&{ args }.nth(0).unwrap());
                        }
                        Some((PathResolution::Def(def), _))
                            if let Some(def) = ConstructableDef::from_module_def(def) =>
                        {
                            let expr_type = self.sem.type_of_expr(expr).unwrap().original;
                            let generic_args = expr_type.expect_adt_of(def.adt(self.db));
                            let callee_type =
                                self.constructable_name_cs(&Constructable::new(def, generic_args));
                            let args_str = args.map(|a| self.emit_expr_str_ast(&a));
                            return code!(callee_type, ".ctor(", join(args_str, ", "), ")");
                        }
                        Some((PathResolution::Def(ModuleDef::Function(f)), _))
                            if let Some(_self_param) = f.self_param(self.db) =>
                        {
                            let mut args = args;
                            let receiver = args.nth(0).unwrap();

                            let receiver = self.emit_expr_str_ast(&receiver);
                            let method_cs = self.function_name(f);
                            let args = args.map(|a| self.emit_expr_str_ast(&a));
                            return code!(receiver, ".", method_cs, "(", join(args, ", "), ")");
                        }
                        Some((PathResolution::Def(ModuleDef::Function(f)), Some(subst)))
                            if call_expr.arg_list().unwrap().args().count() == 0
                                && let Some(resolved_info) = resolve_into(f, &subst, self.db) =>
                        {
                            let receiver = self.emit_expr_str_ast(&{ args }.nth(0).unwrap());
                            return if Some(&resolved_info.target_ty)
                                == resolved_info.self_ty.as_ref()
                                || self.rust_type_to_cs(&resolved_info.target_ty)
                                    == self.rust_type_to_cs(resolved_info.self_ty.as_ref().unwrap())
                            {
                                code!("/*omit Into::into call*/(", receiver, ")")
                            } else {
                                code!(
                                    self.rust_type_to_cs(&resolved_info.target_ty),
                                    ".m_From/*convert from Into::into call*/(",
                                    receiver,
                                    ")"
                                )
                            };
                        }
                        _ => {}
                    }
                }

                let callee_str = self.emit_expr_str_ast(&callee);
                let args_str = args.map(|a| self.emit_expr_str_ast(&a));
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
                    None => "/* op= */ =",
                    Some(BinaryOp::ArithOp(ArithOp::Add)) => "+",
                    Some(BinaryOp::ArithOp(ArithOp::Mul)) => "*",
                    Some(BinaryOp::ArithOp(ArithOp::Sub)) => "-",
                    Some(BinaryOp::ArithOp(ArithOp::Div)) => "/",
                    Some(BinaryOp::ArithOp(ArithOp::Rem)) => "%",
                    Some(BinaryOp::ArithOp(ArithOp::Shl)) => "<<",
                    Some(BinaryOp::ArithOp(ArithOp::Shr)) => ">>",
                    Some(BinaryOp::ArithOp(ArithOp::BitXor)) => "^",
                    Some(BinaryOp::ArithOp(ArithOp::BitOr)) => "|",
                    Some(BinaryOp::ArithOp(ArithOp::BitAnd)) => "&",
                    Some(BinaryOp::CmpOp(c)) => {
                        use hir_def::hir::{CmpOp, Ordering};
                        match c {
                            CmpOp::Eq { negated: false } => "==",
                            CmpOp::Eq { negated: true } => "!=",
                            CmpOp::Ord {
                                ordering: Ordering::Less,
                                strict: true,
                            } => "<",
                            CmpOp::Ord {
                                ordering: Ordering::Less,
                                strict: false,
                            } => "<=",
                            CmpOp::Ord {
                                ordering: Ordering::Greater,
                                strict: true,
                            } => ">",
                            CmpOp::Ord {
                                ordering: Ordering::Greater,
                                strict: false,
                            } => ">=",
                        }
                    }
                    Some(BinaryOp::LogicOp(LogicOp::And)) => "&&",
                    Some(BinaryOp::LogicOp(LogicOp::Or)) => "&&",

                    Some(BinaryOp::Assignment { op }) => match op {
                        None => "=",
                        Some(ArithOp::Add) => "+=",
                        Some(ArithOp::Mul) => "*=",
                        Some(ArithOp::Sub) => "-=",
                        Some(ArithOp::Div) => "/=",
                        Some(ArithOp::Rem) => "%=",
                        Some(ArithOp::Shl) => "<<=",
                        Some(ArithOp::Shr) => ">>=",
                        Some(ArithOp::BitXor) => "^=",
                        Some(ArithOp::BitOr) => "|=",
                        Some(ArithOp::BitAnd) => "&=",
                    },
                };
                if statement {
                    code!(lhs_code, " ", op_str, " ", rhs_code)
                } else {
                    code!("(", lhs_code, " ", op_str, " ", rhs_code, ")")
                }
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
                // TODO: Cast might not be compatible
                code!(
                    "(",
                    self.rust_type_to_cs(&self.sem.resolve_type(&cast_expr.ty().unwrap()).unwrap()),
                    ") ",
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
                    "default(global::System.ValueTuple)".into()
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
                    Some(variant) if let Some(def) = ConstructableDef::from_variant(variant) => {
                        let ty_args = (self.sem.type_of_expr(expr).unwrap().original)
                            .expect_adt_of(def.adt(self.db));

                        Constructable::new(def, ty_args)
                    }
                    Some(_) => {
                        eprintln!("Union unsupported at {}", self.expr_location_ast(expr));

                        return fcode!(
                            "(UnionConstruction/*{:?}*/)",
                            record_expr.syntax().text().to_string()
                        );
                    }
                };
                let type_name = self.constructable_name_cs(&c);

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
                        code!(cs_f, " = (", val, ")")
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
                    .unwrap_or(code!("unspecified"));
                let end_code = range_expr
                    .end()
                    .map(|e| self.emit_expr_str_ast(&e))
                    .unwrap_or(code!("unspecified"));
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
                    let Some((element, _len)) = self
                        .sem
                        .type_of_expr(expr)
                        .unwrap()
                        .original
                        .as_array(self.db)
                    else {
                        panic!("");
                    };
                    let cg = self.cg;
                    let parts = elements.map(|e| self.emit_expr_str_ast(&e));
                    code!(
                        "new ",
                        cg.rust_type_to_cs(&element),
                        "[] { ",
                        join(parts, ", "),
                        " }"
                    )
                }
            },
            ast::Expr::ClosureExpr(closure) => {
                let param_names = closure.param_list().unwrap().params().enumerate();
                let params: Vec<String> = (param_names.clone())
                    .map(|(i, p)| {
                        if let ast::Pat::IdentPat(ident_pat) = p.pat().unwrap()
                            && let Some(local) = self.sem.to_def(&ident_pat)
                        {
                            let binding = self.alloc_binding_ast(&local);
                            binding
                        } else {
                            format!("__cp{}", i)
                        }
                    })
                    .collect();

                let mut body_block = Code::new();
                for (i, p) in param_names.clone() {
                    if let ast::Pat::IdentPat(ident_pat) = p.pat().unwrap()
                        && let Some(_) = self.sem.to_def(&ident_pat)
                    {
                    } else {
                        self.emit_let_stmt(
                            &mut body_block,
                            &p.pat().unwrap(),
                            code!(&format!("__cp{}", i)),
                            Self::emit_unreachable,
                        );
                    }
                }

                if let ast::Expr::BlockExpr(block) = closure.body().unwrap() {
                    self.emit_block_contents(
                        &mut body_block,
                        block.statements(),
                        block.tail_expr(),
                        true,
                        true,
                    );
                    code!(
                        "(",
                        join(params, ", "),
                        ") => {\n",
                        indent,
                        body_block,
                        dedent,
                        "}"
                    )
                } else {
                    let body_str = self.emit_expr_str_ast(&closure.body().unwrap());
                    if body_block.is_empty() {
                        code!("(", join(params, ", "), ") => ", body_str)
                    } else {
                        code!(
                            "(",
                            join(params, ", "),
                            ") => {\n",
                            indent,
                            body_block,
                            "return ",
                            body_str,
                            ";\n",
                            dedent,
                            "}"
                        )
                    }
                }
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
                let pattern_cs = self.emit_pattern_ast(&let_expr.pat().unwrap());
                code!("(", val, " is ", pattern_cs, ")")
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
            ast::Expr::MatchExpr(match_expr) => {
                let arms = match_expr.match_arm_list().unwrap();
                let match_expr = match_expr.expr().unwrap();
                let scrutinee = self.emit_expr_str_ast(&match_expr);

                let mut out = Code::new();

                out.w("(").w(scrutinee).wln(") switch {");
                out.indent();

                for arm in arms.arms() {
                    // Emit pattern check
                    let pat_cs = self.emit_pattern_ast(&arm.pat().unwrap());

                    out.w("").w(pat_cs).w(" ");
                    if let Some(guard) = arm.guard() {
                        out.w("when ");
                        out.w(self.emit_expr_str_ast(&guard.condition().unwrap()));
                    }
                    out.w("=> ")
                        .w(self.emit_expr_str_ast(&arm.expr().unwrap()))
                        .wln(",");
                }
                out.dedent();

                out.w("}");

                out
            }
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
                panic!(
                    "Unsupported Expression: Offsetof / assembly at {}",
                    self.expr_location_ast(expr)
                );
            }
            ast::Expr::LoopExpr(_) | ast::Expr::ForExpr(_) | ast::Expr::WhileExpr(_) => {
                // TODO
                "/* loop */ default!".into()
            }
            ast::Expr::FormatArgsExpr(_) => {
                // TODO
                "/* FormatArgsExpr */ default!".into()
            }
            ast::Expr::MacroExpr(m) => {
                // TODO
                let macro_call = m.macro_call().unwrap();
                fcode!(
                    "/* macro name {macro_path} */ macro_{short_name}()",
                    macro_path = macro_call.path().unwrap().syntax().text(),
                    short_name = (macro_call.path().unwrap().segments().last().unwrap())
                        .syntax()
                        .text(),
                )
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

    fn build_positional_field_inits(
        &mut self,
        fields: &[hir::Field],
        args: &[ast::Expr],
    ) -> Vec<Code> {
        fields
            .iter()
            .zip(args.iter())
            .map(|(field, arg)| {
                let f_name = names::field_name(field.name(self.db).as_str());
                let val = self.emit_expr_str_ast(arg);
                code!(f_name, " = (", val, ")")
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
    fn emit_pattern_ast(&mut self, pat: &ast::Pat) -> Code {
        match pat {
            ast::Pat::WildcardPat(w) => "var _".into(),
            ast::Pat::IdentPat(ident_pat)
                if let Some(const_ref) = self.sem.resolve_bind_pat_to_const(ident_pat) =>
            {
                match const_ref {
                    ModuleDef::EnumVariant(variant) => {
                        let ty_args = (self.sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(variant.parent_enum(self.db).into());

                        fcode!(
                            "{}",
                            self.constructable_name_cs(&Constructable::new(
                                variant.into(),
                                ty_args
                            ))
                        )
                    }
                    _ => {
                        // TODO
                        code!(format!("{:?}", const_ref))
                    }
                }
            }
            ast::Pat::IdentPat(ident_pat) => {
                let local = self.sem.to_def(ident_pat).unwrap();
                let cs_local_name = self.alloc_binding_ast(&local);
                if let Some(sub) = ident_pat.pat() {
                    code!(self.emit_pattern_ast(&sub), " ", cs_local_name)
                } else {
                    code!("var ", cs_local_name)
                }
            }
            ast::Pat::TupleStructPat(tuple_struct) => {
                let path = tuple_struct.path().unwrap();

                let (cs_type_name, field_count) = match self.sem.resolve_path(&path) {
                    Some(PathResolution::Def(module_def))
                        if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                    {
                        let ty_args = (self.sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(def.adt(self.db));
                        let field_count = def.fields(self.db).len();
                        let cs_variant =
                            self.constructable_name_cs(&Constructable::new(def, ty_args));
                        (cs_variant, field_count)
                    }
                    resolved => {
                        eprintln!(
                            "Unable to resolve path in pattern {resolved:?}: {}",
                            self.expr_location_ast(pat)
                        );
                        return code!("Unknown");
                    }
                };

                let pattern_codes = self.resolve_tuple_like_struct(
                    field_count,
                    &tuple_struct.fields().collect::<Vec<_>>(),
                );

                let pattern_codes = pattern_codes
                    .iter()
                    .enumerate()
                    .map(|(indeex, pat_code)| code!(format("f_{indeex}: "), pat_code));
                code!(cs_type_name, "{", join(pattern_codes, ","), "}")
            }
            ast::Pat::PathPat(path) => {
                let path = path.path().unwrap();
                match self.sem.resolve_path(&path) {
                    Some(PathResolution::Def(ModuleDef::Const(const_))) => {
                        code!(self.const_path_cs(const_))
                    }
                    Some(PathResolution::Def(module_def))
                        if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                    {
                        let ty_args = (self.sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(def.adt(self.db));
                        code!(self.constructable_name_cs(&Constructable::new(def, ty_args)))
                    }
                    resolved => {
                        eprintln!(
                            "Unable to resolve path in pattern {resolved:?}: {}",
                            self.expr_location_ast(pat)
                        );
                        fcode!(
                            "Unknown /* bad path resolution: {} */",
                            path.syntax().text()
                        )
                    }
                }
            }
            ast::Pat::RecordPat(record_pat) => {
                let path = record_pat.path().unwrap();
                let (cs_type_name, field_count) = match self.sem.resolve_path(&path) {
                    Some(PathResolution::Def(module_def))
                        if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                    {
                        let ty_args = (self.sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(def.adt(self.db));
                        let field_count = def.fields(self.db).len();
                        let cs_variant =
                            self.constructable_name_cs(&Constructable::new(def, ty_args));
                        (cs_variant, field_count)
                    }
                    resolved => {
                        eprintln!(
                            "Unable to resolve path in pattern {resolved:?}: {}",
                            self.expr_location_ast(pat)
                        );
                        return fcode!(
                            "Unknown /* Bad record path resolution: {} */",
                            path.syntax().text()
                        );
                    }
                };

                let fields = record_pat.record_pat_field_list().unwrap();
                let fields = fields.fields().collect::<Vec<_>>();
                if matches!(fields.as_slice(), []) {
                    code!(cs_type_name, "{", "}")
                } else {
                    let pattern_codes = fields.iter().enumerate().map(|(indeex, field)| {
                        let field_name = field.field_name().unwrap().text().as_str().to_string();
                        let field_name_cs = names::field_name(&field_name);
                        if let Some(field_pat) = field.pat() {
                            code!(
                                format("{field_name_cs}: "),
                                self.emit_pattern_ast(&field_pat)
                            )
                        } else {
                            eprintln!(
                                "Unable to create variable from field in pattern: {}",
                                self.expr_location_ast(pat)
                            );
                            code!(format("{field_name_cs}: var /*unresolved*/"), field_name)
                        }
                    });
                    code!(cs_type_name, "{", join(pattern_codes, ","), "}")
                }
            }
            ast::Pat::LiteralPat(literal_pat) => {
                self.emit_expr_str_ast(&ast::Expr::Literal(literal_pat.literal().unwrap()))
            }
            ast::Pat::OrPat(or_pat) => {
                let parts = or_pat.pats().map(|p| self.emit_pattern_ast(&p));
                code!("(", join(parts, " or "), ")")
            }
            ast::Pat::TuplePat(tuple_pat) => {
                let type_ = self.sem.type_of_pat(&pat).unwrap();
                let field_count = type_.original.tuple_fields(self.db).len();
                let pattern_codes = self.resolve_tuple_like_struct(
                    field_count,
                    &tuple_pat.fields().collect::<Vec<_>>(),
                );

                code!("(", join(pattern_codes, ","), ")")
            }
            ast::Pat::RangePat(range_pat) => {
                match (
                    range_pat.start(),
                    range_pat.op_kind().unwrap(),
                    range_pat.end(),
                ) {
                    (Some(lower), RangeOp::Exclusive, Some(upper)) => {
                        code!(
                            "(>=",
                            self.emit_pattern_ast(&lower),
                            " and <",
                            self.emit_pattern_ast(&upper),
                            ")"
                        )
                    }
                    (Some(lower), RangeOp::Inclusive, Some(upper)) => {
                        code!(
                            "(>=",
                            self.emit_pattern_ast(&lower),
                            " and <=",
                            self.emit_pattern_ast(&upper),
                            ")"
                        )
                    }
                    (Some(lower), RangeOp::Exclusive, None) => {
                        code!("(>=", self.emit_pattern_ast(&lower), ")")
                    }
                    (None, RangeOp::Exclusive, Some(upper)) => {
                        code!("(<", self.emit_pattern_ast(&upper), ")")
                    }
                    (None, RangeOp::Inclusive, Some(upper)) => {
                        code!("(<=", self.emit_pattern_ast(&upper), ")")
                    }
                    (lower, op, upper) => {
                        panic!("Bad pattern: {lower:?} {op:?} {upper:?}")
                    }
                }
            }
            ast::Pat::RefPat(ref_pat) => self.emit_pattern_ast(&ref_pat.pat().unwrap()),
            pat => {
                eprintln!(
                    "Unsupported pattern {pat:?} at {}",
                    self.expr_location_ast(pat)
                );
                code!("Unknown /* unsupported pattern */")
            }
        }
    }

    fn resolve_tuple_like_struct(
        &mut self,
        field_count: usize,
        patterns: &[ast::Pat],
    ) -> Vec<Code> {
        if matches!(patterns, [ast::Pat::RestPat(_)]) {
            vec![code!("_"); field_count]
        } else if let Some(position) = patterns
            .iter()
            .position(|p| matches!(p, ast::Pat::RestPat(_)))
        {
            let prefix = &patterns[..position];
            let suffix = &patterns[(position + 1)..];
            let mut pattern_codes = vec![code!("_"); field_count];
            for (pat, code) in prefix.iter().zip(pattern_codes.iter_mut()) {
                *code = self.emit_pattern_ast(pat);
            }
            for (pat, code) in suffix.iter().rev().zip(pattern_codes.iter_mut().rev()) {
                *code = self.emit_pattern_ast(pat);
            }

            pattern_codes
        } else {
            let pattern_codes = patterns
                .iter()
                .enumerate()
                .map(|(indeex, pat)| self.emit_pattern_ast(pat));
            pattern_codes.collect::<Vec<_>>()
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

struct ResolvedInto<'db> {
    pub self_ty: Option<hir::Type<'db>>,
    pub target_ty: hir::Type<'db>,
}

fn resolve_into<'db>(
    f: hir::Function,
    args: &hir::GenericSubstitution<'db>,
    db: &'db dyn HirDatabase,
) -> Option<ResolvedInto<'db>> {
    if f.name(db).symbol() == &hir::sym::into
        && let ItemContainer::Trait(trait_) = f.container(db)
        && trait_.name(db).symbol() == &hir::sym::Into
    {
        let params = hir::GenericDef::Trait(trait_).params(db);
        let (symbol0, type0) = &args.types(db)[0];
        let (symbol1, type1) = &args.types(db)[1];
        assert_eq!(params[0].name(db).symbol(), symbol0);
        assert_eq!(params[1].name(db).symbol(), symbol1);
        Some(ResolvedInto::<'db> {
            self_ty: Some(type0.clone()),
            target_ty: type1.clone(),
        })
    } else if f.name(db).symbol() == &hir::sym::into
        && let ItemContainer::Impl(impl_) = f.container(db)
        && let Some(trait_ref) = impl_.trait_ref(db)
        && trait_ref.trait_().name(db).symbol() == &hir::sym::Into
    {
        let impl_params = hir::GenericDef::Impl(impl_).type_or_const_params(db);

        let self_type = impl_.self_ty(db);
        let type_arg: hir::Type = trait_ref
            .get_type_argument(1)
            .expect("get_type_argument of Into")
            .to_type(db);

        let self_as_param = self_type
            .as_type_param(db)
            .expect("Self type of impl Into<T> for U is not type param");
        let target_as_param = type_arg
            .as_type_param(db)
            .expect("T of Into<T> impl is not type param");

        let self_index = (impl_params.iter())
            .position(|x| x.as_type_param(db).is_some_and(|x| x == self_as_param))
            .unwrap_or_else(|| panic!("{self_as_param:?}\n{impl_params:?}"));
        let target_index = (impl_params.iter())
            .position(|x| x.as_type_param(db).is_some_and(|x| x == target_as_param))
            .unwrap();

        let self_type = args.types(db)[self_index].1.clone();
        let target_type = args.types(db)[target_index].1.clone();

        Some(ResolvedInto::<'db> {
            self_ty: Some(self_type),
            target_ty: target_type,
        })
    } else {
        None
    }
}
