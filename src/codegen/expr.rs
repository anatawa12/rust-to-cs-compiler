//! Generates C# expressions and statements from Rust HIR bodies.

mod macros;

use super::{CodeGenerator, generic_args, names, output::Code};
use crate::codegen::constructable::{Constructable, ConstructableDef};
use crate::codegen::decl::CsFunctionType;
use crate::codegen::function_resolution::{ArgSource, ResolvedFunction};
use crate::codegen::simple_extensions::*;
use crate::codegen::ty::CsTypeOption;
use hir::db::HirDatabase;
use hir::{
    HasContainer, HasCrate, Local, ModuleDef, PathResolution, Semantics, StructKind, Type,
    TypeInfo, sym,
};
use itertools::Either;
use ra_internal::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{BitAnd, BitAndAssign, BitOr};
use std::sync::atomic::AtomicUsize;
use syntax::ast::{
    self, ArithOp, AstNode as _, HasArgList as _, HasLoopBody as _, LogicOp, RangeItem as _,
};
use syntax::ast::{BinaryOp, RangeOp, UnaryOp};

/// Generates C# for the body of a single function.
pub struct BodyGen<'g, 'db> {
    cg: &'g CodeGenerator<'db>,
    /// Mapping from Local to the C# local name allocated for it.
    locals: RefCell<HashMap<Local, String>>,
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

impl<'g, 'db> std::ops::Deref for BodyGen<'g, 'db> {
    type Target = CodeGenerator<'db>;

    fn deref(&self) -> &Self::Target {
        self.cg
    }
}

struct CodeContext<'db> {
    is_async: bool,
    returning: hir::Type<'db>,
}

impl<'g, 'db> BodyGen<'g, 'db> {
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

