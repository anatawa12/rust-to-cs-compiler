// THIR expression and statement → C# code generation.
//
// The entry point is [`compile_expr`].  Each THIR [`ExprKind`] variant
// maps to a C# expression string.  Statements are handled by
// [`compile_block`] which emits full C# statements (with semicolons and
// newlines) via a [`CsWriter`].
//
// Note: this module is currently not called because THIR is stolen before
// `after_analysis`.  It is kept for future use when THIR access is available.
// See `docs/mir-vs-thir.md`.

#![allow(dead_code)]

use rustc_middle::mir::BinOp;
use rustc_middle::thir::{
    BlockId, ExprId, ExprKind, LogicalOp, StmtId, StmtKind, Thir,
};
use rustc_middle::ty::TyCtxt;

use crate::codegen::naming;
use crate::codegen::types::ty_to_cs;
use crate::codegen::writer::CsWriter;

/// Context passed through the expression compiler.
pub struct ExprCtx<'tcx, 'thir> {
    pub tcx:       TyCtxt<'tcx>,
    pub thir:      &'thir Thir<'tcx>,
    /// Counter used to assign unique indices to local variables.
    pub local_idx: usize,
}

impl<'tcx, 'thir> ExprCtx<'tcx, 'thir> {
    pub fn new(tcx: TyCtxt<'tcx>, thir: &'thir Thir<'tcx>) -> Self {
        ExprCtx { tcx, thir, local_idx: 0 }
    }

    fn next_idx(&mut self) -> usize {
        let i = self.local_idx;
        self.local_idx += 1;
        i
    }
}

// ── Expression compiler ───────────────────────────────────────────────────

