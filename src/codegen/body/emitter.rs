//! Generates C# expressions and statements from Rust HIR bodies.

use super::super::{CodeGenerator, generic_args, names, output::Code};
use crate::codegen::body::eir;
use crate::codegen::body::eir::EirNode;
use crate::codegen::body::semantics::EirSemantics;
use crate::codegen::constructable::{Constructable, ConstructableDef};
use crate::codegen::decl::CsFunctionType;
use crate::codegen::function_resolution::{ArgSource, ResolvedFunction};
use crate::codegen::simple_extensions::*;
use crate::codegen::ty::CsTypeOption;
use eir::{ArithOp, BinaryOp, LogicOp, RangeOp, UnaryOp};
use hir::db::HirDatabase;
use hir::{HasCrate, Local, ModuleDef, PathResolution, StructKind, Type, sym};
use itertools::Either;
use ra_internal::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};
use std::sync::atomic::AtomicUsize;
use syntax::ast;

/// Generates C# for the body of a single function.
pub struct EirEmitter<'g, 'db> {
    cg: &'g CodeGenerator<'db>,
    /// Mapping from Local to the C# local name allocated for it.
    locals: RefCell<HashMap<Local<'db>, String>>,
    /// Counter per original Rust name for uniqueness.
    name_counts: RefCell<HashMap<String, usize>>,
    cs_type: CsFunctionType,
    ctx: RefCell<CodeContext<'db>>,

    // internals
    match_index: AtomicUsize,

    pub deferred: RefCell<Vec<ItemInBody>>,
}

/// Items emitting are deferred.
/// Caller must generate those items.
#[derive(Debug, Copy, Clone)]
pub enum ItemInBody {
    Function(hir::Function),
    Adt(hir::Adt),
    Const(hir::Const),
    Impl(hir::Impl),
}

impl_from!(hir::Function, hir::Adt(hir::Enum, hir::Struct), hir::Impl, hir::Const for ItemInBody);

impl hir::HasContainer for ItemInBody {
    fn container(&self, db: &dyn HirDatabase) -> hir::ItemContainer {
        match self {
            ItemInBody::Function(i) => hir::HasContainer::container(i, db),
            ItemInBody::Const(i) => hir::HasContainer::container(i, db),
            ItemInBody::Adt(hir::Adt::Struct(i)) => hir::HasContainer::container(i, db),
            ItemInBody::Adt(hir::Adt::Enum(i)) => hir::HasContainer::container(i, db),
            ItemInBody::Adt(hir::Adt::Union(i)) => hir::HasContainer::container(i, db),
            ItemInBody::Impl(i) => hir::ItemContainer::Module(i.module(db)),
        }
    }
}

impl<'g, 'db> std::ops::Deref for EirEmitter<'g, 'db> {
    type Target = CodeGenerator<'db>;

    fn deref(&self) -> &Self::Target {
        self.cg
    }
}

struct CodeContext<'db> {
    is_async: bool,
    returning: hir::Type<'db>,
}

impl<'g, 'db> EirEmitter<'g, 'db> {
    pub fn new(cg: &'g CodeGenerator<'db>, is_async: bool, cs_type: CsFunctionType) -> Self {
        Self {
            cg,
            locals: RefCell::new(HashMap::new()),
            name_counts: RefCell::new(HashMap::new()),
            ctx: RefCell::new(CodeContext {
                is_async,
                returning: hir::Type::error(cg.db, cg.krate),
            }),
            match_index: AtomicUsize::new(0),
            deferred: RefCell::new(Vec::new()),
            cs_type,
        }
    }

    pub fn deferred(&mut self) -> &mut Vec<ItemInBody> {
        self.deferred.get_mut()
    }

    /// Emit the full function body block.
    pub fn emit_function_body(&self, mut f: eir::Fn, out: &mut Code) {
        let mut transformer = crate::codegen::body::transformer::EirTransformer::new(self.cg);
        let std::ops::ControlFlow::Continue(()) = f.accept_mut(&mut transformer);
        self.emit_function_body_impl(f, out)
    }

    pub fn emit_expr(&self, mut expr: eir::Expr) -> Code {
        let mut transformer = crate::codegen::body::transformer::EirTransformer::new(self.cg);
        let std::ops::ControlFlow::Continue(()) = expr.accept_mut(&mut transformer);
        self.emit_expr_str_ast(&expr)
    }
}

impl<'g, 'db> EirEmitter<'g, 'db> {
    fn alloc_binding_ast(&self, local: &Local<'db>) -> String {
        let rust_name = local.name(self.db).as_str().to_string();
        let mut name_counts = self.name_counts.borrow_mut();
        let count = name_counts.entry(rust_name.clone()).or_insert(0);
        let cs_name = names::local_name(&rust_name, *count);
        *count += 1;
        self.add_local(*local, cs_name.clone());
        cs_name
    }

    fn binding_name_ast(&self, local: Local) -> String {
        if let Some(local) = self.locals.borrow().get(&local) {
            return local.clone();
        }

        if let Either::Left(pat) = local.primary_source(self.db).source.value
            && let Some(new_local) = self.eir_sem.to_def(&pat)
            && local != new_local
        {
            self.binding_name_ast(new_local)
        } else {
            format!("/* unbound {:?} */unknown", local)
        }
    }

    fn label_name(&self, l: &eir::Lifetime) -> String {
        names::camel(&l.text()[1..])
    }

    fn add_local(&self, local: Local<'db>, name: String) {
        self.locals.borrow_mut().insert(local, name);
    }

    fn add_deferred(&self, deferred: impl Into<ItemInBody>) {
        self.deferred.borrow_mut().push(deferred.into());
    }

