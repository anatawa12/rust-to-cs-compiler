/// Generates C# expressions and statements from Rust HIR bodies.
use std::collections::HashMap;

use super::{CodeGenerator, names, output::Output};
use crate::codegen::ty::Constructable;
use hir::{AssocItem, Function, Local, Name, PathKind, db::HirDatabase};
use hir_def::expr_store::scope::ExprScopes;
use hir_def::expr_store::{BodySourceMap, HygieneId};
use hir_def::resolver::{
    HasResolver, ResolveValueResult, Resolver, TypeNs, ValueNs, resolver_for_scope,
};
use hir_def::{
    CallableDefId, DefWithBodyId, HasModule, ItemContainerId, VariantId,
    expr_store::Body,
    hir::{
        Array, BinaryOp, BindingId, Expr, ExprId, Literal, Pat, PatId, RangeOp, Statement, UnaryOp,
    },
};
use hir_ty::InferenceResult;
use hir_ty::next_solver::TyKind;

/// Generates C# for the body of a single function.
pub struct BodyGen<'db> {
    cg: &'db CodeGenerator<'db>,
    body: &'db Body,
    source_map: &'db BodySourceMap,
    scopes: &'db ExprScopes,
    def_id: DefWithBodyId,
    infer: &'db InferenceResult,
    /// Mapping from BindingId to the C# local name allocated for it.
    bindings: HashMap<BindingId, String>,
    /// Counter per original Rust name for uniqueness.
    name_counts: HashMap<String, usize>,
    /// Whether we're inside an async fn (controls .GetAwaiter()/.GetResult() vs await).
    is_async: bool,
}

impl<'db> std::ops::Deref for BodyGen<'db> {
    type Target = CodeGenerator<'db>;

    fn deref(&self) -> &Self::Target {
        self.cg
    }
}