/// Compile a single THIR expression to a C# expression string.
///
/// Returns `None` for unsupported expression kinds; callers emit a TODO
/// comment in that case.
pub fn compile_expr<'tcx>(
    ctx: &mut ExprCtx<'tcx, '_>,
    expr_id: ExprId,
) -> Option<String> {
    let expr = &ctx.thir.exprs[expr_id];
    match &expr.kind {
        // Pass through scope wrappers transparently.
        ExprKind::Scope { value, .. } => compile_expr(ctx, *value),
        ExprKind::Use  { source }     => compile_expr(ctx, *source),
        ExprKind::NeverToAny { source } | ExprKind::Cast { source } => {
            compile_expr(ctx, *source)
        }

        // ── literals ─────────────────────────────────────────────────────
        ExprKind::Literal { lit, neg } => {
            let s = lit_to_cs(lit, *neg)?;
            Some(s)
        }
        ExprKind::ZstLiteral { .. } => {
            // Zero-sized struct literal (e.g. unit struct, `()`).
            let ty_str = ty_to_cs(ctx.tcx, expr.ty)
                .unwrap_or_else(|| "/* zst */global::r2CsRuntime.Void".into());
            Some(format!("default({ty_str})"))
        }
        ExprKind::NonHirLiteral { lit, .. } => {
            Some(format!("{}", lit.to_bits(lit.size())))
        }

        // ── variables ────────────────────────────────────────────────────
        ExprKind::VarRef { id } => {
            // Map HirId → local-variable name.  We use the HirId's local_id
            // as the index for uniqueness.
            let hir_id = id.0;
            let name = ctx.tcx.hir_node(hir_id).ident().map_or_else(
                || format!("l__var_{}", hir_id.local_id.index()),
                |ident| naming::local_name(ident.as_str(), hir_id.local_id.index()),
            );
            Some(name)
        }

        // ── binary operations ─────────────────────────────────────────────
        ExprKind::Binary { op, lhs, rhs } => {
            let l = compile_expr(ctx, *lhs)?;
            let r = compile_expr(ctx, *rhs)?;
            let op_str = binop_to_cs(op)?;
            Some(format!("({l} {op_str} {r})"))
        }
        ExprKind::LogicalOp { op, lhs, rhs } => {
            let l = compile_expr(ctx, *lhs)?;
            let r = compile_expr(ctx, *rhs)?;
            let op_str = match op {
                LogicalOp::And => "&&",
                LogicalOp::Or  => "||",
            };
            Some(format!("({l} {op_str} {r})"))
        }
        ExprKind::Unary { op, arg } => {
            use rustc_middle::mir::UnOp;
            let a = compile_expr(ctx, *arg)?;
            let op_str = match op {
                UnOp::Not => "!",
                UnOp::Neg => "-",
                UnOp::PtrMetadata => return None, // intrinsic, skip
            };
            Some(format!("({op_str}{a})"))
        }

        // ── assignment ────────────────────────────────────────────────────
        ExprKind::Assign { lhs, rhs } => {
            let l = compile_expr(ctx, *lhs)?;
            let r = compile_expr(ctx, *rhs)?;
            Some(format!("{l} = {r}"))
        }
        ExprKind::AssignOp { op, lhs, rhs } => {
            let l = compile_expr(ctx, *lhs)?;
            let r = compile_expr(ctx, *rhs)?;
            use rustc_middle::mir::AssignOp;
            let op_str = match op {
                AssignOp::AddAssign    => "+=",
                AssignOp::SubAssign    => "-=",
                AssignOp::MulAssign    => "*=",
                AssignOp::DivAssign    => "/=",
                AssignOp::RemAssign    => "%=",
                AssignOp::BitAndAssign => "&=",
                AssignOp::BitOrAssign  => "|=",
                AssignOp::BitXorAssign => "^=",
                AssignOp::ShlAssign    => "<<=",
                AssignOp::ShrAssign    => ">>=",
            };
            Some(format!("{l} {op_str} {r}"))
        }

        // ── field access ──────────────────────────────────────────────────
        ExprKind::Field { lhs, name, .. } => {
            let base = compile_expr(ctx, *lhs)?;
            // For named fields the name is the field index; we need the
            // actual field name from the ADT definition.
            let lhs_ty = ctx.thir.exprs[*lhs].ty;
            let field_name = field_cs_name(ctx.tcx, lhs_ty, name.index());
            Some(format!("{base}.{field_name}"))
        }

        // ── tuple field access ────────────────────────────────────────────
        ExprKind::Tuple { fields } => {
            let parts: Option<Vec<_>> = fields
                .iter()
                .map(|f| compile_expr(ctx, *f))
                .collect();
            Some(format!("({})", parts?.join(", ")))
        }

        // ── ADT construction ──────────────────────────────────────────────
        ExprKind::Adt(adt) => {
            let ty_str = ty_to_cs(ctx.tcx, expr.ty)?;
            if adt.fields.is_empty() {
                return Some(format!("default({ty_str})"));
            }
            // Emit: new s_T { f_field1 = expr1, ... }
            let mut field_inits = Vec::new();
            for f in adt.fields.iter() {
                let lhs_ty = expr.ty;
                let cs_name = field_cs_name(ctx.tcx, lhs_ty, f.name.index());
                let val = compile_expr(ctx, f.expr)?;
                field_inits.push(format!("{cs_name} = {val}"));
            }
            Some(format!("new {ty_str} {{ {} }}", field_inits.join(", ")))
        }

        // ── function calls ────────────────────────────────────────────────
        ExprKind::Call { fun, args, .. } => {
            let callee = compile_expr(ctx, *fun)?;
            let cs_args: Option<Vec<_>> = args
                .iter()
                .map(|a| compile_expr(ctx, *a))
                .collect();
            Some(format!("{callee}({})", cs_args?.join(", ")))
        }

        // ── borrow / deref ────────────────────────────────────────────────
        ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } => {
            // Borrows/derefs are transparent in the C# representation:
            // Ref<T> construction happens at the call site, not in the expression.
            compile_expr(ctx, *arg)
        }

        // ── if expression ─────────────────────────────────────────────────
        // if-expressions become ternary when they have both arms,
        // otherwise we emit a TODO and handle them as statements.
        ExprKind::If { cond, then, else_opt: Some(else_expr), .. } => {
            let c = compile_expr(ctx, *cond)?;
            let t = compile_expr(ctx, *then)?;
            let e = compile_expr(ctx, *else_expr)?;
            Some(format!("({c} ? {t} : {e})"))
        }
        ExprKind::If { .. } => None, // handled as statement

        // ── block ─────────────────────────────────────────────────────────
        ExprKind::Block { block: block_id } => {
            let block = &ctx.thir.blocks[*block_id];
            if block.stmts.is_empty() {
                if let Some(expr_id) = block.expr {
                    return compile_expr(ctx, expr_id);
                }
                return Some("default(global::r2CsRuntime.Void)".into());
            }
            // Multi-statement blocks can't be inlined as a single expression.
            None
        }

        // ── return ────────────────────────────────────────────────────────
        ExprKind::Return { value: Some(v) } => {
            let val = compile_expr(ctx, *v)?;
            Some(format!("return {val}"))
        }
        ExprKind::Return { value: None } => {
            Some("return default(global::r2CsRuntime.Void)".into())
        }

        // ── break / continue ──────────────────────────────────────────────
        ExprKind::Break { value: None, .. } => Some("break".into()),
        ExprKind::Continue { .. }            => Some("continue".into()),

        // Unsupported.
        _ => None,
    }
}