    fn inc_match_index(&self) -> usize {
        self.match_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn check_cfg(&self, value: &impl eir::HasAttrs) -> bool {
        fn check_meta(meta: ast::Meta, krate: hir::Crate, db: &dyn HirDatabase) -> Option<bool> {
            match meta {
                ast::Meta::CfgMeta(meta) => {
                    let cfg_predicate = meta.cfg_predicate()?;
                    let cfg_predicate = hir::CfgExpr::parse_from_ast(cfg_predicate);
                    krate.cfg(db).check(&cfg_predicate)
                }
                ast::Meta::CfgAttrMeta(meta) => {
                    let cfg_predicate = meta.cfg_predicate()?;
                    let cfg_predicate = hir::CfgExpr::parse_from_ast(cfg_predicate);
                    if krate.cfg(db).check(&cfg_predicate)? {
                        for meta in meta.metas() {
                            if check_meta(meta, krate, db) == Some(false) {
                                return Some(false);
                            }
                        }
                        None
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }

        value.attrs().all(|attr| {
            let Some(meta) = attr.meta() else { return true };
            check_meta(meta, self.krate, self.db).unwrap_or(true)
        })
    }

    fn is_async(&self) -> bool {
        self.ctx.borrow().is_async
    }

    fn returning_type(&self) -> hir::Type<'db> {
        self.ctx.borrow().returning.clone()
    }

    fn new_ctx(&self, new: CodeContext<'db>) -> impl Drop {
        struct RestoreCtxScope<'a, 'db> {
            cell: &'a RefCell<CodeContext<'db>>,
            prev: Option<CodeContext<'db>>,
        }

        impl Drop for RestoreCtxScope<'_, '_> {
            fn drop(&mut self) {
                *self.cell.borrow_mut() = self.prev.take().unwrap();
            }
        }

        let mut ctx = self.ctx.borrow_mut();
        let prev = std::mem::replace(&mut *ctx, new);
        RestoreCtxScope {
            cell: &self.ctx,
            prev: Some(prev),
        }
    }

    #[tracing::instrument(skip_all)]
    fn emit_function_body_impl(&self, f: eir::Fn, out: &mut Code) {
        let params = f.param_list();
        let f_def = self.eir_sem.to_def(&f).unwrap();
        let _scope = self.new_ctx(CodeContext {
            is_async: self.is_async(),
            returning: f_def
                .async_ret_type(self.db)
                .unwrap_or_else(|| f_def.ret_ty(self.db)),
        });
        if let Some(self_param) = params.self_param() {
            match self.cs_type {
                CsFunctionType::TraitDefaultImpl => {
                    self.add_local(self.eir_sem.to_def(self_param).unwrap(), "self".into());
                }
                _ => {
                    self.add_local(self.eir_sem.to_def(self_param).unwrap(), "this".into());
                }
            }
        }
        for param in params.params() {
            match param.pat() {
                eir::Pat::IdentPat(ident) if let Some(local) = self.eir_sem.to_def(ident) => {
                    self.alloc_binding_ast(&local);
                }
                _ => {
                    // TODO
                }
            }
        }
        if let Some(body) = f.body() {
            self.emit_returning_block(out, body);
        } else {
            out.wln("throw new System.NotImplementedException(\"builtin-derive\");");
        }
    }
}

#[derive(Clone)]
struct ExprGenOption {
    returning: bool,
}

impl ExprGenOption {
    fn returning() -> Self {
        Self { returning: true }
    }

    fn non_last(&self) -> ExprGenOption {
        Self { returning: false }
    }
}

#[derive(Copy, Clone, Debug)]
struct EmittedExprInfo {
    diverging: bool,
}

impl EmittedExprInfo {
    pub fn diverging() -> Self {
        Self { diverging: true }
    }

    pub fn non_diverging() -> Self {
        Self { diverging: false }
    }
}

impl BitOr<EmittedExprInfo> for EmittedExprInfo {
    type Output = EmittedExprInfo;
    fn bitor(self, rhs: EmittedExprInfo) -> Self::Output {
        Self {
            diverging: self.diverging | rhs.diverging,
        }
    }
}

impl BitAnd<EmittedExprInfo> for EmittedExprInfo {
    type Output = EmittedExprInfo;
    fn bitand(self, rhs: EmittedExprInfo) -> Self::Output {
        Self {
            diverging: self.diverging & rhs.diverging,
        }
    }
}

impl BitAndAssign for EmittedExprInfo {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs
    }
}

impl BitOrAssign<EmittedExprInfo> for EmittedExprInfo {
    fn bitor_assign(&mut self, rhs: EmittedExprInfo) {
        *self = *self | rhs
    }
}

impl<'g, 'db> EirEmitter<'g, 'db> {
    /// Emit an expression as a statement (with semicolon if needed).
    fn emit_expr_as_stmt_ast(
        &self,
        out: &mut Code,
        expr: &eir::Expr,
        option: ExprGenOption,
    ) -> EmittedExprInfo {
        if !self.check_cfg(expr) {
            return EmittedExprInfo::non_diverging();
        }

        match expr {
            eir::Expr::BlockExpr(block_expr) if block_expr.modifier().is_none() => {
                let statements = block_expr.statements();
                let tail = block_expr.tail_expr();
                self.emit_block_contents(out, statements, tail, option)
            }
            eir::Expr::ReturnExpr(ret_expr) => {
                if let Some(value_expr) = ret_expr.expr() {
                    let info =
                        self.emit_expr_as_stmt_ast(out, value_expr, ExprGenOption::returning());
                    if !info.diverging {
                        self.emit_return_void(out);
                    }
                    EmittedExprInfo::diverging()
                } else {
                    self.emit_return_void(out);
                    EmittedExprInfo::diverging()
                }
            }
            eir::Expr::TupleExpr(tuple_expr) if tuple_expr.fields().next().is_none() => {
                out.wln("/* () unit expr */;");
                EmittedExprInfo::non_diverging()
            }
            eir::Expr::IfExpr(if_expr) => {
                let condition = if_expr.condition();
                let then_branch = if_expr.then_branch();
                let else_branch = if_expr.else_branch();

                let cond = self.emit_expr_str_ast(condition);
                out.w("if (").w(cond).wln(") {");
                out.indent();
                let then_part = self.emit_simple_block_as_stmt(out, then_branch, option.clone());
                out.dedent();
                let else_part = match else_branch {
                    Some(eir::ElseBranch::IfExpr(else_if)) => {
                        out.w("} else ");
                        // TODO: prevent cloning
                        self.emit_expr_as_stmt_ast(out, &else_if.clone().into(), option.clone())
                    }
                    Some(eir::ElseBranch::Block(else_e)) => {
                        out.w("} else {");
                        out.wln("");
                        out.indent();
                        let part = self.emit_simple_block_as_stmt(out, else_e, option.clone());
                        out.dedent();
                        out.wln("}");
                        part
                    }
                    None => {
                        out.wln("}");
                        EmittedExprInfo::non_diverging()
                    }
                };

                then_part & else_part
            }
            eir::Expr::LoopExpr(loop_expr) => {
                let label = loop_expr.label();
                let body = loop_expr.loop_body();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(&l.lifetime().unwrap())))
                    .unwrap_or_default();
                out.w(label_str).wln("while (true) {");
                out.indent();
                self.emit_simple_block_as_stmt(out, body, option.non_last());
                out.dedent();
                out.wln("}");
                EmittedExprInfo::diverging()
            }
            eir::Expr::WhileExpr(while_expr) => {
                let label = while_expr.label();
                let condition = while_expr.condition();
                let body = while_expr.loop_body();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(&l.lifetime().unwrap())))
                    .unwrap_or_default();
                out.w(label_str)
                    .w("while (")
                    .w(self.emit_expr_str_ast(condition))
                    .wln(") {");
                out.indent();
                self.emit_simple_block_as_stmt(out, body, option.non_last());
                out.dedent();
                out.wln("}");

                EmittedExprInfo::non_diverging()
            }
            eir::Expr::ForExpr(for_expr) => {
                let label = for_expr.label();
                let pat = for_expr.pat();
                let iterable = for_expr.iterable();
                let body = for_expr.loop_body();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(&l.lifetime().unwrap())))
                    .unwrap_or_default();
                let temp_name = format!("__temp_{}", self.inc_match_index());
                out.w(label_str)
                    .w("foreach (var ")
                    .w(&temp_name)
                    .w(" in ")
                    .w(self.emit_expr_str_ast(iterable))
                    .wln(") {");
                out.indent();
                self.emit_let_stmt(out, pat, code!(&temp_name), Self::emit_unreachable);
                self.emit_simple_block_as_stmt(out, body, option.non_last());
                out.dedent();
                out.wln("}");

                EmittedExprInfo::non_diverging()
            }
            // TODO? While and For
            eir::Expr::MatchExpr(match_expr) => {
                let arms = match_expr.match_arm_list();
                let match_expr = match_expr.expr();
                let scrutinee = self.emit_expr_str_ast(match_expr);
                out.w("switch (").w(scrutinee).wln(") {");
                out.indent();

                let mut result = EmittedExprInfo::diverging();
                for arm in arms.arms() {
                    // Emit pattern check
                    let pat_cs = self.emit_pattern_ast(arm.pat());

                    out.w("case ").w(pat_cs).w(" ");
                    if let Some(guard) = arm.guard() {
                        out.w("when ");
                        out.w(self.emit_expr_str_ast(guard));
                    }
                    out.wln(":{");
                    out.indent();
                    let arm_res = self.emit_expr_as_stmt_ast(out, arm.expr(), option.clone());
                    if !arm_res.diverging {
                        out.wln("break;");
                    }
                    result &= arm_res;
                    out.dedent();
                    out.wln("}");
                }
                out.wln("default: throw new System.NullReferenceException();");
                out.dedent();

                out.wln("}");
                result
            }
            eir::Expr::BreakExpr(break_expr) => {
                let label = break_expr.lifetime();
                let break_expr = break_expr.expr();

                // TODO: Labelled break
                let label_str = label
                    .map(|l| format!(" /*{}*/", self.label_name(l)))
                    .unwrap_or_default();
                if let Some(e) = break_expr {
                    let val = self.emit_expr_str_ast(e);
                    out.w("/* break ").w(val).wln(" */"); // TODO: break-with-value
                    EmittedExprInfo::non_diverging()
                } else {
                    out.wln(format!("break{};", label_str));
                    EmittedExprInfo::diverging()
                }
            }
            eir::Expr::ContinueExpr(continue_expr) => {
                let label = continue_expr.lifetime();
                // TODO: Labelled continue
                let label_str = label
                    .map(|l| format!(" /*{}*/", self.label_name(l)))
                    .unwrap_or_default();
                out.wln(format!("continue{};", label_str));
                EmittedExprInfo::diverging()
            }

            eir::Expr::AwaitExpr(await_expr) => {
                let inner_str = self.emit_expr_str_ast(await_expr.expr());
                if option.returning {
                    out.wln(code!("return await ", inner_str, ";"));
                    EmittedExprInfo::diverging()
                } else {
                    out.wln(code!("await ", inner_str, ";"));
                    EmittedExprInfo::non_diverging()
                }
            }
            eir::Expr::MacroStmts(stmts) => {
                self.emit_block_contents(out, stmts.statements(), stmts.tail_expr(), option)
            }
            eir::Expr::RawCodeExpr(raw_cod) => {
                out.wln(code!(raw_cod.code(), ";"));
                if raw_cod.divergent() {
                    EmittedExprInfo::diverging()
                } else {
                    EmittedExprInfo::non_diverging()
                }
            }
            _ => {
                // Generic expression: emit as expression statement
                let s = self.emit_expr_str_ast_inner(expr, true);
                if option.returning && !self.returning_type().is_unit() {
                    out.w("return ").w(s).wln(";");
                    EmittedExprInfo::diverging()
                } else {
                    out.w(s).wln(";");
                    EmittedExprInfo::non_diverging() // TODO: emit_expr_str_ast_inner's diverging
                }
            }
        }
    }

    fn emit_return_void(&self, out: &mut Code) {
        if self.is_async() {
            out.wln("return default(global::System.ValueTuple); // void");
        } else {
            out.wln("return;");
        }
    }

    fn emit_let_stmt(
        &self,
        out: &mut Code,
        pat: &eir::Pat,
        value_cs: Code,
        else_gen: impl FnOnce(&EirEmitter<'g, 'db>, &mut Code),
    ) {
        if let eir::Pat::IdentPat(ident_pat) = pat {
            let local = self.eir_sem.to_def(ident_pat).unwrap();
            let cs_local_name = self.alloc_binding_ast(&local);
            out.w("var ").w(cs_local_name).w(" = ").w(value_cs).wln(";");
        } else {
            let pattern_cs = self.emit_pattern_ast(pat);

            out.w("if (!(").w(value_cs).w(" is ").w(pattern_cs).w(")) ");
            else_gen(self, out);
        }
    }

    fn emit_unreachable(&self, out: &mut Code) {
        out.wln("throw new Exception(\"unreachable\");");
    }

    fn emit_simple_block_as_stmt(
        &self,
        out: &mut Code,
        block: &eir::BlockExpr,
        option: ExprGenOption,
    ) -> EmittedExprInfo {
        self.emit_block_contents(out, block.statements(), block.tail_expr(), option)
    }

    fn emit_block_contents<'a>(
        &self,
        out: &mut Code,
        statements: impl IntoIterator<Item = &'a eir::Stmt>,
        tail: Option<&eir::Expr>,
        option: ExprGenOption,
    ) -> EmittedExprInfo {
        let mut info = EmittedExprInfo::non_diverging();
        for stmt in statements {
            match stmt {
                eir::Stmt::LetStmt(let_stmt) => {
                    let pat = let_stmt.pat();
                    //let _type_ref = let_stmt.ty();
                    let initializer = let_stmt.initializer();
                    let else_branch = let_stmt.let_else().map(|x| x.block_expr());

                    if let Some(init) = initializer {
                        let init_str = self.emit_expr_str_ast(init);
                        if let Some(else_branch) = else_branch {
                            self.emit_let_stmt(out, pat, init_str, |this, out| {
                                out.wln("{").indent();
                                this.emit_simple_block_as_stmt(out, else_branch, option.non_last());
                                out.dedent();
                                out.wln("}");
                            });
                        } else {
                            self.emit_let_stmt(out, pat, init_str, Self::emit_unreachable);
                        }
                    } else {
                        let bindings = self.collect_bindings_in_pat_ast(pat);
                        for b in &bindings {
                            let cs_type = self.rust_type_to_cs(&b.ty(self.db));
                            let cs_name = self.alloc_binding_ast(b);
                            out.wln(format!("{} {} = (default!);", cs_type, cs_name));
                        }
                    }
                }
                eir::Stmt::ExprStmt(expr_stmt) => {
                    info |= self.emit_expr_as_stmt_ast(out, expr_stmt.expr(), option.non_last());
                }

                eir::Stmt::Item(ast::Item::Fn(fn_)) => {
                    let f = self.eir_sem.to_def(fn_).unwrap();
                    out.wln(format!("// inner fn: {}", f.name(self.db).as_str()));
                    self.add_deferred(f);
                }
                eir::Stmt::Item(ast::Item::Enum(adt)) => {
                    let f = self.eir_sem.to_def(adt).unwrap();
                    out.wln(format!("// inner enum: {}", f.name(self.db).as_str()));
                    self.add_deferred(f);
                }
                eir::Stmt::Item(ast::Item::Struct(adt)) => {
                    let f = self.eir_sem.to_def(adt).unwrap();
                    out.wln(format!("// inner enum: {}", f.name(self.db).as_str()));
                    self.add_deferred(f);
                }
                eir::Stmt::Item(ast::Item::Impl(impl_)) => {
                    let f = self.eir_sem.to_def(impl_).unwrap();
                    out.wln("// impl");
                    self.add_deferred(f);
                }
                eir::Stmt::Item(ast::Item::Const(c)) => {
                    let c = self.eir_sem.to_def(c).unwrap();
                    out.wln("// const");
                    self.add_deferred(c);
                }
                // TODO: impl
                // TODO: use, type alias: remove with comment?
                eir::Stmt::Item(item) => {
                    out.wln(format!("/* unsupported inner item: {:?} */", item));
                }
            }
        }

        if let Some(tail_expr) = tail {
            info |= self.emit_expr_as_stmt_ast(out, tail_expr, option);
        }

        info
    }

    fn emit_expr_str_ast(&self, expr: &eir::Expr) -> Code {
        self.emit_expr_str_ast_inner(expr, false)
    }

    #[tracing::instrument(skip_all, fields(expr_at = self.eir_sem.location(expr)))]
    fn emit_expr_str_ast_inner(&self, expr: &eir::Expr, statement: bool) -> Code {
        match expr {
            //Expr::Missing => "/* missing */default!".into(),
            eir::Expr::Literal(lit) => self.emit_literal_ast(lit),
            eir::Expr::PathExpr(path) => match self.eir_sem.resolve_path_with_subst(path.path()) {
                None => {
                    eprintln!(
                        "Unresolved Path at {loc}",
                        loc = self.eir_sem.location(expr),
                    );
                    fcode!("/* {} */", path.syntax_text())
                }
                Some((hir::PathResolution::Def(hir::ModuleDef::Function(f)), args)) => {
                    let args = args.map(|x| x.types(self.db)).unwrap_or_default();
                    match self.resolve_function(f, args) {
                        // simple: path simply represents static or module functions
                        ResolvedFunction::Static {
                            self_ty,
                            trait_: _,
                            function_name,
                            generic_sources,
                            generic_args: args,
                            args_map: None,
                        } => {
                            let mut path = self
                                .rust_type_to_cs_options(&self_ty, CsTypeOption::static_access());
                            path.push('.');
                            path.push_str(&function_name);
                            path = generic_args(
                                path,
                                self.map_cs_type_param_source(&generic_sources, &args),
                            );
                            path.into()
                        }
                        ResolvedFunction::ModuleFunction {
                            function_path,
                            generic_sources: _,
                            generic_args: _,
                            args_map: None,
                        } => function_path.into(),

                        // other cases require wrapping with lambda expression
                        resolved => {
                            let num_params = f.num_params(self.db);

                            code!(
                                "((",
                                join((0..num_params).map(|i| fcode!("_p_{i}")), ", "),
                                ") => ",
                                self.emit_call_expr(
                                    resolved,
                                    (0..num_params).map(|i| fcode!("_p_{i}"))
                                ),
                                ")"
                            )
                        }
                    }
                }
                Some((hir::PathResolution::Def(module_def), _))
                    if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                {
                    let expr_type = self.eir_sem.type_of_expr(expr).original;
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
                                loc = self.eir_sem.location(path),
                                //type = self.new_type(infer).display(
                                //    self.db,
                                //    self.krate.to_display_target(self.db)
                                //)
                            );

                            fcode!("new /* unexpected struct/enum kind and infer */ UnknownType")
                        }
                    }
                }
                Some((hir::PathResolution::SelfType(impl_), _))
                    if let self_ty = impl_.self_ty(self.db)
                        && let Some(hir::Adt::Struct(struct_)) = self_ty.as_adt() =>
                {
                    let def = ConstructableDef::from(struct_);
                    let expr_type = self.eir_sem.type_of_expr(expr).original;
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
                                loc = self.eir_sem.location(path),
                                //type = self.new_type(infer).display(
                                //    self.db,
                                //    self.krate.to_display_target(self.db)
                                //)
                            );

                            fcode!("new /* unexpected struct/enum kind and infer */ UnknownType")
                        }
                    }
                }
                Some((hir::PathResolution::SelfType(_), _)) => {
                    eprintln!("ImplSelf at {}", self.eir_sem.location(expr));
                    "(ImplSelf)".into()
                }
                Some((hir::PathResolution::Local(local), _)) => self.binding_name_ast(local).into(),
                Some((hir::PathResolution::Def(hir::ModuleDef::Const(const_)), _)) => {
                    self.const_path_cs(const_).into()
                }
                Some((hir::PathResolution::Def(hir::ModuleDef::Static(static_)), _)) => {
                    let mut path = self.module_class_cs(static_.module(self.db));
                    path.push('.');
                    path.push_str(&names::static_name(static_.name(self.db).as_str()));
                    path.into()
                }
                Some((hir::PathResolution::TypeParam(_param), _)) => {
                    eprintln!("GenericParam at {}", self.eir_sem.location(expr));
                    "(GenericParam)".into()
                }

                Some((resolved, _)) => {
                    eprintln!(
                        "Path at {loc}: {resolved:?}",
                        loc = self.eir_sem.location(expr),
                    );
                    fcode!("/* {} */", path.syntax_text())
                }
            },
            eir::Expr::FieldExpr(field_expr) => {
                //let _name = field_expr.name_ref().unwrap();
                let receiver_part = field_expr.expr();
                let receiver_type = self.eir_sem.type_of_expr(receiver_part);
                let receiver = self.emit_expr_str_ast(receiver_part);
                let adjuster = if let Some(adjusted) = receiver_type.adjusted
                    && std::iter::successors(
                        receiver_type.original.as_reference().map(|x| x.0),
                        |c| c.as_reference().map(|x| x.0),
                    )
                    .all(|remove_ref| adjusted != remove_ref)
                {
                    tracing::trace!(
                        "adjusted type {original} to {adjusted} to access {name}",
                        original = receiver_type.original.debug_display(self.db),
                        adjusted = adjusted.debug_display(self.db),
                        name = field_expr.name_ref().text(),
                    );
                    ".m_Deref()"
                } else {
                    ""
                };
                match self.eir_sem.resolve_field(field_expr) {
                    None => {
                        eprintln!("Unresolved field at {}", self.eir_sem.location(field_expr));
                        code!(receiver, adjuster, "./* unresolved field */")
                    }
                    Some(Either::Left(field)) => {
                        code!(receiver, adjuster, ".", self.field_name(&field))
                    }
                    Some(Either::Right(field)) => {
                        code!(receiver, adjuster, ".Item", field.index + 1)
                    }
                }
            }
            eir::Expr::MethodCallExpr(method_call) => {
                let receiver = self.emit_expr_str_ast(method_call.receiver());

                let db = self.db;

                match self.eir_sem.resolve_method_call_fallback(method_call) {
                    Some((Either::Left(f), args)) => {
                        let args = args.map(|args| args.types(self.db)).unwrap_or_else(|| {
                            // this is builtin derive. all except for Hash::hash implementation
                            if *f.name(self.db).symbol() == sym::hash {
                                vec![(
                                    hir::Symbol::intern("H"),
                                    hir::Type::error(self.db, f.krate(self.db)),
                                )]
                            } else {
                                vec![]
                            }
                        });
                        let resolved = self.resolve_function(f, args);
                        self.emit_call_expr(
                            resolved,
                            [receiver].into_iter().chain(
                                method_call
                                    .arg_list()
                                    .args()
                                    .map(|arg| self.emit_expr_str_ast(arg)),
                            ),
                        )
                    }
                    Some((Either::Right(_), _)) => {
                        eprintln!(
                            "method call resolved to field at {}",
                            self.eir_sem.location(method_call)
                        );
                        let method_cs = method_call.name_ref().text().to_string();
                        let args = method_call
                            .arg_list()
                            .args()
                            .map(|a| self.emit_expr_str_ast(a));
                        code!(receiver, ".", method_cs, "(", join(args, ", "), ")")
                    }
                    None => {
                        eprintln!(
                            "Unresolved method call at {}",
                            self.eir_sem.location(method_call)
                        );
                        let method_cs = method_call.name_ref().text().to_string();
                        let args = method_call
                            .arg_list()
                            .args()
                            .map(|a| self.emit_expr_str_ast(a));
                        code!(receiver, ".", method_cs, "(", join(args, ", "), ")")
                    }
                }
            }
            eir::Expr::CallExpr(call_expr) => {
                let callee = call_expr.expr();
                let args = call_expr.arg_list().args();
                if let eir::Expr::PathExpr(path) = &callee {
                    match self.eir_sem.resolve_path_with_subst(path.path()) {
                        Some((PathResolution::Def(def), _))
                            if let Some(def) = ConstructableDef::from_module_def(def) =>
                        {
                            let expr_type = self.eir_sem.type_of_expr(expr).original;
                            let generic_args = expr_type.expect_adt_of(def.adt(self.db));
                            let callee_type =
                                self.constructable_name_cs(&Constructable::new(def, generic_args));
                            let args_str = args.map(|a| self.emit_expr_str_ast(a));
                            return code!(callee_type, ".ctor(", join(args_str, ", "), ")");
                        }
                        Some((PathResolution::Def(ModuleDef::Function(f)), subst)) => {
                            let generics =
                                subst.map(|args| args.types(self.db)).unwrap_or_else(|| {
                                    // this is builtin derive. all except for Hash::hash implementation
                                    if *f.name(self.db).symbol() == sym::hash {
                                        vec![(
                                            hir::Symbol::intern("H"),
                                            hir::Type::error(self.db, f.krate(self.db)),
                                        )]
                                    } else {
                                        vec![]
                                    }
                                });
                            let resolved = self.resolve_function(f, generics);
                            return self.emit_call_expr(
                                resolved,
                                args.into_iter().map(|a| self.emit_expr_str_ast(a)),
                            );
                        }
                        _ => {}
                    }
                }

                let callee_str = self.emit_expr_str_ast(callee);
                let args_str = args.map(|a| self.emit_expr_str_ast(a));
                code!(callee_str, "(", join(args_str, ", "), ")")
            }
            eir::Expr::AwaitExpr(await_expr) => {
                let inner_str = self.emit_expr_str_ast(await_expr.expr());
                code!("(await ", inner_str, ")")
            }
            eir::Expr::BinExpr(bin_expr) => {
                let lhs_code = self.emit_expr_str_ast(bin_expr.lhs());
                let rhs_code = self.emit_expr_str_ast(bin_expr.rhs());
                let op_str = match bin_expr.op_kind() {
                    BinaryOp::ArithOp(ArithOp::Add) => "+",
                    BinaryOp::ArithOp(ArithOp::Mul) => "*",
                    BinaryOp::ArithOp(ArithOp::Sub) => "-",
                    BinaryOp::ArithOp(ArithOp::Div) => "/",
                    BinaryOp::ArithOp(ArithOp::Rem) => "%",
                    BinaryOp::ArithOp(ArithOp::Shl) => "<<",
                    BinaryOp::ArithOp(ArithOp::Shr) => ">>",
                    BinaryOp::ArithOp(ArithOp::BitXor) => "^",
                    BinaryOp::ArithOp(ArithOp::BitOr) => "|",
                    BinaryOp::ArithOp(ArithOp::BitAnd) => "&",
                    BinaryOp::CmpOp(c) => {
                        use eir::{CmpOp, Ordering};
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
                    BinaryOp::LogicOp(LogicOp::And) => "&&",
                    BinaryOp::LogicOp(LogicOp::Or) => "&&",

                    BinaryOp::Assignment { op } => match op {
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
            eir::Expr::PrefixExpr(prefix_expr) => {
                let inner_str = self.emit_expr_str_ast(prefix_expr.expr());
                match prefix_expr.op_kind() {
                    UnaryOp::Deref => code!("(/* deref_op */", inner_str, ")"),
                    UnaryOp::Not => code!("!(", inner_str, ")"),
                    UnaryOp::Neg => code!("-(", inner_str, ")"),
                }
            }
            eir::Expr::RefExpr(ref_expr) => self.emit_expr_str_ast(ref_expr.expr()),
            eir::Expr::CastExpr(cast_expr) => {
                // TODO: Cast might not be compatible
                code!(
                    "(",
                    self.rust_type_to_cs(&self.eir_sem.resolve_type(cast_expr.ty()).unwrap()),
                    ") ",
                    self.emit_expr_str_ast(cast_expr.expr())
                )
            }
            eir::Expr::IfExpr(if_expr) => self.emit_if_expr_as_expr(if_expr),
            eir::Expr::BlockExpr(block_expr) => match block_expr.modifier() {
                None => self.emit_simple_block_as_expr(block_expr),
                Some(eir::BlockModifier::Async) => {
                    let mut body_code = Code::new();
                    let return_type = (self.eir_sem.type_of_expr(expr))
                        .adjusted()
                        .future_output(self.db)
                        .unwrap();

                    let return_type_cs = self.rust_type_to_cs(&return_type);
                    {
                        let _scope = self.new_ctx(CodeContext {
                            is_async: true,
                            returning: return_type,
                        });
                        self.emit_returning_block(&mut body_code, block_expr);
                    }

                    code!(
                        "RustTask.New<",
                        return_type_cs,
                        ">(async () => {\n",
                        indent,
                        body_code,
                        dedent,
                        "})"
                    )
                }
                Some(modifier) => panic!(
                    "Unsupported block modifier: {} at {}",
                    match modifier {
                        eir::BlockModifier::Async => "async",
                        eir::BlockModifier::Unsafe => "unsafe",
                        //eir::BlockModifier::Try { .. } => "try",
                        eir::BlockModifier::Const => "const",
                        eir::BlockModifier::AsyncGen => "async gen",
                        eir::BlockModifier::Gen => "gen",
                        eir::BlockModifier::Label(_) => "label",
                    },
                    self.eir_sem.location(expr)
                ),
            },
            eir::Expr::TupleExpr(tuple_expr) => {
                if tuple_expr.fields().next().is_none() {
                    "default(global::System.ValueTuple)".into()
                } else {
                    let parts = tuple_expr.fields().map(|e| self.emit_expr_str_ast(e));
                    code!("(", join(parts, ", "), ")")
                }
            }
            eir::Expr::RecordExpr(record_expr) => {
                let c = match self.eir_sem.resolve_variant(record_expr) {
                    None => {
                        eprintln!(
                            "Struct construction without type in {}",
                            self.eir_sem.location(expr)
                        );

                        return fcode!("(TypelessConstruction/*{}*/)", record_expr.syntax_text());
                    }
                    Some(variant) if let Some(def) = ConstructableDef::from_variant(variant) => {
                        let ty_args = (self.eir_sem.type_of_expr(expr).original)
                            .expect_adt_of(def.adt(self.db));

                        Constructable::new(def, ty_args)
                    }
                    Some(_) => {
                        eprintln!("Union unsupported at {}", self.eir_sem.location(expr));

                        return fcode!("(UnionConstruction/*{}*/)", record_expr.syntax_text());
                    }
                };
                let type_name = self.constructable_name_cs(&c);

                let field_inits = record_expr
                    .record_expr_field_list()
                    .fields()
                    .filter(|&f| self.check_cfg(f))
                    .map(|f| {
                        let cs_f = names::field_name(f.field_name().unwrap().text());
                        let field = c
                            .fields(self.db)
                            .into_iter()
                            .find(|x| x.name(self.db).as_str() == f.field_name().unwrap().text());
                        let _cs_ty = field
                            .map(|f| self.rust_type_to_cs(&f.ty(self.db)))
                            .unwrap_or_else(|| "object /*unknown field type*/".into());
                        let val = self.emit_expr_str_ast(f.expr());
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
            eir::Expr::IndexExpr(index_expr) => {
                let base_str = self.emit_expr_str_ast(index_expr.base());
                let idx_str = self.emit_expr_str_ast(index_expr.index());
                code!(base_str, ".Index(", idx_str, ")")
            }
            eir::Expr::RangeExpr(range_expr) => {
                fn single_generics<'db>(sem: &EirSemantics<'db>, expr: &eir::Expr) -> Type<'db> {
                    let [type_] = <[_; 1]>::try_from(
                        sem.type_of_expr(expr)
                            .original
                            .as_adt_with_args()
                            .unwrap()
                            .1,
                    )
                    .unwrap();
                    type_.unwrap()
                }

                match (range_expr.start(), range_expr.op_kind(), range_expr.end()) {
                    (Some(start), RangeOp::Exclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.eir_sem, expr)),
                            ">.NewRange(",
                            self.emit_expr_str_ast(start),
                            ", ",
                            self.emit_expr_str_ast(end),
                            ")"
                        )
                    }
                    (Some(start), RangeOp::Inclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.eir_sem, expr)),
                            ">.NewRangeInclusive(",
                            self.emit_expr_str_ast(start),
                            ", ",
                            self.emit_expr_str_ast(end),
                            ")"
                        )
                    }
                    (Some(start), _, None) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.eir_sem, expr)),
                            ">.NewRangeFrom(",
                            self.emit_expr_str_ast(start),
                            ")"
                        )
                    }
                    (None, _, None) => {
                        code!("RangeFull.NewRangeFull()")
                    }
                    (None, RangeOp::Exclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.eir_sem, expr)),
                            ">.NewRangeTo(",
                            self.emit_expr_str_ast(end),
                            ")"
                        )
                    }
                    (None, RangeOp::Inclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.eir_sem, expr)),
                            ">.NewRangeToInclusive(",
                            self.emit_expr_str_ast(end),
                            ")"
                        )
                    }
                }
            }
            eir::Expr::ArrayRepeatExpr(repeat) => {
                let val = self.emit_expr_str_ast(repeat.initializer());
                let len = self.emit_expr_str_ast(repeat.repeat());
                code!("new object[", len, "] /* fill ", val, "*/")
            }
            eir::Expr::ArrayElementListExpr(elem_list) => {
                let Some((element, _len)) =
                    self.eir_sem.type_of_expr(expr).original.as_array(self.db)
                else {
                    panic!("");
                };
                let cg = self.cg;
                let parts = elem_list.elements().map(|e| self.emit_expr_str_ast(e));
                code!(
                    "new ",
                    cg.rust_type_to_cs(&element),
                    "[] { ",
                    join(parts, ", "),
                    " }"
                )
            }
            eir::Expr::ClosureExpr(closure) => {
                let param_names = closure.param_list().params().enumerate();
                let params: Vec<String> = (param_names.clone())
                    .map(|(i, p)| {
                        if let eir::Pat::IdentPat(ident_pat) = p.pat()
                            && let Some(local) = self.eir_sem.to_def(ident_pat)
                        {
                            self.alloc_binding_ast(&local)
                        } else {
                            format!("__cp{}", i)
                        }
                    })
                    .collect();

                let mut body_block = Code::new();
                for (i, p) in param_names.clone() {
                    if let eir::Pat::IdentPat(ident_pat) = p.pat()
                        && let Some(_) = self.eir_sem.to_def(ident_pat)
                    {
                    } else {
                        self.emit_let_stmt(
                            &mut body_block,
                            p.pat(),
                            code!(&format!("__cp{}", i)),
                            Self::emit_unreachable,
                        );
                    }
                }

                let output = if let Some(type_of_expr) = self.eir_sem.type_of_expr_opt(expr) {
                    let type_of_expr = type_of_expr.adjusted();
                    assert!(type_of_expr.impls_fnonce(self.db));
                    type_of_expr
                        .normalize_trait_assoc_type(
                            self.db,
                            &[],
                            self.lang_items.FnOnceOutput().unwrap(),
                        )
                        .expect("No output for fn")
                } else {
                    // arbitary type
                    self.cg.lang_items.Result().unwrap().ty(self.db)
                };
                let _scope = self.new_ctx(CodeContext {
                    is_async: false, // TODO
                    returning: output,
                });

                if let eir::Expr::BlockExpr(block) = closure.body()
                    && block.modifier().is_none()
                {
                    self.emit_block_contents(
                        &mut body_block,
                        block.statements(),
                        block.tail_expr(),
                        ExprGenOption::returning(),
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
                    let body_str = self.emit_expr_str_ast(closure.body());
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
            eir::Expr::ReturnExpr(return_expr) => {
                let val = return_expr
                    .expr()
                    .map(|e| self.emit_expr_str_ast(e))
                    .unwrap_or_else(|| "0 /* unit */".into());
                code!(
                    "/* return-expr */ throw r2CsRuntime.Helpers.Returns<object>(",
                    val,
                    ")"
                )
            }
            eir::Expr::LetExpr(let_expr) => {
                let val = self.emit_expr_str_ast(let_expr.expr());
                let pattern_cs = self.emit_pattern_ast(let_expr.pat());
                code!("(", val, " is ", pattern_cs, ")")
            }
            eir::Expr::MatchExpr(match_expr) => {
                let arms = match_expr.match_arm_list();
                let match_expr = match_expr.expr();
                let scrutinee = self.emit_expr_str_ast(match_expr);

                let mut out = Code::new();

                out.w("(").w(scrutinee).wln(") switch {");
                out.indent();

                for arm in arms.arms() {
                    if !self.check_cfg(arm) {
                        continue;
                    }
                    // Emit pattern check
                    let pat_cs = self.emit_pattern_ast(arm.pat());

                    out.w("").w(pat_cs).w(" ");
                    if let Some(guard) = arm.guard() {
                        out.w("when ");
                        out.w(self.emit_expr_str_ast(guard));
                    }
                    out.w("=> ").w(self.emit_expr_str_ast(arm.expr())).wln(",");
                }
                out.dedent();

                out.w("}");

                out
            }
            eir::Expr::BreakExpr(break_expr) => break_expr
                .expr()
                .map(|e| self.emit_expr_str_ast(e))
                .unwrap_or_else(|| "default!".into()),
            eir::Expr::ContinueExpr(_) => "default!".into(),
            eir::Expr::UnderscoreExpr(_) => "_".into(),
            eir::Expr::LoopExpr(_) | eir::Expr::ForExpr(_) | eir::Expr::WhileExpr(_) => {
                // TODO
                "/* loop */ default!".into()
            }
            eir::Expr::FormatArgsExpr(args) => {
                let mut result = code!(r##"$""##);

                for segment in args.segments() {
                    match segment {
                        eir::FormatArgsSegment::Literal(literal) => {
                            write!(result, "{}", literal.escape_default());
                        }
                        eir::FormatArgsSegment::Braces(c) => write!(result, "{c}{c}"), // C# also uses '{' '}'
                        eir::FormatArgsSegment::Expr(expr) => {
                            result.w("{").w(self.emit_expr_str_ast(expr)).w("}");
                        }
                    }
                }

                result.w("\"");

                result
            }
            eir::Expr::TryExpr(try_expr) => {
                code!("Try(", self.emit_expr_str_ast(try_expr.expr()), ")")
            }
            eir::Expr::VecRepeatExpr(repeat) => {
                let val = self.emit_expr_str_ast(repeat.initializer());
                let len = self.emit_expr_str_ast(repeat.repeat());
                code!("new object[", len, "] /* fill ", val, "*/")
            }
            eir::Expr::VecListExpr(elem_list) => {
                let Some((adt, generics)) =
                    self.eir_sem.type_of_expr(expr).original.as_adt_with_args()
                else {
                    panic!("");
                };
                assert_eq!(Some(adt), self.cg.lang_items.Vec().map(hir::Adt::Struct));
                let element = generics[0].as_ref().unwrap().clone();
                let cg = self.cg;
                if elem_list.elements().next().is_some() {
                    let parts = elem_list.elements().map(|e| self.emit_expr_str_ast(e));
                    code!(
                        "new List<",
                        cg.rust_type_to_cs(&element),
                        "> { ",
                        join(parts, ", "),
                        " }"
                    )
                } else {
                    code!("new List<", cg.rust_type_to_cs(&element), ">()")
                }
            }
            eir::Expr::RawCodeExpr(raw_cod) => raw_cod.code().clone(),
            eir::Expr::MacroStmts(_) => panic!("Unexpected macro stmts"),
        }
    }

    fn emit_if_expr_as_expr(&self, if_expr: &eir::IfExpr) -> Code {
        match if_expr.else_branch() {
            Some(eir::ElseBranch::Block(else_block)) => {
                let cond = self.emit_expr_str_ast(if_expr.condition());
                let then_s = self.emit_simple_block_as_expr(if_expr.then_branch());
                let else_s = self.emit_simple_block_as_expr(else_block);
                code!("(", cond, " ? ", then_s, " : ", else_s, ")")
            }
            Some(eir::ElseBranch::IfExpr(if_expr)) => {
                let cond = self.emit_expr_str_ast(if_expr.condition());
                let then_s = self.emit_simple_block_as_expr(if_expr.then_branch());
                let else_s = self.emit_if_expr_as_expr(if_expr);
                code!("(", cond, " ? ", then_s, " : ", else_s, ")")
            }
            None => {
                let cond = self.emit_expr_str_ast(if_expr.condition());
                let then_s = self.emit_simple_block_as_expr(if_expr.then_branch());
                code!("(", cond, " ? ", then_s, " : ValueTuple)")
            }
        }
    }

    fn emit_simple_block_as_expr(&self, block_expr: &eir::BlockExpr) -> Code {
        if block_expr.statements().count() == 0 {
            if let Some(t) = block_expr.tail_expr() {
                return self.emit_expr_str_ast(t);
            }
            return "default!".into();
        }
        "/* block expr */ default!".into()
    }

    fn emit_returning_block(&self, out: &mut Code, block_expr: &eir::BlockExpr) {
        let statements = block_expr.statements();
        let tail = block_expr.tail_expr();
        let emit_info = self.emit_block_contents(out, statements, tail, ExprGenOption::returning());
        if !emit_info.diverging && !self.returning_type().is_unit() {
            tracing::error!(
                "Error emitting expr: non-void returning expr does not diverging at {}",
                self.eir_sem.location(block_expr)
            );
        }
        if self.returning_type().is_unit() && self.is_async() && !emit_info.diverging {
            out.wln("return default;");
        }
    }

    fn emit_literal_ast(&self, lit: &eir::Literal) -> Code {
        match &lit.kind() {
            eir::LiteralKind::Bool(b) => b.to_string().into(),
            eir::LiteralKind::IntNumber(v) => v.value().unwrap().to_string().into(),
            eir::LiteralKind::FloatNumber(v) => v.value_string().into(),
            eir::LiteralKind::Char(c) => fcode!("'{}'", c.value().unwrap().escape_default()),
            eir::LiteralKind::String(s) => fcode!("\"{}\"", s.value().unwrap().escape_default(),),
            eir::LiteralKind::Byte(b) => b.value().unwrap().to_string().into(),
            eir::LiteralKind::ByteString(_) => "/* byte string */ new byte[] {}".into(),
            eir::LiteralKind::CString(_) => "/* cstring */ \"\"".into(),
        }
    }

    fn emit_call_expr(
        &self,
        function: ResolvedFunction<'db>,
        mut args: impl Iterator<Item = Code>,
    ) -> Code {
        fn code_solver<'a>(args: &'a [Code]) -> impl Fn(&'a ArgSource) -> &'a Code {
            |source| match *source {
                ArgSource::Source(i) => &args[i],
                ArgSource::CustomExpr(ref e) => e,
            }
        }

        match function {
            ResolvedFunction::Static {
                self_ty,
                trait_: _,
                function_name,
                generic_sources,
                generic_args: generics,
                args_map,
            } => {
                let mut path =
                    self.rust_type_to_cs_options(&self_ty, CsTypeOption::static_access());
                path.push('.');
                path.push_str(&function_name);
                path = generic_args(
                    path,
                    self.map_cs_type_param_source(&generic_sources, &generics),
                );
                match args_map {
                    None => {
                        code!(path, "(", join(args, ", "), ")")
                    }
                    Some(map) => {
                        let args = args.collect::<Vec<_>>();
                        code!(
                            path,
                            "(",
                            join(map.iter().map(code_solver(&args)), ", "),
                            ")"
                        )
                    }
                }
            }
            ResolvedFunction::Method {
                self_ty: _,
                trait_: _,
                function_name,
                generic_sources,
                generic_args: generics,
                args_map,
            } => {
                let reference = if matches!(
                    function_name.as_str(),
                    "m_VisitStr"
                        | "m_VisitBorrowedStr"
                        | "m_VisitString"
                        | "m_VisitBool"
                        | "m_VisitI8"
                        | "m_VisitI16"
                        | "m_VisitI32"
                        | "m_VisitI64"
                        | "m_VisitI128"
                        | "m_VisitU8"
                        | "m_VisitU16"
                        | "m_VisitU32"
                        | "m_VisitU64"
                        | "m_VisitU128"
                        | "m_VisitF32"
                        | "m_VisitF64"
                        | "m_VisitChar"
                        | "m_VisitBytes"
                        | "m_VisitBorrowedBytes"
                        | "m_VisitByteBuf"
                        | "m_VisitNone"
                        | "m_VisitUnit"
                        | "m_VisitSome"
                        | "m_VisitNewtypeStruct"
                        | "m_VisitSeq"
                        | "m_VisitMap"
                        | "m_VisitEnum"
                    // ***Access methods
                        | "m_NextKey"
                        | "m_NextElement"
                    // Visitor Wrappers
                        | "m_NewMapKeyDeserializer"
                        | "m_NewDedupForwarderVisitor"
                ) {
                    generic_args(
                        function_name,
                        self.map_cs_type_param_source(&generic_sources, &generics),
                    )
                } else {
                    function_name
                };

                match args_map {
                    None => {
                        let receiver = args.next().unwrap();
                        code!(receiver, ".", reference, "(", join(args, ", "), ")")
                    }
                    Some((self_i, map)) => {
                        let args = args.collect::<Vec<_>>();
                        code!(
                            code_solver(&args)(&self_i),
                            ".",
                            reference,
                            "(",
                            join(map.iter().map(code_solver(&args)), ", "),
                            ")"
                        )
                    }
                }
            }
            ResolvedFunction::ModuleFunction {
                function_path,
                generic_sources,
                generic_args: generics,
                args_map,
                ..
            } => {
                let path = generic_args(
                    function_path,
                    self.map_cs_type_param_source(&generic_sources, &generics),
                );
                match args_map {
                    None => {
                        code!(path, "(", join(args, ", "), ")")
                    }
                    Some(map) => {
                        let args = args.collect::<Vec<_>>();
                        code!(
                            path,
                            "(",
                            join(map.iter().map(code_solver(&args)), ", "),
                            ")"
                        )
                    }
                }
            }
            ResolvedFunction::OmitCall { comment } => {
                let value = args.next().unwrap();
                code!(comment, "(", value, ")")
            }
        }
    }

    /// Emit a pattern as a condition check against a scrutinee expression.
    fn emit_pattern_ast(&self, pat: &eir::Pat) -> Code {
        match pat {
            eir::Pat::WildcardPat(_w) => "{} _".into(),
            eir::Pat::IdentPat(ident_pat)
                if let Some(const_ref) = self.eir_sem.resolve_bind_pat_to_const(ident_pat) =>
            {
                match const_ref {
                    const_ref
                        if let Some(constructable) =
                            ConstructableDef::from_module_def(const_ref) =>
                    {
                        let ty_args = (self.eir_sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(constructable.adt(self.db));

                        fcode!(
                            "{}",
                            self.constructable_name_cs(&Constructable::new(constructable, ty_args))
                        )
                    }
                    _ => {
                        // TODO
                        code!(format!("/* Unresolved ident */{:?}", const_ref))
                    }
                }
            }
            eir::Pat::IdentPat(ident_pat) => {
                let local = self.eir_sem.to_def(ident_pat).unwrap();
                let cs_local_name = self.alloc_binding_ast(&local);
                if let Some(sub) = ident_pat.pat() {
                    code!(self.emit_pattern_ast(sub), " ", cs_local_name)
                } else {
                    code!("var ", cs_local_name)
                }
            }
            eir::Pat::TupleStructPat(tuple_struct) => {
                let path = tuple_struct.path();

                let (cs_type_name, field_count) = match self.eir_sem.resolve_path(path) {
                    Some(PathResolution::Def(module_def))
                        if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                    {
                        let ty_args = (self.eir_sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(def.adt(self.db));
                        let field_count = def.fields(self.db).len();
                        let cs_variant =
                            self.constructable_name_cs(&Constructable::new(def, ty_args));
                        (cs_variant, field_count)
                    }
                    resolved => {
                        eprintln!(
                            "Unable to resolve path in pattern {resolved:?}: {}",
                            self.eir_sem.location(pat)
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
            eir::Pat::PathPat(path) => {
                let path = path.path();
                match self.eir_sem.resolve_path(path) {
                    Some(PathResolution::Def(ModuleDef::Const(const_))) => {
                        code!(self.const_path_cs(const_))
                    }
                    Some(PathResolution::Def(module_def))
                        if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                    {
                        let ty_args = (self.eir_sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(def.adt(self.db));
                        code!(self.constructable_name_cs(&Constructable::new(def, ty_args)))
                    }
                    resolved => {
                        eprintln!(
                            "Unable to resolve path in pattern {resolved:?}: {}",
                            self.eir_sem.location(pat)
                        );
                        fcode!("Unknown /* bad path resolution: {} */", path.syntax_text())
                    }
                }
            }
            eir::Pat::RecordPat(record_pat) => {
                let path = record_pat.path();
                let (cs_type_name, _field_count) = match self.eir_sem.resolve_path(path) {
                    Some(PathResolution::Def(module_def))
                        if let Some(def) = ConstructableDef::from_module_def(module_def) =>
                    {
                        let ty_args = (self.eir_sem.type_of_pat(pat).unwrap().original)
                            .expect_adt_of(def.adt(self.db));
                        let field_count = def.fields(self.db).len();
                        let cs_variant =
                            self.constructable_name_cs(&Constructable::new(def, ty_args));
                        (cs_variant, field_count)
                    }
                    resolved => {
                        eprintln!(
                            "Unable to resolve path in pattern {resolved:?}: {}",
                            self.eir_sem.location(pat)
                        );
                        return fcode!(
                            "Unknown /* Bad record path resolution: {} */",
                            path.syntax_text()
                        );
                    }
                };

                let fields = record_pat.record_pat_field_list();
                let fields = fields.fields().collect::<Vec<_>>();
                if matches!(fields.as_slice(), []) {
                    code!(cs_type_name, "{", "}")
                } else {
                    let pattern_codes = fields.iter().map(|field| {
                        let field_name = field.field_name().syntax_text().to_string();
                        let field_name_cs = names::field_name(&field_name);
                        if let Some(field_pat) = Some(field.pat()) {
                            code!(
                                format("{field_name_cs}: "),
                                self.emit_pattern_ast(field_pat)
                            )
                        } else {
                            eprintln!(
                                "Unable to create variable from field in pattern: {}",
                                self.eir_sem.location(pat)
                            );
                            code!(format("{field_name_cs}: var /*unresolved*/"), field_name)
                        }
                    });
                    code!(cs_type_name, "{", join(pattern_codes, ","), "}")
                }
            }
            eir::Pat::LiteralPat(literal_pat) => {
                self.emit_expr_str_ast(&eir::Expr::Literal(literal_pat.literal().clone()))
            }
            eir::Pat::OrPat(or_pat) => {
                let parts = or_pat.pats().map(|p| self.emit_pattern_ast(p));
                code!("(", join(parts, " or "), ")")
            }
            eir::Pat::SlicePat(slice_pat) => {
                let components = slice_pat.components();
                assert!(components.slice().is_none());
                code!(
                    "[",
                    join(
                        components.prefix().iter().map(|p| self.emit_pattern_ast(p)),
                        ","
                    ),
                    "]"
                )
            }
            eir::Pat::TuplePat(tuple_pat) => {
                let type_ = self.eir_sem.type_of_pat(pat).unwrap();
                let field_count = type_.original.tuple_fields(self.db).len();
                let pattern_codes = self.resolve_tuple_like_struct(
                    field_count,
                    &tuple_pat.fields().collect::<Vec<_>>(),
                );

                code!("(", join(pattern_codes, ","), ")")
            }
            eir::Pat::RangePat(range_pat) => {
                match (range_pat.start(), range_pat.op_kind(), range_pat.end()) {
                    (Some(lower), RangeOp::Exclusive, Some(upper)) => {
                        code!(
                            "(>=",
                            self.emit_pattern_ast(lower),
                            " and <",
                            self.emit_pattern_ast(upper),
                            ")"
                        )
                    }
                    (Some(lower), RangeOp::Inclusive, Some(upper)) => {
                        code!(
                            "(>=",
                            self.emit_pattern_ast(lower),
                            " and <=",
                            self.emit_pattern_ast(upper),
                            ")"
                        )
                    }
                    (Some(lower), RangeOp::Exclusive, None) => {
                        code!("(>=", self.emit_pattern_ast(lower), ")")
                    }
                    (None, RangeOp::Exclusive, Some(upper)) => {
                        code!("(<", self.emit_pattern_ast(upper), ")")
                    }
                    (None, RangeOp::Inclusive, Some(upper)) => {
                        code!("(<=", self.emit_pattern_ast(upper), ")")
                    }
                    (lower, op, upper) => {
                        panic!(
                            "Bad pattern: {lower:?} {op:?} {upper:?}",
                            lower = lower.map(|_| "pattern"),
                            upper = upper.map(|_| "pattern")
                        )
                    }
                }
            }
            eir::Pat::RefPat(ref_pat) => self.emit_pattern_ast(ref_pat.pat()),
            eir::Pat::RestPat(_) => panic!("Bad pattern: RestPat"),
        }
    }

    fn resolve_tuple_like_struct(&self, field_count: usize, patterns: &[&eir::Pat]) -> Vec<Code> {
        if matches!(patterns, [eir::Pat::RestPat(_)]) {
            vec![code!("_"); field_count]
        } else if let Some(position) = patterns
            .iter()
            .position(|p| matches!(p, eir::Pat::RestPat(_)))
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
            let pattern_codes = patterns.iter().map(|pat| self.emit_pattern_ast(pat));
            pattern_codes.collect::<Vec<_>>()
        }
    }

    /// Collect all binding IDs in a pattern.
    fn collect_bindings_in_pat_ast(&self, pat: &eir::Pat) -> Vec<Local<'db>> {
        let mut result = Vec::new();
        self.collect_bindings_recursive_ast(pat, &mut result);
        result
    }

    fn collect_bindings_recursive_ast(&self, pat: &eir::Pat, result: &mut Vec<Local<'db>>) {
        match pat {
            eir::Pat::IdentPat(ident_pat) if let Some(local) = self.eir_sem.to_def(ident_pat) => {
                result.push(local);
                if let Some(sub) = ident_pat.pat() {
                    self.collect_bindings_recursive_ast(sub, result);
                }
            }
            eir::Pat::IdentPat(_) => {}
            eir::Pat::LiteralPat(_) => {}
            eir::Pat::OrPat(pat) => pat
                .pats()
                .for_each(|pat| self.collect_bindings_recursive_ast(pat, result)),
            eir::Pat::PathPat(_) => {}
            eir::Pat::RangePat(pat) => {
                if let Some(start) = pat.start() {
                    self.collect_bindings_recursive_ast(start, result)
                }
                if let Some(end) = pat.end() {
                    self.collect_bindings_recursive_ast(end, result)
                }
            }
            eir::Pat::RecordPat(pat) => {
                pat.record_pat_field_list()
                    .fields()
                    .for_each(|field| self.collect_bindings_recursive_ast(field.pat(), result));
            }
            eir::Pat::RefPat(pat) => self.collect_bindings_recursive_ast(pat.pat(), result),
            eir::Pat::RestPat(_) => {}
            eir::Pat::SlicePat(pat) => {
                let components = pat.components();
                (components.prefix().iter())
                    .for_each(|pat| self.collect_bindings_recursive_ast(pat, result));
                (components.suffix().iter())
                    .for_each(|pat| self.collect_bindings_recursive_ast(pat, result));
            }
            eir::Pat::TuplePat(pat) => pat
                .fields()
                .for_each(|pat| self.collect_bindings_recursive_ast(pat, result)),
            eir::Pat::TupleStructPat(pat) => pat
                .fields()
                .for_each(|pat| self.collect_bindings_recursive_ast(pat, result)),
            eir::Pat::WildcardPat(_) => {}
        }
    }
}
