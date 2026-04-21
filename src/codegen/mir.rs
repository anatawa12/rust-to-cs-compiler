/// MIR (Mid-level IR) → C# code generation.
///
/// MIR is the reliable source for function bodies in `after_analysis`
/// (THIR is consumed by borrow checking before `after_analysis` is called).
///
/// Output strategy:
/// - Each MIR basic block becomes a goto-labeled C# block.
/// - For simple linear CFG (no back-edges, single-successor blocks) the
///   labels are elided and the code reads naturally.
/// - `SwitchInt` on a bool becomes `if / else`.
/// - `SwitchInt` on an int becomes `switch`.
/// - Other terminators (Goto, Return, Call, Drop, …) are mapped directly.

use rustc_middle::mir::{
    BasicBlock, BasicBlockData, Body, Local, Operand, Place, PlaceElem,
    Rvalue, StatementKind, TerminatorKind,
};
use rustc_middle::ty::TyCtxt;

use crate::codegen::types::ty_to_cs;
use crate::codegen::writer::CsWriter;

// ── Public entry point ────────────────────────────────────────────────────

/// Emit C# statements for the body of a function.
///
/// `body` is the (optimized) MIR body.  Output is written to `w` at the
/// current indentation level.
pub fn compile_mir_body<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    w: &mut CsWriter,
) {
    let ctx = MirCtx { tcx, body };
    ctx.compile(w);
}

// ── Internal context ──────────────────────────────────────────────────────

struct MirCtx<'a, 'tcx> {
    tcx:  TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
}

impl<'a, 'tcx> MirCtx<'a, 'tcx> {
    fn compile(&self, w: &mut CsWriter) {
        // Declare all non-parameter, non-return locals.
        self.emit_local_decls(w);

        // Emit basic blocks.
        let bbs = self.body.basic_blocks.indices();
        for bb in bbs {
            let data = &self.body.basic_blocks[bb];
            self.compile_bb(bb, data, w);
        }
    }

    /// Declare local variables (skip _0 = return, _1.._arg_count = params).
    fn emit_local_decls(&self, w: &mut CsWriter) {
        let arg_count = self.body.arg_count;
        for (local, decl) in self.body.local_decls.iter_enumerated() {
            let idx = local.index();
            if idx == 0 || (idx >= 1 && idx <= arg_count) {
                continue; // return slot / params already in signature
            }
            let ty_str = ty_to_cs(self.tcx, decl.ty)
                .unwrap_or_else(|| "object /* unknown */".into());
            w.write_line(&format!("{ty_str} {local_name} = default;",
                local_name = local_cs_name(local)));
        }
    }

    /// Compile a single basic block.
    fn compile_bb(&self, bb: BasicBlock, data: &BasicBlockData<'tcx>, w: &mut CsWriter) {
        // Emit label (skip for bb0 which is always the entry).
        if bb.index() != 0 {
            // C# labels need a statement after them; `goto` goes to the label.
            w.write_line(&format!("{}: ;", bb_label(bb)));
        }

        // Emit statements.
        for stmt in &data.statements {
            self.compile_stmt(stmt, w);
        }

        // Emit terminator.
        if let Some(term) = &data.terminator {
            self.compile_terminator(&term.kind, w);
        }
    }

    fn compile_stmt(&self, stmt: &rustc_middle::mir::Statement<'tcx>, w: &mut CsWriter) {
        match &stmt.kind {
            StatementKind::Assign(box (place, rvalue)) => {
                let lhs = self.place_cs(place);
                let rhs = self.rvalue_cs(rvalue);
                w.write_line(&format!("{lhs} = {rhs};"));
            }
            // StorageLive/Dead are hints to the optimizer; no C# equivalent.
            StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => {}
            // Nop, FakeRead, etc. — ignore.
            _ => {}
        }
    }