// ── Block / statement compiler ────────────────────────────────────────────

/// Compile a THIR `BlockId` into C# statements written to `w`.
pub fn compile_block<'tcx>(
    ctx: &mut ExprCtx<'tcx, '_>,
    block_id: BlockId,
    w: &mut CsWriter,
) {
    let block = ctx.thir.blocks[block_id].clone();
    for stmt_id in block.stmts.iter() {
        compile_stmt(ctx, *stmt_id, w);
    }
    if let Some(expr_id) = block.expr {
        // Tail expression — if it produces a value, emit `return`.
        let expr = &ctx.thir.exprs[expr_id];
        let is_unit = matches!(expr.ty.kind(), rustc_middle::ty::TyKind::Tuple(f) if f.is_empty());
        if is_unit {
            // side-effects only
            compile_expr_stmt(ctx, expr_id, w);
        } else {
            match compile_expr(ctx, expr_id) {
                Some(cs) => w.write_line(&format!("return {cs};")),
                None     => w.write_line("/* TODO: tail expression */"),
            }
        }
    }
}

/// Compile a single THIR statement.
fn compile_stmt<'tcx>(
    ctx: &mut ExprCtx<'tcx, '_>,
    stmt_id: StmtId,
    w: &mut CsWriter,
) {
    let stmt = ctx.thir.stmts[stmt_id].clone();
    match &stmt.kind {
        StmtKind::Expr { expr, .. } => {
            compile_expr_stmt(ctx, *expr, w);
        }
        StmtKind::Let { pattern, initializer, .. } => {
            compile_let(ctx, pattern, *initializer, w);
        }
    }
}

/// Emit an expression as a C# statement (expression ; or if/while body).
fn compile_expr_stmt<'tcx>(
    ctx: &mut ExprCtx<'tcx, '_>,
    expr_id: ExprId,
    w: &mut CsWriter,
) {
    let expr = &ctx.thir.exprs[expr_id];
    // Special-case control flow constructs that become C# statements.
    match &expr.kind.clone() {
        ExprKind::Scope { value, .. } => {
            compile_expr_stmt(ctx, *value, w);
        }
        ExprKind::Block { block: block_id } => {
            w.write_line("{");
            w.indent();
            compile_block(ctx, *block_id, w);
            w.dedent();
            w.write_line("}");
        }
        ExprKind::If { cond, then, else_opt, .. } => {
            let cond_cs = compile_expr(ctx, *cond)
                .unwrap_or_else(|| "/* TODO cond */".into());
            w.write_line(&format!("if ({cond_cs})"));
            w.write_line("{");
            w.indent();
            compile_expr_stmt(ctx, *then, w);
            w.dedent();
            w.write_line("}");
            if let Some(else_expr) = else_opt {
                w.write_line("else");
                w.write_line("{");
                w.indent();
                compile_expr_stmt(ctx, *else_expr, w);
                w.dedent();
                w.write_line("}");
            }
        }
        ExprKind::Loop { body } => {
            w.write_line("while (true)");
            w.write_line("{");
            w.indent();
            compile_expr_stmt(ctx, *body, w);
            w.dedent();
            w.write_line("}");
        }
        ExprKind::Return { value } => {
            match value {
                Some(v) => {
                    match compile_expr(ctx, *v) {
                        Some(cs) => w.write_line(&format!("return {cs};")),
                        None     => w.write_line("return /* TODO */;"),
                    }
                }
                None => w.write_line("return;"),
            }
        }
        ExprKind::Break { value: None, .. } => w.write_line("break;"),
        ExprKind::Continue { .. }            => w.write_line("continue;"),
        _ => {
            match compile_expr(ctx, expr_id) {
                Some(cs) => w.write_line(&format!("{cs};")),
                None     => {
                    w.write_line(&format!(
                        "/* TODO: {:?} */",
                        std::mem::discriminant(&ctx.thir.exprs[expr_id].kind)
                    ));
                }
            }
        }
    }
}

