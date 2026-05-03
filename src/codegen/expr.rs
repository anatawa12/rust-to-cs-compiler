/// Generates C# expressions and statements from Rust HIR bodies.
use std::collections::HashMap;

use hir::{DefWithBody, GenericDef, HasName, db::HirDatabase};
use hir_def::{
    DefWithBodyId,
    expr_store::Body,
    hir::{
        Array, BinaryOp, BindingId, Expr, ExprId, Literal, Pat, PatId, RangeOp, Statement,
        UnaryOp,
    },
};

use super::{names, output::Output, ty};

/// Generates C# for the body of a single function.
pub struct BodyGen<'db> {
    db: &'db dyn HirDatabase,
    body: &'db Body,
    /// Mapping from BindingId to the C# local name allocated for it.
    bindings: HashMap<BindingId, String>,
    /// Counter per original Rust name for uniqueness.
    name_counts: HashMap<String, usize>,
    /// Whether we're inside an async fn (controls .GetAwaiter()/.GetResult() vs await).
    is_async: bool,
}

impl<'db> BodyGen<'db> {
    pub fn new(db: &'db dyn HirDatabase, body: &'db Body, is_async: bool) -> Self {
        Self {
            db,
            body,
            bindings: HashMap::new(),
            name_counts: HashMap::new(),
            is_async,
        }
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

    /// Emit the full function body block.
    pub fn emit_body(&mut self, out: &mut Output) {
        let root = self.body.root_expr();
        self.emit_expr_as_stmt(out, root, true);
    }

    /// Emit an expression as a statement (with semicolon if needed).
    fn emit_expr_as_stmt(&mut self, out: &mut Output, expr_id: ExprId, is_tail: bool) {
        let expr = &self.body[expr_id];
        match expr {
            Expr::Block { statements, tail, .. } => {
                self.emit_block_contents(out, statements, *tail, is_tail);
            }
            Expr::Return { expr: ret_expr } => {
                let val = ret_expr
                    .map(|e| self.emit_expr_str(e))
                    .unwrap_or_else(|| "0".to_string());
                out.writeln(&format!("return {};", val));
            }
            Expr::If { condition, then_branch, else_branch } => {
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
            Expr::Match { expr: match_expr, arms } => {
                let scrutinee = self.emit_expr_str(*match_expr);
                out.writeln(&format!("var __match_{:?} = {};", match_expr.into_raw(), scrutinee));
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
            Expr::Break { expr: break_expr, label } => {
                let label_str = label
                    .map(|l| {
                        let lbl = &self.body[l];
                        format!(" {}", names::camel(lbl.name.as_str()))
                    })
                    .unwrap_or_default();
                if let Some(e) = break_expr {
                    let val = self.emit_expr_str(*e);
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
        // Collect locals introduced in this block for drop tracking
        let mut drop_locals: Vec<String> = Vec::new();

        for stmt in statements {
            match stmt {
                Statement::Let { pat, type_ref: _, initializer, else_branch } => {
                    // Allocate bindings for pattern
                    let bindings = self.collect_bindings_in_pat(*pat);

                    if let Some(init) = initializer {
                        let init_str = self.emit_expr_str(*init);
                        // Simple case: single binding
                        if bindings.len() == 1 {
                            let cs_name = self.alloc_binding(bindings[0]);
                            drop_locals.push(cs_name.clone());
                            out.writeln(&format!("var {} = new r2CsRuntime.Slot<object>({});", cs_name, init_str));
                        } else if bindings.is_empty() {
                            // Wildcard or unit pattern
                            out.writeln(&format!("{};", init_str));
                        } else {
                            // Destructuring: use temp and then bind
                            let tmp = format!("__tmp_{:?}", pat.into_raw());
                            out.writeln(&format!("var {} = {};", tmp, init_str));
                            for b in &bindings {
                                let cs_name = self.alloc_binding(*b);
                                drop_locals.push(cs_name.clone());
                                let rust_name = self.body[*b].name.as_str().to_string();
                                out.writeln(&format!(
                                    "var {} = new r2CsRuntime.Slot<object>({}.{});",
                                    cs_name, tmp,
                                    names::field_name(&rust_name)
                                ));
                            }
                        }
                    } else {
                        // No initializer: declare with default
                        for b in &bindings {
                            let cs_name = self.alloc_binding(*b);
                            drop_locals.push(cs_name.clone());
                            out.writeln(&format!("var {} = new r2CsRuntime.Slot<object>(default!);", cs_name));
                        }
                    }

                    if let Some(else_e) = else_branch {
                        out.writeln("// let-else not fully supported");
                        // TODO: proper let-else generation
                    }
                }
                Statement::Expr { expr, has_semi } => {
                    self.emit_expr_as_stmt(out, *expr, false);
                }
                Statement::Item(_) => {
                    // inner items — skip for now
                }
            }
        }

        // Tail expression (the value of the block)
        if let Some(tail_expr) = tail {
            // Drop locals before returning the tail value
            // For now, just emit the expression
            let tail_str = self.emit_expr_str(tail_expr);
            if is_tail {
                if tail_str != "()" && !tail_str.is_empty() {
                    out.writeln(&format!("return {};", tail_str));
                }
            } else {
                if tail_str != "()" && !tail_str.is_empty() {
                    out.writeln(&format!("{};", tail_str));
                }
            }
        }
    }

    /// Emit an expression as a string (inline expression).
    pub fn emit_expr_str(&mut self, expr_id: ExprId) -> String {
        let expr = &self.body[expr_id].clone(); // clone to avoid borrow conflict
        match expr {
            Expr::Missing => "/* missing */default!".to_string(),
            Expr::Literal(lit) => self.emit_literal(lit),
            Expr::Path(path) => {
                // Try to resolve the path to a binding or call
                let segments: Vec<String> = path
                    .segments()
                    .iter()
                    .map(|s| s.name.as_str().to_string())
                    .collect();
                if segments.len() == 1 {
                    let name = &segments[0];
                    // Check if it's a local variable (binding) - search by Rust name
                    if let Some(cs_name) = self.find_local_by_rust_name(name) {
                        return format!("{}.value", cs_name);
                    }
                    // Otherwise treat as function/type name
                    names::method_name(name)
                } else if segments.is_empty() {
                    "/* empty path */default!".to_string()
                } else {
                    // Multi-segment path: module::function or Type::method
                    let last = segments.last().unwrap();
                    let rest = &segments[..segments.len() - 1];
                    format!("{}.{}", rest.join("."), names::method_name(last))
                }
            }
            Expr::Field { expr: field_expr, name } => {
                let receiver = self.emit_expr_str(*field_expr);
                format!("{}.{}.value", receiver, names::field_name(name.as_str()))
            }
            Expr::MethodCall { receiver, method_name, args, .. } => {
                let recv_str = self.emit_expr_str(*receiver);
                let method_cs = names::method_name(method_name.as_str());
                let args_str: Vec<String> = args.iter().map(|a| self.emit_expr_str(*a)).collect();
                format!("{}.{}({})", recv_str, method_cs, args_str.join(", "))
            }
            Expr::Call { callee, args } => {
                let callee_str = self.emit_expr_str(*callee);
                let args_str: Vec<String> = args.iter().map(|a| self.emit_expr_str(*a)).collect();
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
                    Some(BinaryOp::ArithOp(a)) => format!("{:?}", a).to_lowercase()
                        .replace("add", "+").replace("sub", "-")
                        .replace("mul", "*").replace("div", "/").replace("rem", "%")
                        .replace("shl", "<<").replace("shr", ">>")
                        .replace("bitxor", "^").replace("bitor", "|").replace("bitand", "&"),
                    Some(BinaryOp::CmpOp(c)) => {
                        use hir_def::hir::{CmpOp, Ordering};
                        match c {
                            CmpOp::Eq { negated: false } => "==".to_string(),
                            CmpOp::Eq { negated: true } => "!=".to_string(),
                            CmpOp::Ord { ordering: Ordering::Less, strict: true } => "<".to_string(),
                            CmpOp::Ord { ordering: Ordering::Less, strict: false } => "<=".to_string(),
                            CmpOp::Ord { ordering: Ordering::Greater, strict: true } => ">".to_string(),
                            CmpOp::Ord { ordering: Ordering::Greater, strict: false } => ">=".to_string(),
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
                    Some(BinaryOp::Assignment { op: Some(a) }) => format!("{:?}=", a).to_lowercase()
                        .replace("add", "+=").replace("sub", "-=")
                        .replace("mul", "*=").replace("div", "/=").replace("rem", "%="),
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
            Expr::Ref { expr: inner, .. } => {
                // References in C# are just the value
                self.emit_expr_str(*inner)
            }
            Expr::Box { expr: inner } => {
                // Box::new(x) → just x (reference semantics)
                self.emit_expr_str(*inner)
            }
            Expr::Cast { expr: inner, .. } => {
                // TODO: proper cast type
                format!("(/* cast */) {}", self.emit_expr_str(*inner))
            }
            Expr::If { condition, then_branch, else_branch } => {
                if let Some(else_e) = else_branch {
                    // Ternary if possible
                    let cond = self.emit_expr_str(*condition);
                    let then_s = self.emit_expr_str(*then_branch);
                    let else_s = self.emit_expr_str(*else_e);
                    format!("({} ? {} : {})", cond, then_s, else_s)
                } else {
                    // if without else as expression: not valid in C# outside a block
                    let cond = self.emit_expr_str(*condition);
                    let then_s = self.emit_expr_str(*then_branch);
                    format!("/* if expr */ {}", cond)
                }
            }
            Expr::Block { statements, tail, .. } => {
                // Blocks as expressions: use an IIFE via Tr() or inline
                if statements.is_empty() {
                    if let Some(t) = tail {
                        return self.emit_expr_str(*t);
                    }
                    return "default!".to_string();
                }
                // For blocks with statements, we need a lambda
                "/* block expr */ default!".to_string()
            }
            Expr::Tuple { exprs } => {
                if exprs.is_empty() {
                    "/* unit */0".to_string()
                } else {
                    let parts: Vec<String> = exprs.iter().map(|e| self.emit_expr_str(*e)).collect();
                    format!("({})", parts.join(", "))
                }
            }
            Expr::RecordLit { path, fields, .. } => {
                let type_name = path
                    .as_ref()
                    .map(|p| {
                        let segs: Vec<String> = p.segments().iter()
                            .map(|s| s.name.as_str().to_string())
                            .collect();
                        names::struct_name(segs.last().unwrap_or(&"Unknown".to_string()))
                    })
                    .unwrap_or_else(|| "/* anon */Unknown".to_string());
                let field_inits: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let cs_f = names::field_name(f.name.as_str());
                        let val = self.emit_expr_str(f.expr);
                        format!("{} = new r2CsRuntime.Slot<object>({})", cs_f, val)
                    })
                    .collect();
                format!("new {}() {{ {} }}", type_name, field_inits.join(", "))
            }
            Expr::Index { base, index } => {
                let base_str = self.emit_expr_str(*base);
                let idx_str = self.emit_expr_str(*index);
                format!("{}[{}]", base_str, idx_str)
            }
            Expr::Range { lhs, rhs, range_type } => {
                // Emit a range comment — range types need std mapping
                let lhs_s = lhs.map(|e| self.emit_expr_str(e)).unwrap_or_default();
                let rhs_s = rhs.map(|e| self.emit_expr_str(e)).unwrap_or_default();
                let dots = match range_type {
                    RangeOp::Exclusive => "..",
                    RangeOp::Inclusive => "..=",
                };
                format!("/* range {}{}{} */ new s_Range({}, {})", lhs_s, dots, rhs_s, lhs_s, rhs_s)
            }
            Expr::Array(arr) => {
                match arr {
                    Array::ElementList { elements } => {
                        let parts: Vec<String> = elements.iter().map(|e| self.emit_expr_str(*e)).collect();
                        format!("new object[] {{ {} }}", parts.join(", "))
                    }
                    Array::Repeat { initializer, repeat } => {
                        let val = self.emit_expr_str(*initializer);
                        let len = self.emit_expr_str(*repeat);
                        format!("new object[{}] /* fill {} */", len, val)
                    }
                }
            }
            Expr::Closure { args, body: closure_body, .. } => {
                // Emit as lambda
                let params: Vec<String> = args.iter().enumerate()
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
                // Allocate bindings for closure params
                for p in args.iter() {
                    let bindings = self.collect_bindings_in_pat(*p);
                    for b in bindings {
                        self.alloc_binding(b);
                    }
                }
                let body_str = self.emit_expr_str(*closure_body);
                format!("({}) => {}", params.join(", "), body_str)
            }
            Expr::Yeet { expr: inner } => {
                // ? operator's internal representation — propagate error
                let val = inner.map(|e| self.emit_expr_str(e)).unwrap_or_else(|| "default!".to_string());
                format!("throw r2CsRuntime.Helpers.Returns<object>({})", val)
            }
            Expr::Return { expr: inner } => {
                let val = inner.map(|e| self.emit_expr_str(e)).unwrap_or_else(|| "0 /* unit */".to_string());
                format!("/* return-expr */ throw r2CsRuntime.Helpers.Returns<object>({})", val)
            }
            Expr::Let { pat, expr: let_expr } => {
                // let pattern = expr used as condition (if let)
                let val = self.emit_expr_str(*let_expr);
                let check = self.emit_pat_check(&val, *pat);
                check
            }
            Expr::Unsafe { statements, tail, .. } => {
                // Treat unsafe blocks like regular blocks
                if statements.is_empty() {
                    if let Some(t) = tail {
                        return self.emit_expr_str(*t);
                    }
                    return "default!".to_string();
                }
                "/* unsafe block */ default!".to_string()
            }
            Expr::Match { .. } | Expr::Loop { .. } => {
                // Block-like expressions used as values: wrap as lambda
                "/* block-expr-value */ default!".to_string()
            }
            Expr::Break { expr: val, .. } => {
                val.map(|e| self.emit_expr_str(e)).unwrap_or_else(|| "default!".to_string())
            }
            Expr::Continue { .. } => "default!".to_string(),
            Expr::Become { expr: inner } => {
                // tail-call → just call
                self.emit_expr_str(*inner)
            }
            Expr::Yield { expr: inner } => {
                inner.map(|e| self.emit_expr_str(e)).unwrap_or_else(|| "default!".to_string())
            }
            Expr::Const(inner) => self.emit_expr_str(*inner),
            Expr::Underscore => "_".to_string(),
            Expr::OffsetOf(_) | Expr::InlineAsm(_) => {
                "/* asm/offsetof */ default!".to_string()
            }
            _ => "/* unhandled */ default!".to_string()
        }
    }

    fn emit_literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Bool(b) => b.to_string(),
            Literal::Int(v, _) => v.to_string(),
            Literal::Uint(v, _) => v.to_string(),
            Literal::Float(f, _) => f.to_string(),
            Literal::Char(c) => format!("'{}'", c.escape_default()),
            Literal::String(s) => format!("\"{}\"", s.as_str().replace('\\', "\\\\").replace('"', "\\\"")),
            Literal::ByteString(_) => "/* byte string */ new byte[] {}".to_string(),
            Literal::CString(_) => "/* cstring */ \"\"".to_string(),
        }
    }

    /// Emit a pattern as a condition check against a scrutinee expression.
    fn emit_pat_check(&mut self, scrutinee: &str, pat_id: PatId) -> String {
        let pat = &self.body[pat_id];
        match pat {
            Pat::Wild | Pat::Missing => "true".to_string(),
            Pat::Bind { id, subpat } => {
                if let Some(sub) = subpat {
                    self.emit_pat_check(scrutinee, *sub)
                } else {
                    "true".to_string() // plain binding always matches
                }
            }
            Pat::TupleStruct { path, args, .. } => {
                if let Some(p) = path {
                    let segs: Vec<String> = p.segments().iter()
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
                let segs: Vec<String> = p.segments().iter()
                    .map(|s| s.name.as_str().to_string())
                    .collect();
                if segs.len() > 1 {
                    let variant = segs.last().unwrap();
                    let cs_variant = names::variant_name(variant);
                    format!("{} is {}", scrutinee, cs_variant)
                } else {
                    // Could be a const or a binding
                    "true".to_string()
                }
            }
            Pat::Record { path, args, .. } => {
                if let Some(p) = path {
                    let segs: Vec<String> = p.segments().iter()
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
                let parts: Vec<String> = pats.iter()
                    .map(|p| self.emit_pat_check(scrutinee, *p))
                    .collect();
                format!("({})", parts.join(" || "))
            }
            Pat::Tuple { args, .. } => {
                // Tuple pattern: check all sub-patterns
                let checks: Vec<String> = args.iter().enumerate()
                    .map(|(i, p)| {
                        let sub_scrutinee = format!("{}.Item{}", scrutinee, i + 1);
                        self.emit_pat_check(&sub_scrutinee, *p)
                    })
                    .collect();
                let combined: Vec<String> = checks.into_iter().filter(|s| s != "true").collect();
                if combined.is_empty() { "true".to_string() } else { format!("({})", combined.join(" && ")) }
            }
            _ => "true".to_string(),
        }
    }

    /// Emit variable binding statements for a pattern matched against a scrutinee.
    fn emit_pat_bindings(&mut self, out: &mut Output, scrutinee: &str, pat_id: PatId) {
        let pat = self.body[pat_id].clone();
        match pat {
            Pat::Bind { id, subpat } => {
                let cs_name = self.alloc_binding(id);
                out.writeln(&format!(
                    "var {} = new r2CsRuntime.Slot<object>({});",
                    cs_name, scrutinee
                ));
                if let Some(sub) = subpat {
                    self.emit_pat_bindings(out, scrutinee, sub);
                }
            }
            Pat::TupleStruct { path, args, .. } => {
                if let Some(p) = &path {
                    let segs: Vec<String> = p.segments().iter()
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
                    let segs: Vec<String> = p.segments().iter()
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
                let cs_name = self.bindings.get(id)
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
                // Take bindings from first alternative only
                if let Some(first) = pats.first() {
                    self.collect_bindings_recursive(*first, result);
                }
            }
            Pat::Slice { prefix, slice, suffix } => {
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
        // Find the most recently allocated binding with this Rust name
        for (id, cs_name) in &self.bindings {
            let binding = &self.body[*id];
            if binding.name.as_str() == rust_name {
                return Some(cs_name.clone());
            }
        }
        None
    }
}