    fn compile_terminator(&self, kind: &TerminatorKind<'tcx>, w: &mut CsWriter) {
        match kind {
            TerminatorKind::Return => {
                // _0 holds the return value.
                w.write_line("return _0;");
            }
            TerminatorKind::Goto { target } => {
                if target.index() == 0 {
                    // Should never goto bb0 in valid MIR, but handle it.
                    w.write_line("goto _entry;");
                } else {
                    w.write_line(&format!("goto {};", bb_label(*target)));
                }
            }
            TerminatorKind::SwitchInt { discr, targets } => {
                let disc_cs = self.operand_cs(discr);
                let arms: Vec<_> = targets.iter().collect();
                if arms.len() == 1 {
                    // bool-like: true branch vs otherwise.
                    let (val, true_bb) = arms[0];
                    let false_bb = targets.otherwise();
                    w.write_line(&format!("if (({disc_cs}) == {val})"));
                    w.write_line("{");
                    w.indent();
                    w.write_line(&format!("goto {};", bb_label(true_bb)));
                    w.dedent();
                    w.write_line("}");
                    w.write_line("else");
                    w.write_line("{");
                    w.indent();
                    w.write_line(&format!("goto {};", bb_label(false_bb)));
                    w.dedent();
                    w.write_line("}");
                } else {
                    // Multi-arm switch.
                    w.write_line(&format!("switch ({disc_cs})"));
                    w.write_line("{");
                    w.indent();
                    for (val, target_bb) in targets.iter() {
                        w.write_line(&format!("case {val}:"));
                        w.indent();
                        w.write_line(&format!("goto {};", bb_label(target_bb)));
                        w.dedent();
                    }
                    w.write_line("default:");
                    w.indent();
                    w.write_line(&format!("goto {};", bb_label(targets.otherwise())));
                    w.dedent();
                    w.dedent();
                    w.write_line("}");
                }
            }
            TerminatorKind::Call {
                func,
                args,
                destination,
                target,
                ..
            } => {
                let func_cs = self.operand_cs(func);
                let args_cs: Vec<_> = args.iter()
                    .map(|a| self.operand_cs(&a.node))
                    .collect();
                let call_expr = format!("{func_cs}({})", args_cs.join(", "));
                let dest_cs = self.place_cs(destination);
                w.write_line(&format!("{dest_cs} = {call_expr};"));
                if let Some(next_bb) = target {
                    if next_bb.index() != 0 {
                        w.write_line(&format!("goto {};", bb_label(*next_bb)));
                    }
                }
            }
            TerminatorKind::Drop { place, target, .. } => {
                // Emit a call to the drop glue if the type implements Drop.
                let place_cs = self.place_cs(place);
                let ty = place.ty(self.body, self.tcx).ty;
                if let Some(ty_str) = ty_to_cs(self.tcx, ty) {
                    w.write_line(&format!(
                        "/* drop */ (({ty_str}){place_cs}).Dispose();"
                    ));
                }
                w.write_line(&format!("goto {};", bb_label(*target)));
            }
            TerminatorKind::Unreachable => {
                w.write_line("throw new global::System.InvalidOperationException(\"unreachable\");");
            }
            TerminatorKind::UnwindResume | TerminatorKind::UnwindTerminate(_) => {
                w.write_line("throw new global::r2CsRuntime.PanicException(\"unwind\");");
            }
            _ => {
                w.write_line("/* TODO: unsupported terminator */");
            }
        }
    }

    // ── Place / Rvalue / Operand → C# strings ────────────────────────────

    fn place_cs(&self, place: &Place<'tcx>) -> String {
        let mut s = local_cs_name(place.local);
        for proj in place.projection.iter() {
            match proj {
                PlaceElem::Deref => {
                    // Ref<T>.AsRef() / ref indirection — just wrap with a comment.
                    s = format!("(*{s})");
                }
                PlaceElem::Field(field_idx, ty) => {
                    // Try to get the field name from the parent type.
                    let parent_ty = place
                        .ty(self.body, self.tcx)
                        .ty;
                    let name = crate::codegen::expr::field_cs_name_mir(
                        self.tcx, parent_ty, field_idx.index());
                    s = format!("{s}.{name}");
                }
                PlaceElem::Index(local) => {
                    s = format!("{s}[{}]", local_cs_name(*local));
                }
                PlaceElem::ConstantIndex { offset, .. } => {
                    s = format!("{s}[{offset}]");
                }
                PlaceElem::Downcast(_, variant_idx) => {
                    s = format!("{s} /* downcast variant {} */", variant_idx.index());
                }
                _ => {
                    s = format!("{s} /* proj */");
                }
            }
        }
        s
    }