impl<'db> BodyGen<'db> {
    pub fn new(
        cg: &'db CodeGenerator<'db>,
        def_id: DefWithBodyId,
        body: &'db Body,
        source_map: &'db BodySourceMap,
        is_async: bool,
    ) -> Self {
        let infer = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            InferenceResult::of(cg.db, def_id)
        })) {
            Ok(infer) => infer,
            Err(e) => {
                eprintln!(
                    "processing {def_id:?} {} {}",
                    match def_id {
                        DefWithBodyId::FunctionId(f) => {
                            let f = hir::Function::from(f);
                            format!(
                                "{} in {}",
                                f.name(cg.db).as_str(),
                                cg.module_class_cs(f.module(cg.db))
                            )
                        }
                        DefWithBodyId::StaticId(f) =>
                            hir::Static::from(f).name(cg.db).as_str().to_string(),
                        DefWithBodyId::ConstId(f) => hir::Const::from(f)
                            .name(cg.db)
                            .as_ref()
                            .map(|x| x.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        DefWithBodyId::VariantId(f) =>
                            hir::EnumVariant::from(f).name(cg.db).as_str().to_string(),
                    },
                    source_map
                        .expr_syntax(body.root_expr())
                        .map(|x| cg.location_with_file(x))
                        .unwrap_or_else(|_| "Unknown location".into())
                );
                std::panic::resume_unwind(e);
            }
        };
        Self {
            cg,
            body,
            source_map,
            scopes: ExprScopes::of(cg.db, def_id),
            def_id,
            infer,
            bindings: HashMap::new(),
            name_counts: HashMap::new(),
            is_async,
        }
    }

    pub fn expr_location(&self, expr_id: ExprId) -> String {
        self.source_map
            .expr_syntax(expr_id)
            .map(|x| self.location_with_file(x))
            .unwrap_or_else(|_| "Unknown location".into())
    }

    pub fn source(&self, expr_id: ExprId) -> &str {
        self.source_map
            .expr_syntax(expr_id)
            .map(|f| {
                let loc = self
                    .sem
                    .diagnostics_display_range(f.map(|x| x.syntax_node_ptr()));
                &self.db.file_text(loc.file_id).text(self.db)[loc.range]
            })
            .unwrap_or("Unknown location")
    }

    fn alloc_binding(&mut self, id: BindingId) -> String {
        let rust_name = self.body[id].name.as_str().to_string();
        let count = self.name_counts.entry(rust_name.clone()).or_insert(0);
        let cs_name = names::local_name(&rust_name, *count);
        *count += 1;
        self.bindings.insert(id, cs_name.clone());
        cs_name
    }

    fn binding_name(&self, id: BindingId) -> String {
        self.bindings
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("/* unbound {:?} */unknown", id))
    }

    fn binding_cs_type(&self, id: BindingId) -> String {
        let local = Local::from((self.def_id, id));
        let local_ty = local.ty(self.db);
        let cs = self.rust_type_to_cs(&local_ty);
        if cs == "void" {
            "object".to_string()
        } else {
            cs
        }
    }

    /// Emit the full function body block.
    pub fn emit_body(&mut self, out: &mut Output) {
        if let Some(self_id) = self.body.self_param {
            self.bindings.insert(self_id, "this".into());
        }
        for &x in &self.body.params {
            match self.body[x] {
                Pat::Bind { id, subpat } => {
                    self.alloc_binding(id);
                }
                Pat::Missing => {}
                Pat::Wild => {}
                Pat::Tuple { .. } => {}
                Pat::Or(_) => {}
                Pat::Record { .. } => {}
                Pat::Range { .. } => {}
                Pat::Slice { .. } => {}
                Pat::Path(_) => {}
                Pat::Lit(_) => {}
                Pat::TupleStruct { .. } => {}
                Pat::Ref { .. } => {}
                Pat::Box { .. } => {}
                Pat::ConstBlock(_) => {}
                Pat::Expr(_) => {}
            }
        }
        let root = self.body.root_expr();
        self.emit_expr_as_stmt(out, root, true);
    }

    /// Emit an expression as a statement (with semicolon if needed).
    fn emit_expr_as_stmt(&mut self, out: &mut Output, expr_id: ExprId, is_tail: bool) {
        let expr = &self.body[expr_id];
        match expr {
            Expr::Block {
                statements, tail, ..
            } => {
                self.emit_block_contents(out, statements, *tail, is_tail);
            }
            Expr::Return { expr: ret_expr } => {
                let val = ret_expr
                    .map(|e| self.emit_expr_str(e))
                    .unwrap_or_else(|| "0".to_string());
                out.writeln(&format!("return {};", val));
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.emit_expr_str(*condition);
                out.writeln(&format!("if ({}) {{", cond));
                out.indent();
                self.emit_expr_as_stmt(out, *then_branch, false);
                out.dedent();
                if let Some(else_e) = else_branch {
                    out.write("} else {");
                    out.writeln("");
                    out.indent();
                    self.emit_expr_as_stmt(out, *else_e, is_tail);
                    out.dedent();
                    out.writeln("}");
                } else {
                    out.writeln("}");
                }
            }
            Expr::Loop { body, label } => {
                let label_str = label
                    .map(|l| {
                        let lbl = &self.body[l];
                        format!("{}: ", names::camel(lbl.name.as_str()))
                    })
                    .unwrap_or_default();
                out.writeln(&format!("{}while (true) {{", label_str));
                out.indent();
                self.emit_expr_as_stmt(out, *body, false);
                out.dedent();
                out.writeln("}");
            }
            Expr::Match {
                expr: match_expr,
                arms,
            } => {
                let scrutinee = self.emit_expr_str(*match_expr);
                out.writeln(&format!(
                    "var __match_{:?} = {};",
                    match_expr.into_raw(),
                    scrutinee
                ));
                let tmp = format!("__match_{:?}", match_expr.into_raw());
                for (i, arm) in arms.iter().enumerate() {
                    let is_last = i == arms.len() - 1;
                    let kw = if i == 0 { "if" } else { "else if" };
                    // Emit pattern check
                    let pat_check = self.emit_pat_check(&tmp, arm.pat);
                    let guard_str = arm
                        .guard
                        .map(|g| format!(" && ({})", self.emit_expr_str(g)))
                        .unwrap_or_default();
                    if pat_check == "true" && guard_str.is_empty() {
                        out.writeln("{");
                    } else {
                        out.writeln(&format!("{} ({}{}) {{", kw, pat_check, guard_str));
                    }
                    out.indent();
                    // Bind pattern variables
                    self.emit_pat_bindings(out, &tmp, arm.pat);
                    // Emit arm body
                    self.emit_expr_as_stmt(out, arm.expr, is_tail);
                    out.dedent();
                    out.write("}");
                    if !is_last {
                        out.writeln("");
                    } else {
                        out.writeln("");
                    }
                }
            }
            Expr::Break {
                expr: break_expr,
                label,
            } => {
                let label_str = label
                    .map(|l| {
                        let lbl = &self.body[l];
                        format!(" {}", names::camel(lbl.name.as_str()))
                    })
                    .unwrap_or_default();
                if let &Some(e) = break_expr {
                    let val = self.emit_expr_str(e);
                    out.writeln(&format!("/* break {} */", val)); // TODO: break-with-value
                } else {
                    out.writeln(&format!("break{};", label_str));
                }
            }
            Expr::Continue { label } => {
                let label_str = label
                    .map(|l| {
                        let lbl = &self.body[l];
                        format!(" {}", names::camel(lbl.name.as_str()))
                    })
                    .unwrap_or_default();
                out.writeln(&format!("continue{};", label_str));
            }
            _ => {
                // Generic expression: emit as expression statement
                let s = self.emit_expr_str(expr_id);
                if !s.is_empty() && s != "()" {
                    out.writeln(&format!("{};", s));
                }
            }
        }
    }

    fn emit_block_contents(
        &mut self,
        out: &mut Output,
        statements: &[Statement],
        tail: Option<ExprId>,
        is_tail: bool,
    ) {
        for stmt in statements {
            match stmt {
                Statement::Let {
                    pat,
                    type_ref: _,
                    initializer,
                    else_branch,
                } => {
                    let bindings = self.collect_bindings_in_pat(*pat);

                    if let Some(init) = initializer {
                        let init_str = self.emit_expr_str(*init);
                        if bindings.len() == 1 {
                            let bid = bindings[0];
                            let cs_type = self.binding_cs_type(bid);
                            let cs_name = self.alloc_binding(bid);
                            out.writeln(&format!(
                                "var {} = new r2CsRuntime.Slot<{}>({});",
                                cs_name, cs_type, init_str
                            ));
                        } else if bindings.is_empty() {
                            out.writeln(&format!("{};", init_str));
                        } else {
                            let tmp = format!("__tmp_{:?}", pat.into_raw());
                            out.writeln(&format!("var {} = {};", tmp, init_str));
                            for b in &bindings {
                                let cs_type = self.binding_cs_type(*b);
                                let cs_name = self.alloc_binding(*b);
                                let rust_name = self.body[*b].name.as_str().to_string();
                                out.writeln(&format!(
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
                            let cs_type = self.binding_cs_type(*b);
                            let cs_name = self.alloc_binding(*b);
                            out.writeln(&format!(
                                "var {} = new r2CsRuntime.Slot<{}>(default!);",
                                cs_name, cs_type
                            ));
                        }
                    }

                    if let Some(_else_e) = else_branch {
                        out.writeln("// let-else not fully supported");
                    }
                }
                Statement::Expr { expr, has_semi: _ } => {
                    self.emit_expr_as_stmt(out, *expr, false);
                }
                Statement::Item(_) => {}
            }
        }

        if let Some(tail_expr) = tail {
            let tail_str = self.emit_expr_str(tail_expr);
            if is_tail {
                if tail_str != "()" && !tail_str.is_empty() {
                    out.writeln(&format!("return {};", tail_str));
                }
            } else if tail_str != "()" && !tail_str.is_empty() {
                out.writeln(&format!("{};", tail_str));
            }
        }
    }

    /// Emit an expression as a string (inline expression).
    pub fn emit_expr_str(&mut self, expr_id: ExprId) -> String {
        let expr = &self.body[expr_id].clone(); // clone to avoid borrow conflict
        match expr {
            Expr::Missing => "/* missing */default!".to_string(),
            Expr::Literal(lit) => self.emit_literal(lit),
            Expr::Path(path) if true => {
                let resolver =
                    resolver_for_scope(self.db, self.def_id, self.scopes.scope_for(expr_id));
                match resolver.resolve_path_in_value_ns_with_prefix_info(
                    self.db,
                    path,
                    self.body.expr_or_pat_path_hygiene(expr_id.into()),
                ) {
                    Some((ResolveValueResult::ValueNs(value_ns), _)) => match value_ns {
                        ValueNs::ImplSelf(_) => {
                            eprintln!("ImplSelf at {}", self.expr_location(expr_id));
                            "(ImplSelf)".into()
                        }
                        ValueNs::LocalBinding(binding) => self.binding_name(binding),
                        ValueNs::FunctionId(f) => self.fn_path_cs(hir::Function::from(f)),
                        ValueNs::ConstId(c) => self.const_path_cs(hir::Const::from(c)),
                        ValueNs::StaticId(s) => {
                            let c = hir::Static::from(s);
                            let mut path = self.module_class_cs(c.module(self.db));
                            path.push('.');
                            path.push_str(c.name(self.db).as_str());
                            path
                        }
                        ValueNs::StructId(s) => {
                            format!("new {}", self.adt_name_cs(hir::Struct::from(s).into()))
                        }
                        ValueNs::EnumVariantId(v) => {
                            format!("new {}", self.enum_variant_cs(hir::EnumVariant::from(v)))
                        }
                        ValueNs::GenericParam(_) => {
                            eprintln!("GenericParam at {}", self.expr_location(expr_id));
                            "(GenericParam)".into()
                        }
                    },
                    Some((ResolveValueResult::Partial(_, _), _))
                        if let TyKind::FnDef(f, _generic) =
                            self.infer.expr_ty(expr_id).inner().internee =>
                    {
                        match f.0 {
                            CallableDefId::FunctionId(f) => self.fn_path_cs(hir::Function::from(f)),
                            CallableDefId::StructId(s) => {
                                format!("new {}", self.adt_name_cs(hir::Struct::from(s).into()))
                            }
                            CallableDefId::EnumVariantId(v) => {
                                format!("new {}", self.enum_variant_cs(hir::EnumVariant::from(v)))
                            }
                        }
                    }
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
                                    AssocItem::Function(f) => self.fn_path_cs(f),
                                    AssocItem::Const(c) => self.const_path_cs(c),
                                    AssocItem::TypeAlias(a) => self.rust_type_to_cs(&a.ty(self.db)),
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
                }
            }
            Expr::Path(path) => match path {
                _ if let PathKind::Super(0) = path.kind()
                    && path.segments().is_empty() =>
                {
                    "this".to_string()
                }
                path => {
                    let segments: Vec<String> = path
                        .segments()
                        .iter()
                        .map(|s| s.name.as_str().to_string())
                        .collect();
                    if segments.len() == 1 {
                        let name = &segments[0];
                        if let Some(cs_name) = self.find_local_by_rust_name(name) {
                            return format!("{}.value", cs_name);
                        }
                        names::method_name(name)
                    } else if segments.is_empty() {
                        eprintln!("empty path: {path:?}");
                        "/* empty path */default!".to_string()
                    } else {
                        let last = segments.last().unwrap();
                        let rest = &segments[..segments.len() - 1];
                        format!("{}.{}", rest.join("."), names::method_name(last))
                    }
                }
            },
            Expr::Field {
                expr: field_expr,
                name,
            } => {
                let receiver = self.emit_expr_str(*field_expr);
                format!("{}.{}.value", receiver, names::field_name(name.as_str()))
            }
            Expr::MethodCall {
                receiver,
                method_name,
                args,
                ..
            } => {
                let recv_str = self.emit_expr_str(*receiver);
                let method_cs = names::method_name(method_name.as_str());
                let args_str: Vec<String> = args.iter().map(|&a| self.emit_expr_str(a)).collect();
                format!("{}.{}({})", recv_str, method_cs, args_str.join(", "))
            }
            Expr::Call { callee, args } => {
                // Detect tuple-struct / enum-variant constructor calls
                //if let Some(variant_id) = self.infer.variant_resolution_for_expr(*callee) {
                //    let args_clone: Vec<ExprId> = args.iter().copied().collect();
                //    return self.emit_constructor_call(variant_id, &args_clone);
                //}
                let callee_str = self.emit_expr_str(*callee);
                let args_str: Vec<String> = args.iter().map(|&a| self.emit_expr_str(a)).collect();
                format!("{}({})", callee_str, args_str.join(", "))
            }
            Expr::Await { expr: inner } => {
                let inner_str = self.emit_expr_str(*inner);
                if self.is_async {
                    format!("(await {})", inner_str)
                } else {
                    format!("{}.GetAwaiter().GetResult()", inner_str)
                }
            }
            Expr::BinaryOp { lhs, rhs, op } => {
                let lhs_str = self.emit_expr_str(*lhs);
                let rhs_str = self.emit_expr_str(*rhs);
                let op_str = match op {
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
                format!("({} {} {})", lhs_str, op_str, rhs_str)
            }
            Expr::Assignment { target, value } => {
                let target_str = self.emit_pat_as_lvalue(*target);
                let value_str = self.emit_expr_str(*value);
                format!("{} = {}", target_str, value_str)
            }
            Expr::UnaryOp { expr: inner, op } => {
                let inner_str = self.emit_expr_str(*inner);
                let op_str = match op {
                    UnaryOp::Deref => format!("(*{})", inner_str),
                    UnaryOp::Not => format!("!({})", inner_str),
                    UnaryOp::Neg => format!("-({})", inner_str),
                };
                op_str
            }
            Expr::Ref { expr: inner, .. } => self.emit_expr_str(*inner),
            Expr::Box { expr: inner } => self.emit_expr_str(*inner),
            Expr::Cast { expr: inner, .. } => {
                format!("(/* cast */) {}", self.emit_expr_str(*inner))
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if let Some(else_e) = else_branch {
                    let cond = self.emit_expr_str(*condition);
                    let then_s = self.emit_expr_str(*then_branch);
                    let else_s = self.emit_expr_str(*else_e);
                    format!("({} ? {} : {})", cond, then_s, else_s)
                } else {
                    let cond = self.emit_expr_str(*condition);
                    format!("/* if expr */ {}", cond)
                }
            }
            Expr::Block {
                statements, tail, ..
            } => {
                if statements.is_empty() {
                    if let &Some(t) = tail {
                        return self.emit_expr_str(t);
                    }
                    return "default!".to_string();
                }
                "/* block expr */ default!".to_string()
            }
            Expr::Tuple { exprs } => {
                if exprs.is_empty() {
                    "/* unit */0".to_string()
                } else {
                    let parts: Vec<String> = exprs.iter().map(|&e| self.emit_expr_str(e)).collect();
                    format!("({})", parts.join(", "))
                }
            }
            Expr::RecordLit { path, fields, .. } => {
                let c = if let Some(path) = path {
                    let resolver =
                        resolver_for_scope(self.db, self.def_id, self.scopes.scope_for(expr_id));
                    if let Some(path) = resolver.resolve_path_in_type_ns_fully(self.db, path) {
                        match path {
                            TypeNs::SelfType(t) => Constructable::Struct(
                                (hir::Impl::from(t).self_ty(self.db).as_adt())
                                    .unwrap()
                                    .as_struct()
                                    .unwrap(),
                            ),
                            TypeNs::AdtId(adt) => {
                                Constructable::Struct(hir::Adt::from(adt).as_struct().unwrap())
                            }
                            TypeNs::EnumVariantId(v) => {
                                Constructable::EnumVariant(hir::EnumVariant::from(v))
                            }
                            TypeNs::TypeAliasId(t) => Constructable::Struct(
                                hir::TypeAlias::from(t)
                                    .ty(self.db)
                                    .as_adt()
                                    .unwrap()
                                    .as_struct()
                                    .unwrap(),
                            ),
                            _ => {
                                eprintln!(
                                    "Bad type for struct construction path in {}",
                                    self.expr_location(expr_id)
                                );
                                return format!(
                                    "(_BadTypeConstruction/*{:?}*/)",
                                    self.source(expr_id)
                                );
                            }
                        }
                    } else {
                        eprintln!("Failed to resolve path in {}", self.expr_location(expr_id));

                        return format!("(Unresolved Construction/*{:?}*/)", self.source(expr_id));
                    }
                } else {
                    eprintln!(
                        "Struct construction without type in {}",
                        self.expr_location(expr_id)
                    );
                    return format!("(TypelessConstruction/*{:?}*/)", self.source(expr_id));
                };
                let type_name = self.constructable_name_cs(c);

                let field_inits: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let cs_f = names::field_name(f.name.as_str());
                        let field = c
                            .fields(self.db)
                            .into_iter()
                            .find(|x| x.name(self.db) == f.name);
                        let cs_ty = field
                            .map(|f| self.rust_type_to_cs(&f.ty(self.db).to_type(self.db)))
                            .unwrap_or_else(|| "object /*unknown field type*/".into());
                        let val = self.emit_expr_str(f.expr);
                        format!("{cs_f} = new r2CsRuntime.Slot<{cs_ty}>({val})")
                    })
                    .collect();
                format!("new {}() {{ {} }}", type_name, field_inits.join(", "))
            }
            Expr::Index { base, index } => {
                let base_str = self.emit_expr_str(*base);
                let idx_str = self.emit_expr_str(*index);
                format!("{}[{}]", base_str, idx_str)
            }
            Expr::Range {
                lhs,
                rhs,
                range_type,
            } => {
                let lhs_s = lhs.map(|e| self.emit_expr_str(e)).unwrap_or_default();
                let rhs_s = rhs.map(|e| self.emit_expr_str(e)).unwrap_or_default();
                let dots = match range_type {
                    RangeOp::Exclusive => "..",
                    RangeOp::Inclusive => "..=",
                };
                format!(
                    "/* range {}{}{} */ new s_Range({}, {})",
                    lhs_s, dots, rhs_s, lhs_s, rhs_s
                )
            }
            Expr::Array(arr) => match arr {
                Array::ElementList { elements } => {
                    let parts: Vec<String> =
                        elements.iter().map(|&e| self.emit_expr_str(e)).collect();
                    format!("new object[] {{ {} }}", parts.join(", "))
                }
                &Array::Repeat {
                    initializer,
                    repeat,
                } => {
                    let val = self.emit_expr_str(initializer);
                    let len = self.emit_expr_str(repeat);
                    format!("new object[{}] /* fill {} */", len, val)
                }
            },
            &Expr::Closure {
                ref args,
                body: closure_body,
                ..
            } => {
                let params: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let bindings = self.collect_bindings_in_pat(*p);
                        if bindings.len() == 1 {
                            let name = self.body[bindings[0]].name.as_str().to_string();
                            names::local_name(&name, 0)
                        } else {
                            format!("__cp{}", i)
                        }
                    })
                    .collect();
                for p in args.iter() {
                    let bindings = self.collect_bindings_in_pat(*p);
                    for b in bindings {
                        self.alloc_binding(b);
                    }
                }
                let body_str = self.emit_expr_str(closure_body);
                format!("({}) => {}", params.join(", "), body_str)
            }
            Expr::Yeet { expr: inner } => {
                let val = inner
                    .map(|e| self.emit_expr_str(e))
                    .unwrap_or_else(|| "default!".to_string());
                format!("throw r2CsRuntime.Helpers.Returns<object>({})", val)
            }
            Expr::Return { expr: inner } => {
                let val = inner
                    .map(|e| self.emit_expr_str(e))
                    .unwrap_or_else(|| "0 /* unit */".to_string());
                format!(
                    "/* return-expr */ throw r2CsRuntime.Helpers.Returns<object>({})",
                    val
                )
            }
            &Expr::Let {
                pat,
                expr: let_expr,
            } => {
                let val = self.emit_expr_str(let_expr);
                let check = self.emit_pat_check(&val, pat);
                check
            }
            Expr::Unsafe {
                statements, tail, ..
            } => {
                if statements.is_empty() {
                    if let &Some(t) = tail {
                        return self.emit_expr_str(t);
                    }
                    return "default!".to_string();
                }
                "/* unsafe block */ default!".to_string()
            }
            Expr::Match { .. } | Expr::Loop { .. } => "/* block-expr-value */ default!".to_string(),
            Expr::Break { expr: val, .. } => val
                .map(|e| self.emit_expr_str(e))
                .unwrap_or_else(|| "default!".to_string()),
            Expr::Continue { .. } => "default!".to_string(),
            &Expr::Become { expr: inner } => self.emit_expr_str(inner),
            &Expr::Yield { expr: inner } => inner
                .map(|e| self.emit_expr_str(e))
                .unwrap_or_else(|| "default!".to_string()),
            &Expr::Const(inner) => self.emit_expr_str(inner),
            Expr::Underscore => "_".to_string(),
            Expr::OffsetOf(_) | Expr::InlineAsm(_) => "/* asm/offsetof */ default!".to_string(),
        }
    }

    /// Emit a tuple-struct or enum-variant constructor call.
    fn emit_constructor_call(&mut self, variant_id: VariantId, args: &[ExprId]) -> String {
        match variant_id {
            VariantId::StructId(sid) => {
                let s = hir::Struct::from(sid);
                let cs_name = names::struct_name(s.name(self.db).as_str());
                let fields = s.fields(self.db);
                if fields.is_empty() || args.is_empty() {
                    return format!("new {}()", cs_name);
                }
                let inits = self.build_positional_field_inits(&fields, args);
                format!("new {}() {{ {} }}", cs_name, inits.join(", "))
            }
            VariantId::EnumVariantId(vid) => {
                let v = hir::EnumVariant::from(vid);
                let cs_name = names::variant_name(v.name(self.db).as_str());
                let fields = v.fields(self.db);
                if fields.is_empty() || args.is_empty() {
                    return format!("new {}()", cs_name);
                }
                let inits = self.build_positional_field_inits(&fields, args);
                format!("new {}() {{ {} }}", cs_name, inits.join(", "))
            }
            VariantId::UnionId(_) => "default! /* union ctor */".to_string(),
        }
    }

    fn build_positional_field_inits(
        &mut self,
        fields: &[hir::Field],
        args: &[ExprId],
    ) -> Vec<String> {
        fields
            .iter()
            .zip(args.iter())
            .map(|(field, &arg_id)| {
                let field_ty = field.ty(self.db).to_type(self.db);
                let cs_ty = {
                    let t = self.rust_type_to_cs(&field_ty);
                    if t == "void" { "object".to_string() } else { t }
                };
                let f_name = names::field_name(field.name(self.db).as_str());
                let val = self.emit_expr_str(arg_id);
                format!("{} = new r2CsRuntime.Slot<{}>({})", f_name, cs_ty, val)
            })
            .collect()
    }

    /// Look up the Slot<T> inner type for a named field from variant resolution.
    fn field_slot_type_from_variant(
        &self,
        maybe_variant: Option<VariantId>,
        field_name: &str,
    ) -> String {
        let Some(vid) = maybe_variant else {
            return "object".to_string();
        };
        let variant_fields: Vec<hir::Field> = match vid {
            VariantId::StructId(sid) => hir::Struct::from(sid).fields(self.db),
            VariantId::EnumVariantId(evid) => hir::EnumVariant::from(evid).fields(self.db),
            VariantId::UnionId(_) => return "object".to_string(),
        };
        variant_fields
            .iter()
            .find(|f| f.name(self.db).as_str() == field_name)
            .map(|f| {
                let t = self.rust_type_to_cs(&f.ty(self.db).to_type(self.db));
                if t == "void" { "object".to_string() } else { t }
            })
            .unwrap_or_else(|| "object".to_string())
    }

    fn emit_literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Bool(b) => b.to_string(),
            Literal::Int(v, _) => v.to_string(),
            Literal::Uint(v, _) => v.to_string(),
            Literal::Float(f, _) => f.to_string(),
            Literal::Char(c) => format!("'{}'", c.escape_default()),
            Literal::String(s) => format!(
                "\"{}\"",
                s.as_str().replace('\\', "\\\\").replace('"', "\\\"")
            ),
            Literal::ByteString(_) => "/* byte string */ new byte[] {}".to_string(),
            Literal::CString(_) => "/* cstring */ \"\"".to_string(),
        }
    }

    /// Emit a pattern as a condition check against a scrutinee expression.
    fn emit_pat_check(&mut self, scrutinee: &str, pat_id: PatId) -> String {
        let pat = &self.body[pat_id];
        match pat {
            Pat::Wild | Pat::Missing => "true".to_string(),
            Pat::Bind { subpat, .. } => {
                if let Some(sub) = subpat {
                    self.emit_pat_check(scrutinee, *sub)
                } else {
                    "true".to_string()
                }
            }
            Pat::TupleStruct { path, args, .. } => {
                if let Some(p) = path {
                    let segs: Vec<String> = p
                        .segments()
                        .iter()
                        .map(|s| s.name.as_str().to_string())
                        .collect();
                    let variant_name = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                    let cs_variant = names::variant_name(&variant_name);
                    format!("{} is {}", scrutinee, cs_variant)
                } else {
                    "true".to_string()
                }
            }
            Pat::Path(p) => {
                let segs: Vec<String> = p
                    .segments()
                    .iter()
                    .map(|s| s.name.as_str().to_string())
                    .collect();
                if segs.len() > 1 {
                    let variant = segs.last().unwrap();
                    let cs_variant = names::variant_name(variant);
                    format!("{} is {}", scrutinee, cs_variant)
                } else {
                    "true".to_string()
                }
            }
            Pat::Record { path, args, .. } => {
                if let Some(p) = path {
                    let segs: Vec<String> = p
                        .segments()
                        .iter()
                        .map(|s| s.name.as_str().to_string())
                        .collect();
                    let variant = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                    let cs_variant = names::variant_name(&variant);
                    format!("{} is {}", scrutinee, cs_variant)
                } else {
                    "true".to_string()
                }
            }
            Pat::Lit(expr_id) => {
                let lit_str = self.emit_expr_str(*expr_id);
                format!("{} == {}", scrutinee, lit_str)
            }
            Pat::Or(pats) => {
                let parts: Vec<String> = pats
                    .iter()
                    .map(|p| self.emit_pat_check(scrutinee, *p))
                    .collect();
                format!("({})", parts.join(" || "))
            }
            Pat::Tuple { args, .. } => {
                let checks: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let sub_scrutinee = format!("{}.Item{}", scrutinee, i + 1);
                        self.emit_pat_check(&sub_scrutinee, *p)
                    })
                    .collect();
                let combined: Vec<String> = checks.into_iter().filter(|s| s != "true").collect();
                if combined.is_empty() {
                    "true".to_string()
                } else {
                    format!("({})", combined.join(" && "))
                }
            }
            _ => "true".to_string(),
        }
    }

    /// Emit variable binding statements for a pattern matched against a scrutinee.
    fn emit_pat_bindings(&mut self, out: &mut Output, scrutinee: &str, pat_id: PatId) {
        let pat = self.body[pat_id].clone();
        match pat {
            Pat::Bind { id, subpat } => {
                let cs_type = self.binding_cs_type(id);
                let cs_name = self.alloc_binding(id);
                out.writeln(&format!(
                    "var {} = new r2CsRuntime.Slot<{}>({});",
                    cs_name, cs_type, scrutinee
                ));
                if let Some(sub) = subpat {
                    self.emit_pat_bindings(out, scrutinee, sub);
                }
            }
            Pat::TupleStruct { path, args, .. } => {
                if let Some(p) = &path {
                    let segs: Vec<String> = p
                        .segments()
                        .iter()
                        .map(|s| s.name.as_str().to_string())
                        .collect();
                    let variant = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                    let cs_variant = names::variant_name(&variant);
                    let tmp = format!("__ts_{}", cs_variant);
                    out.writeln(&format!("var {} = ({}) {};", tmp, cs_variant, scrutinee));
                    for (i, sub_pat) in args.iter().enumerate() {
                        let sub_scrutinee = format!("{}.f_{}.value", tmp, i);
                        self.emit_pat_bindings(out, &sub_scrutinee, *sub_pat);
                    }
                }
            }
            Pat::Record { path, args, .. } => {
                if let Some(p) = &path {
                    let segs: Vec<String> = p
                        .segments()
                        .iter()
                        .map(|s| s.name.as_str().to_string())
                        .collect();
                    let variant = segs.last().unwrap_or(&"Unknown".to_string()).clone();
                    let cs_variant = names::variant_name(&variant);
                    let tmp = format!("__rc_{}", cs_variant);
                    out.writeln(&format!("var {} = ({}) {};", tmp, cs_variant, scrutinee));
                    for field_pat in args.iter() {
                        let field_name = names::field_name(field_pat.name.as_str());
                        let sub_scrutinee = format!("{}.{}.value", tmp, field_name);
                        self.emit_pat_bindings(out, &sub_scrutinee, field_pat.pat);
                    }
                }
            }
            Pat::Tuple { args, .. } => {
                for (i, sub_pat) in args.iter().enumerate() {
                    let sub_scrutinee = format!("{}.Item{}", scrutinee, i + 1);
                    self.emit_pat_bindings(out, &sub_scrutinee, *sub_pat);
                }
            }
            _ => {}
        }
    }

    /// Emit a pattern as an lvalue (for assignment).
    fn emit_pat_as_lvalue(&self, pat_id: PatId) -> String {
        let pat = &self.body[pat_id];
        match pat {
            Pat::Bind { id, .. } => {
                let cs_name = self
                    .bindings
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "/* unbound */unknown".to_string());
                format!("{}.value", cs_name)
            }
            Pat::Wild => "_".to_string(),
            _ => "/* complex lvalue */unknown".to_string(),
        }
    }

    /// Collect all binding IDs in a pattern.
    fn collect_bindings_in_pat(&self, pat_id: PatId) -> Vec<BindingId> {
        let mut result = Vec::new();
        self.collect_bindings_recursive(pat_id, &mut result);
        result
    }

    fn collect_bindings_recursive(&self, pat_id: PatId, result: &mut Vec<BindingId>) {
        let pat = &self.body[pat_id];
        match pat {
            Pat::Bind { id, subpat } => {
                result.push(*id);
                if let Some(sub) = subpat {
                    self.collect_bindings_recursive(*sub, result);
                }
            }
            Pat::TupleStruct { args, .. } | Pat::Tuple { args, .. } => {
                for a in args.iter() {
                    self.collect_bindings_recursive(*a, result);
                }
            }
            Pat::Record { args, .. } => {
                for f in args.iter() {
                    self.collect_bindings_recursive(f.pat, result);
                }
            }
            Pat::Or(pats) => {
                if let Some(first) = pats.first() {
                    self.collect_bindings_recursive(*first, result);
                }
            }
            Pat::Slice {
                prefix,
                slice,
                suffix,
            } => {
                for p in prefix.iter().chain(slice.iter()).chain(suffix.iter()) {
                    self.collect_bindings_recursive(*p, result);
                }
            }
            Pat::Ref { pat, .. } | Pat::Box { inner: pat } => {
                self.collect_bindings_recursive(*pat, result);
            }
            _ => {}
        }
    }

    fn find_local_by_rust_name(&self, rust_name: &str) -> Option<String> {
        for (id, cs_name) in &self.bindings {
            let binding = &self.body[*id];
            if binding.name.as_str() == rust_name {
                return Some(cs_name.clone());
            }
        }
        None
    }
}