    fn alloc_binding_ast(&self, local: &Local) -> String {
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

    fn add_local(&self, local: Local, name: String) {
        self.locals.borrow_mut().insert(local, name);
    }

    fn add_deferred(&self, deferred: impl Into<ItemInBody>) {
        self.deferred.borrow_mut().push(deferred.into());
    }

    pub fn deferred(&mut self) -> &mut Vec<ItemInBody> {
        self.deferred.get_mut()
    }

    fn inc_match_index(&self) -> usize {
        self.match_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn check_cfg(&self, value: &impl ast::HasAttrs) -> bool {
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

    #[track_caller]
    fn type_of_expr(&self, expr: &ast::Expr) -> TypeInfo<'db> {
        self.sem
            .type_of_expr(expr)
            .unwrap_or_else(|| panic!("unknown type expr at {}", self.expr_location_ast(expr)))
    }

    /// Emit the full function body block.
    #[tracing::instrument(skip_all)]
    pub fn emit_function_body(&self, f: ast::Fn, out: &mut Code) {
        let params = f.param_list().unwrap();
        let _scope = self.new_ctx(CodeContext {
            is_async: self.is_async(),
            returning: if self.is_async() {
                (self.sem.to_def(&f).unwrap().ret_type(self.db))
                    .future_output(self.db)
                    .unwrap()
            } else {
                (self.sem.to_def(&f).unwrap()).ret_type(self.db)
            },
        });
        if let Some(self_param) = params.self_param() {
            match self.cs_type {
                CsFunctionType::TraitDefaultImpl => {
                    self.add_local(self.sem.to_def(&self_param).unwrap(), "self".into());
                }
                _ => {
                    self.add_local(self.sem.to_def(&self_param).unwrap(), "this".into());
                }
            }
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
            self.emit_returning_block(out, &body);
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

impl<'g, 'db> BodyGen<'g, 'db> {
    /// Emit an expression as a statement (with semicolon if needed).
    fn emit_expr_as_stmt_ast(
        &self,
        out: &mut Code,
        expr: ast::Expr,
        option: ExprGenOption,
    ) -> EmittedExprInfo {
        if !self.check_cfg(&expr) {
            return EmittedExprInfo::non_diverging();
        }

        match expr {
            ast::Expr::BlockExpr(ref block_expr) if block_expr.modifier().is_none() => {
                let statements = block_expr.statements();
                let tail = block_expr.tail_expr();
                self.emit_block_contents(out, statements, tail, option)
            }
            ast::Expr::ReturnExpr(ret_expr) => {
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
            ast::Expr::TupleExpr(tuple_expr) if tuple_expr.fields().next().is_none() => {
                out.wln("/* () unit expr */;");
                EmittedExprInfo::non_diverging()
            }
            ast::Expr::IfExpr(if_expr) => {
                let condition = if_expr.condition().unwrap();
                let then_branch = if_expr.then_branch().unwrap();
                let else_branch = if_expr.else_branch();

                let cond = self.emit_expr_str_ast(&condition);
                out.w("if (").w(cond).wln(") {");
                out.indent();
                let then_part = self.emit_expr_as_stmt_ast(out, then_branch.into(), option.clone());
                out.dedent();
                let else_part = match else_branch {
                    Some(ast::ElseBranch::IfExpr(else_if)) => {
                        out.w("} else ");
                        self.emit_expr_as_stmt_ast(out, else_if.into(), option.clone())
                    }
                    Some(ast::ElseBranch::Block(else_e)) => {
                        out.w("} else {");
                        out.wln("");
                        out.indent();
                        let part = self.emit_expr_as_stmt_ast(out, else_e.into(), option.clone());
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
            ast::Expr::LoopExpr(loop_expr) => {
                let label = loop_expr.label();
                let body = loop_expr.loop_body().unwrap();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(l.lifetime().unwrap())))
                    .unwrap_or_default();
                out.w(label_str).wln("while (true) {");
                out.indent();
                self.emit_expr_as_stmt_ast(out, body.into(), option.non_last());
                out.dedent();
                out.wln("}");
                EmittedExprInfo::diverging()
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
                self.emit_expr_as_stmt_ast(out, body.into(), option.non_last());
                out.dedent();
                out.wln("}");

                EmittedExprInfo::non_diverging()
            }
            ast::Expr::ForExpr(for_expr) => {
                let label = for_expr.label();
                let pat = for_expr.pat().unwrap();
                let iterable = for_expr.iterable().unwrap();
                let body = for_expr.loop_body().unwrap();

                let label_str = label
                    .map(|l| format!("{}: ", self.label_name(l.lifetime().unwrap())))
                    .unwrap_or_default();
                let temp_name = format!("__temp_{}", self.inc_match_index());
                out.w(label_str)
                    .w("foreach (var ")
                    .w(&temp_name)
                    .w(" in ")
                    .w(self.emit_expr_str_ast(&iterable))
                    .wln(") {");
                out.indent();
                self.emit_let_stmt(out, &pat, code!(&temp_name), Self::emit_unreachable);
                self.emit_expr_as_stmt_ast(out, body.into(), option.non_last());
                out.dedent();
                out.wln("}");

                EmittedExprInfo::non_diverging()
            }
            // TODO? While and For
            ast::Expr::MatchExpr(match_expr) => {
                let arms = match_expr.match_arm_list().unwrap();
                let match_expr = match_expr.expr().unwrap();
                let scrutinee = self.emit_expr_str_ast(&match_expr);
                out.w("switch (").w(scrutinee).wln(") {");
                out.indent();

                let mut result = EmittedExprInfo::diverging();
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
                    let arm_res =
                        self.emit_expr_as_stmt_ast(out, arm.expr().unwrap(), option.clone());
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
                    EmittedExprInfo::non_diverging()
                } else {
                    out.wln(format!("break{};", label_str));
                    EmittedExprInfo::diverging()
                }
            }
            ast::Expr::ContinueExpr(continue_expr) => {
                let label = continue_expr.lifetime();
                // TODO: Labelled continue
                let label_str = label
                    .map(|l| format!(" /*{}*/", self.label_name(l)))
                    .unwrap_or_default();
                out.wln(format!("continue{};", label_str));
                EmittedExprInfo::diverging()
            }

            ast::Expr::AwaitExpr(await_expr) => {
                let inner_str = self.emit_expr_str_ast(&await_expr.expr().unwrap());
                if option.returning {
                    out.wln(code!("return await ", inner_str, ";"));
                    EmittedExprInfo::diverging()
                } else {
                    out.wln(code!("await ", inner_str, ";"));
                    EmittedExprInfo::non_diverging()
                }
            }
            ast::Expr::MacroExpr(ref m) if self.should_inline_macro(&m.macro_call().unwrap()) => {
                let macro_call = self
                    .sem
                    .expand_macro_call(&m.macro_call().unwrap())
                    .unwrap()
                    .value;
                let expr = ast::MacroStmts::cast(macro_call.clone())
                    .unwrap_or_else(|| panic!("{:?}", macro_call));
                self.emit_block_contents(out, expr.statements(), expr.expr(), option)
            }
            ast::Expr::MacroExpr(ref m) => {
                let (s, info) = self.emit_expr_macro(&expr, &m.macro_call().unwrap());
                if info.diverging {
                    out.w(s).wln(";");
                    info
                } else if option.returning && !self.returning_type().is_unit() {
                    out.w("return ").w(s).wln(";");
                    EmittedExprInfo::diverging()
                } else {
                    out.w(s).wln(";");
                    EmittedExprInfo::non_diverging()
                }
            }
            _ => {
                // Generic expression: emit as expression statement
                let s = self.emit_expr_str_ast_inner(&expr, true);
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
        pat: &ast::Pat,
        value_cs: Code,
        else_gen: impl FnOnce(&BodyGen<'g, 'db>, &mut Code),
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

    fn emit_unreachable(&self, out: &mut Code) {
        out.wln("throw new Exception(\"unreachable\");");
    }

    fn emit_block_contents(
        &self,
        out: &mut Code,
        statements: impl IntoIterator<Item = ast::Stmt>,
        tail: Option<ast::Expr>,
        option: ExprGenOption,
    ) -> EmittedExprInfo {
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
                                    option.non_last(),
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
                            let cs_name = self.alloc_binding_ast(b);
                            out.wln(format!("{} {} = (default!);", cs_type, cs_name));
                        }
                    }
                }
                ast::Stmt::ExprStmt(expr_stmt) => {
                    self.emit_expr_as_stmt_ast(out, expr_stmt.expr().unwrap(), option.non_last());
                }

                ast::Stmt::Item(ast::Item::Fn(fn_)) => {
                    let f = self.sem.to_def(&fn_).unwrap();
                    out.wln(format!("// inner fn: {}", f.name(self.db).as_str()));
                    self.add_deferred(f);
                }
                ast::Stmt::Item(ast::Item::Enum(adt)) => {
                    let f = self.sem.to_def(&adt).unwrap();
                    out.wln(format!("// inner enum: {}", f.name(self.db).as_str()));
                    self.add_deferred(f);
                }
                ast::Stmt::Item(ast::Item::Struct(adt)) => {
                    let f = self.sem.to_def(&adt).unwrap();
                    out.wln(format!("// inner enum: {}", f.name(self.db).as_str()));
                    self.add_deferred(f);
                }
                ast::Stmt::Item(ast::Item::Impl(impl_)) => {
                    let f = self.sem.to_def(&impl_).unwrap();
                    out.wln("// impl");
                    self.add_deferred(f);
                }
                ast::Stmt::Item(ast::Item::Const(c)) => {
                    let c = self.sem.to_def(&c).unwrap();
                    out.wln("// const");
                    self.add_deferred(c);
                }
                // TODO: impl
                // TODO: use, type alias: remove with comment?
                ast::Stmt::Item(item) => {
                    out.wln(format!("/* unsupported inner item: {:?} */", item));
                }
            }
        }

        if let Some(tail_expr) = tail {
            self.emit_expr_as_stmt_ast(out, tail_expr, option)
        } else {
            EmittedExprInfo::non_diverging()
        }
    }

    pub fn emit_expr_str_ast(&self, expr: &ast::Expr) -> Code {
        self.emit_expr_str_ast_inner(expr, false)
    }

    pub fn emit_expr_str_ast_inner(&self, expr: &ast::Expr, statement: bool) -> Code {
        match expr {
            //Expr::Missing => "/* missing */default!".into(),
            ast::Expr::Literal(lit) => self.emit_literal_ast(lit),
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
                    let expr_type = self.type_of_expr(expr).original;
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
                Some((hir::PathResolution::SelfType(impl_), _))
                    if let self_ty = impl_.self_ty(self.db)
                        && let Some(hir::Adt::Struct(struct_)) = self_ty.as_adt() =>
                {
                    let def = ConstructableDef::from(struct_);
                    let expr_type = self.type_of_expr(expr).original;
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
                Some((hir::PathResolution::SelfType(_), _)) => {
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
                    path.push_str(&names::static_name(static_.name(self.db).as_str()));
                    path.into()
                }
                Some((hir::PathResolution::TypeParam(_param), _)) => {
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
                let _name = field_expr.name_ref().unwrap();
                let receiver_part = field_expr.expr().unwrap();
                let receiver_type = self.type_of_expr(&receiver_part);
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
                        code!(receiver, adjuster, ".Item", field.index + 1)
                    }
                }
            }
            ast::Expr::MethodCallExpr(method_call) => {
                let receiver = self.emit_expr_str_ast(&method_call.receiver().unwrap());

                let db = self.db;

                match self.sem.resolve_method_call_fallback(method_call) {
                    Some((Either::Left(f), _))
                        if f.name(db).symbol().as_str() == "push"
                            && let hir::ItemContainer::Impl(impl_) = f.container(db)
                            && let Some(hir::Adt::Struct(struct_)) = impl_.self_ty(db).as_adt()
                            && let None = impl_.trait_(db)
                            && (Some(struct_) == self.lang_items.OsString()
                                || Some(struct_) == self.lang_items.String())
                            && let ast::Expr::PathExpr(receiver_path) =
                                method_call.receiver().unwrap()
                            && let Some(hir::PathResolution::Local(receiver_var)) =
                                self.sem.resolve_path(&receiver_path.path().unwrap())
                            && let Some(hir::Adt::Struct(struct_of_reciver_var)) =
                                receiver_var.ty(db).as_adt()
                            && struct_of_reciver_var == struct_
                            && statement =>
                    {
                        let mut code: Code = (self.binding_name_ast(receiver_var)).into();
                        code.w(" += ");
                        code.w(self.emit_expr_str_ast(
                            &method_call.arg_list().unwrap().args().next().unwrap(),
                        ));
                        code
                    }
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
                                    .unwrap()
                                    .args()
                                    .map(|arg| self.emit_expr_str_ast(&arg)),
                            ),
                        )
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
                            let expr_type = self.type_of_expr(expr).original;
                            let generic_args = expr_type.expect_adt_of(def.adt(self.db));
                            let callee_type =
                                self.constructable_name_cs(&Constructable::new(def, generic_args));
                            let args_str = args.map(|a| self.emit_expr_str_ast(&a));
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
                                args.map(|a| self.emit_expr_str_ast(&a)),
                            );
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
                        use ast::{CmpOp, Ordering};
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
                    UnaryOp::Deref => code!("(/* deref_op */", inner_str, ")"),
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
            ast::Expr::BlockExpr(block_expr) => match block_expr.modifier() {
                None => {
                    if block_expr.statements().count() == 0 {
                        if let Some(t) = block_expr.tail_expr() {
                            return self.emit_expr_str_ast(&t);
                        }
                        return "default!".into();
                    }
                    "/* block expr */ default!".into()
                }
                Some(ast::BlockModifier::Async(_)) => {
                    let mut body_code = Code::new();
                    let return_type = (self.type_of_expr(&expr))
                        .adjusted()
                        .future_output(self.db)
                        .unwrap();

                    let return_type_cs = self.rust_type_to_cs(&return_type);
                    {
                        let _scope = self.new_ctx(CodeContext {
                            is_async: true,
                            returning: return_type,
                        });
                        self.emit_returning_block(&mut body_code, &block_expr);
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
                        ast::BlockModifier::Async(_) => "async",
                        ast::BlockModifier::Unsafe(_) => "unsafe",
                        ast::BlockModifier::Try { .. } => "try",
                        ast::BlockModifier::Const(_) => "const",
                        ast::BlockModifier::AsyncGen(_) => "async gen",
                        ast::BlockModifier::Gen(_) => "gen",
                        ast::BlockModifier::Label(_) => "label",
                    },
                    self.expr_location_ast(expr)
                ),
            },
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
                        let ty_args =
                            (self.type_of_expr(expr).original).expect_adt_of(def.adt(self.db));

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
                    .filter(|f| self.check_cfg(f))
                    .map(|f| {
                        let cs_f = names::field_name(f.field_name().unwrap().text().as_str());
                        let field = c.fields(self.db).into_iter().find(|x| {
                            x.name(self.db).as_str() == f.field_name().unwrap().text().as_str()
                        });
                        let _cs_ty = field
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
                code!(base_str, ".Index(", idx_str, ")")
            }
            ast::Expr::RangeExpr(range_expr) => {
                fn single_generics<'db>(
                    sem: &Semantics<'db, dyn HirDatabase>,
                    expr: &ast::Expr,
                ) -> Type<'db> {
                    let [type_] = <[_; 1]>::try_from(
                        sem.type_of_expr(expr)
                            .unwrap()
                            .original
                            .as_adt_with_args()
                            .unwrap()
                            .1,
                    )
                    .unwrap();
                    type_.unwrap()
                }

                match (
                    range_expr.start(),
                    range_expr.op_kind().unwrap(),
                    range_expr.end(),
                ) {
                    (Some(start), RangeOp::Exclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.sem, expr)),
                            ">.NewRange(",
                            self.emit_expr_str_ast(&start),
                            ", ",
                            self.emit_expr_str_ast(&end),
                            ")"
                        )
                    }
                    (Some(start), RangeOp::Inclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.sem, expr)),
                            ">.NewRangeInclusive(",
                            self.emit_expr_str_ast(&start),
                            ", ",
                            self.emit_expr_str_ast(&end),
                            ")"
                        )
                    }
                    (Some(start), _, None) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.sem, expr)),
                            ">.NewRangeFrom(",
                            self.emit_expr_str_ast(&start),
                            ")"
                        )
                    }
                    (None, _, None) => {
                        code!("RangeFull.NewRangeFull()")
                    }
                    (None, RangeOp::Exclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.sem, expr)),
                            ">.NewRangeTo(",
                            self.emit_expr_str_ast(&end),
                            ")"
                        )
                    }
                    (None, RangeOp::Inclusive, Some(end)) => {
                        code!(
                            "Range<",
                            self.rust_type_to_cs(&single_generics(&self.sem, expr)),
                            ">.NewRangeToInclusive(",
                            self.emit_expr_str_ast(&end),
                            ")"
                        )
                    }
                }
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
                    let Some((element, _len)) = self.type_of_expr(expr).original.as_array(self.db)
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
                            self.alloc_binding_ast(&local)
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

                let type_of_expr = self.type_of_expr(expr).adjusted();
                assert!(type_of_expr.impls_fnonce(self.db));
                let output = type_of_expr
                    .normalize_trait_assoc_type(
                        self.db,
                        &[],
                        self.lang_items.FnOnceOutput().unwrap(),
                    )
                    .expect("No output for fn");
                let output = output.resolve_associated_type(self.db);
                let _scope = self.new_ctx(CodeContext {
                    is_async: false, // TODO
                    returning: output,
                });

                if let ast::Expr::BlockExpr(block) = closure.body().unwrap() {
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

                // Since we lower Cow<T> => T, we remove
                if let Some(cow) = self.sem.type_of_expr(&match_expr).and_then(|t| {
                    if let Some((adt, _args)) = t.adjusted.unwrap_or(t.original).as_adt_with_args()
                        && let hir::Adt::Enum(enum_) = adt
                        && Some(enum_) == self.lang_items.Cow()
                        && let Some(arms) = arms
                            .arms()
                            .map(|arm| {
                                if let ast::Pat::TupleStructPat(pat) = arm.pat().unwrap()
                                    && let Some(PathResolution::Def(def)) =
                                        self.sem.resolve_path(&pat.path().unwrap())
                                    && let ModuleDef::EnumVariant(variant) = def
                                    && (Some(variant) == self.lang_items.CowBorrowed()
                                        || Some(variant) == self.lang_items.CowOwned())
                                {
                                    Some(if Some(variant) == self.lang_items.CowBorrowed() {
                                        Some((pat.fields(), arm))
                                    } else {
                                        None
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect::<Option<Vec<_>>>()
                    {
                        Some(arms)
                    } else {
                        None
                    }
                }) {
                    let cow = cow.into_iter().flatten().collect::<Vec<_>>();
                    assert!(cow.len() == 1);

                    let mut out = Code::new();

                    out.w("(").w(scrutinee).wln(") switch {");
                    out.indent();

                    for (pat, arm) in cow {
                        // Emit pattern check
                        let pat_cs = self.emit_pattern_ast(&{ pat }.next().unwrap());

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

                    return out;
                }

                let mut out = Code::new();

                out.w("(").w(scrutinee).wln(") switch {");
                out.indent();

                for arm in arms.arms() {
                    if !self.check_cfg(&arm) {
                        continue;
                    }
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
            ast::Expr::MacroExpr(m) => self.emit_expr_macro(expr, &m.macro_call().unwrap()).0,

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

    fn emit_returning_block(&self, out: &mut Code, block_expr: &ast::BlockExpr) {
        let statements = block_expr.statements();
        let tail = block_expr.tail_expr();
        let emit_info = self.emit_block_contents(out, statements, tail, ExprGenOption::returning());
        if !emit_info.diverging && !self.returning_type().is_unit() {
            tracing::error!(
                "Error emitting expr: non-void returning expr does not diverging at {}",
                self.expr_location_ast(block_expr)
            );
        }
        if self.returning_type().is_unit() && self.is_async() && !emit_info.diverging {
            out.wln("return default;");
        }
    }

    fn emit_literal_ast(&self, lit: &ast::Literal) -> Code {
        match lit.kind() {
            ast::LiteralKind::Bool(b) => b.to_string().into(),
            ast::LiteralKind::IntNumber(v) => v.value().unwrap().to_string().into(),
            ast::LiteralKind::FloatNumber(v) => v.value_string().into(),
            ast::LiteralKind::Char(c) => fcode!("'{}'", c.value().unwrap().escape_default()),
            ast::LiteralKind::String(s) => fcode!("\"{}\"", s.value().unwrap().escape_default(),),
            ast::LiteralKind::Byte(b) => b.value().unwrap().to_string().into(),
            ast::LiteralKind::ByteString(_) => "/* byte string */ new byte[] {}".into(),
            ast::LiteralKind::CString(_) => "/* cstring */ \"\"".into(),
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
    fn emit_pattern_ast(&self, pat: &ast::Pat) -> Code {
        match pat {
            ast::Pat::WildcardPat(_w) => "{} _".into(),
            ast::Pat::IdentPat(ident_pat)
                if let Some(const_ref) = self.sem.resolve_bind_pat_to_const(ident_pat) =>
            {
                match const_ref {
                    const_ref
                        if let Some(constructable) =
                            ConstructableDef::from_module_def(const_ref) =>
                    {
                        let ty_args = (self.sem.type_of_pat(pat).unwrap().original)
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
                let (cs_type_name, _field_count) = match self.sem.resolve_path(&path) {
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
                    let pattern_codes = fields.iter().map(|field| {
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
            ast::Pat::SlicePat(slice_pat) => {
                let components = slice_pat.components();
                assert!(components.slice.is_none());
                code!(
                    "[",
                    join(
                        components.prefix.iter().map(|p| self.emit_pattern_ast(p)),
                        ","
                    ),
                    "]"
                )
            }
            ast::Pat::TuplePat(tuple_pat) => {
                let type_ = self.sem.type_of_pat(pat).unwrap();
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

    fn resolve_tuple_like_struct(&self, field_count: usize, patterns: &[ast::Pat]) -> Vec<Code> {
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
            let pattern_codes = patterns.iter().map(|pat| self.emit_pattern_ast(pat));
            pattern_codes.collect::<Vec<_>>()
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
}