    fn rvalue_cs(&self, rv: &Rvalue<'tcx>) -> String {
        match rv {
            Rvalue::Use(op) => self.operand_cs(op),
            Rvalue::Ref(_, _, place) => {
                // In C# we represent borrows as their place directly.
                self.place_cs(place)
            }
            Rvalue::BinaryOp(op, box (lhs, rhs)) => {
                let l = self.operand_cs(lhs);
                let r = self.operand_cs(rhs);
                use rustc_middle::mir::BinOp;
                let op_str = match op {
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
                    _ => "/* BinOp */+",
                };
                format!("({l} {op_str} {r})")
            }
            Rvalue::UnaryOp(op, operand) => {
                let a = self.operand_cs(operand);
                use rustc_middle::mir::UnOp;
                let op_str = match op {
                    UnOp::Not => "!",
                    UnOp::Neg => "-",
                    UnOp::PtrMetadata => return format!("/* PtrMetadata */{a}"),
                };
                format!("({op_str}{a})")
            }
            Rvalue::Aggregate(kind, fields) => {
                use rustc_middle::mir::AggregateKind;
                match kind.as_ref() {
                    AggregateKind::Tuple => {
                        let parts: Vec<_> = fields.iter()
                            .map(|f| self.operand_cs(f))
                            .collect();
                        format!("({})", parts.join(", "))
                    }
                    AggregateKind::Adt(def_id, variant_idx, _, _, _) => {
                        let adt_def = self.tcx.adt_def(*def_id);
                        let variant = &adt_def.variant(*variant_idx);
                        let ty_str = crate::codegen::types::def_id_to_cs_path(self.tcx, *def_id);
                        let mut field_inits = Vec::new();
                        for (i, (field, op)) in variant.fields.iter().zip(fields.iter()).enumerate() {
                            let f_name = crate::codegen::naming::field_name(field.name.as_str());
                            let val = self.operand_cs(op);
                            field_inits.push(format!("{f_name} = {val}"));
                        }
                        format!("new {ty_str} {{ {} }}", field_inits.join(", "))
                    }
                    _ => format!("/* agg */default"),
                }
            }
            Rvalue::Discriminant(place) => {
                format!("/* discriminant */{}", self.place_cs(place))
            }
            Rvalue::CopyForDeref(place) => self.place_cs(place),
            _ => format!("/* rvalue */default"),
        }
    }

    fn operand_cs(&self, op: &Operand<'tcx>) -> String {
        match op {
            Operand::Copy(place) | Operand::Move(place) => self.place_cs(place),
            Operand::Constant(c) => const_cs(&c.const_),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn local_cs_name(local: Local) -> String {
    if local.index() == 0 {
        "_0".into()
    } else {
        format!("_{}", local.index())
    }
}

fn bb_label(bb: BasicBlock) -> String {
    format!("_bb{}", bb.index())
}

fn const_cs(c: &rustc_middle::mir::Const<'_>) -> String {
    use rustc_middle::mir::Const;
    match c {
        Const::Val(val, ty) => {
            use rustc_middle::mir::ConstValue;
            match val {
                ConstValue::Scalar(scalar) => {
                    use rustc_middle::mir::interpret::Scalar;
                    match scalar {
                        Scalar::Int(si) => {
                            // Extract the raw bit pattern.
                            let bits = si.to_bits(si.size()).unwrap_or(0);
                            format!("{bits}")
                        }
                        Scalar::Ptr(_, _) => "/* ptr const */default".into(),
                    }
                }
                ConstValue::ZeroSized => "default".into(),
                _ => "/* const */default".into(),
            }
        }
        _ => "/* const */default".into(),
    }
}