/// Compile a `let` binding into a C# local declaration.
fn compile_let<'tcx>(
    ctx: &mut ExprCtx<'tcx, '_>,
    pat: &rustc_middle::thir::Pat<'tcx>,
    initializer: Option<ExprId>,
    w: &mut CsWriter,
) {
    use rustc_middle::thir::PatKind;
    match &pat.kind {
        PatKind::Binding { name, .. } => {
            let idx     = ctx.next_idx();
            let cs_name = naming::local_name(name.as_str(), idx);
            let ty_str  = ty_to_cs(ctx.tcx, pat.ty)
                .unwrap_or_else(|| "var".into());
            match initializer.and_then(|id| compile_expr(ctx, id)) {
                Some(init) => {
                    w.write_line(&format!("{ty_str} {cs_name} = {init};"));
                }
                None => {
                    w.write_line(&format!("{ty_str} {cs_name} = default;"));
                }
            }
        }
        PatKind::Wild => {
            // `let _ = expr` — emit a discard.
            if let Some(id) = initializer {
                match compile_expr(ctx, id) {
                    Some(cs) => w.write_line(&format!("_ = {cs};")),
                    None     => w.write_line("/* discarded TODO */"),
                }
            }
        }
        _ => {
            // Pattern matching — not yet supported; emit a TODO.
            w.write_line("/* TODO: pattern let */");
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Maps a `BinOp` to the C# operator string.
fn binop_to_cs(op: &BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Add | BinOp::AddUnchecked => "+",
        BinOp::Sub | BinOp::SubUnchecked => "-",
        BinOp::Mul | BinOp::MulUnchecked => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::BitAnd => "&",
        BinOp::BitOr  => "|",
        BinOp::BitXor => "^",
        BinOp::Shl | BinOp::ShlUnchecked => "<<",
        BinOp::Shr | BinOp::ShrUnchecked => ">>",
        BinOp::Eq  => "==",
        BinOp::Ne  => "!=",
        BinOp::Lt  => "<",
        BinOp::Le  => "<=",
        BinOp::Gt  => ">",
        BinOp::Ge  => ">=",
        _ => return None,
    })
}

/// Given the type of an expression and a field index, return the C# field name.
fn field_cs_name<'tcx>(
    _tcx: TyCtxt<'tcx>,
    ty: rustc_middle::ty::Ty<'tcx>,
    field_idx: usize,
) -> String {
    field_cs_name_impl(ty, field_idx)
}

/// Public variant used by `mir.rs` (same logic, no TyCtxt needed).
pub fn field_cs_name_mir<'tcx>(
    _tcx: TyCtxt<'tcx>,
    ty: rustc_middle::ty::Ty<'tcx>,
    field_idx: usize,
) -> String {
    field_cs_name_impl(ty, field_idx)
}

fn field_cs_name_impl(ty: rustc_middle::ty::Ty<'_>, field_idx: usize) -> String {
    use rustc_middle::ty::TyKind;
    match ty.kind() {
        TyKind::Adt(adt_def, _) => {
            // Use the first (and only) variant for structs.
            let variant = adt_def.variant(rustc_abi::VariantIdx::ZERO);
            if field_idx < variant.fields.len() {
                let name = variant.fields[rustc_abi::FieldIdx::from_usize(field_idx)]
                    .name
                    .as_str();
                naming::field_name(name)
            } else {
                format!("f_{field_idx}")
            }
        }
        TyKind::Tuple(_) => {
            // Tuple fields → Item1, Item2, … (C# ValueTuple naming).
            format!("Item{}", field_idx + 1)
        }
        _ => format!("f_{field_idx}"),
    }
}

/// Convert a HIR literal to a C# literal string.
fn lit_to_cs(lit: &rustc_hir::Lit, neg: bool) -> Option<String> {
    use rustc_ast::LitKind;
    let s = match &lit.node {
        LitKind::Int(v, _) => {
            let val = v.0;
            if neg {
                format!("(-{val})")
            } else {
                format!("{val}")
            }
        }
        LitKind::Float(sym, _) => {
            if neg {
                format!("(-{})", sym.as_str())
            } else {
                sym.as_str().to_owned()
            }
        }
        LitKind::Bool(b) => b.to_string(),
        LitKind::Char(c) => format!("'{c}'"),
        LitKind::Str(sym, _) => format!("\"{}\"", sym.as_str()),
        LitKind::Byte(b) => format!("((byte){b})"),
        _ => return None,
    };
    Some(s)
}
