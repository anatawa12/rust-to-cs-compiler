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
///
/// # Reference representation
///
/// Thin Rust references (`&T`, `&mut T`) and raw pointers (`*const T`,
/// `*mut T`) are represented as C# raw pointers (`T*`).  All generated
/// function bodies are marked `unsafe`.
///
/// This is safe because all Rust locals (including async-fn locals, which
/// are represented as non-heap C# structs) live on the C# stack.  The GC
/// never relocates stack values, so `T*` pointers remain valid for the
/// lifetime of the stack frame — which exactly matches Rust's borrow rules.
///
/// Fat references keep their wrapper types:
///   `&[T]`  → `LenRef<Slice<T>>`  (needs a length field alongside the pointer)
///   `&dyn T` → `DynRef<T>`         (needs a vtable field alongside the pointer)
///
/// The `Var<T>` heap-boxing mechanism is **not used**: all Rust locals are
/// plain C# value-type locals whose address can be taken directly with `&`.

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
        self.emit_local_decls(w);
        let bbs = self.body.basic_blocks.indices();
        for bb in bbs {
            let data = &self.body.basic_blocks[bb];
            self.compile_bb(bb, data, w);
        }
    }

    /// Declare local variables (skip _1.._arg_count = params).
    fn emit_local_decls(&self, w: &mut CsWriter) {
        let arg_count = self.body.arg_count;
        for (local, decl) in self.body.local_decls.iter_enumerated() {
            let idx = local.index();
            if idx >= 1 && idx <= arg_count {
                continue; // params are already in the signature
            }
            let ty_str = ty_to_cs(self.tcx, decl.ty)
                .unwrap_or_else(|| "object /* unknown */".into());
            w.write_line(&format!("{ty_str} {} = default;", local_cs_name(local)));
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
            StatementKind::Assign(assign) => {
                let (place, rvalue) = assign.as_ref();
                // With raw-pointer references, writing through a pointer is
                // expressed as `(*ptr) = value`, which is a valid C# lvalue.
                // `place_cs` already emits `(*ptr)` for a Deref of a pointer
                // type, so the generic `lhs = rhs` path handles this correctly
                // without any special-casing.
                let lhs = self.place_cs(place);
                let rhs = self.rvalue_cs(rvalue);
                w.write_line(&format!("{lhs} = {rhs};"));
            }
            StatementKind::SetDiscriminant { place, variant_index } => {
                // Set the discriminant field on the enum struct.
                let p = self.place_cs(place);
                let disc = variant_index.index();
                w.write_line(&format!("{p}.f_discriminant = {disc};"));
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
                let disc_ty = discr.ty(self.body, self.tcx);
                let arms: Vec<_> = targets.iter().collect();

                // Check if this is a bool branch (single arm, discriminant is bool).
                let is_bool = matches!(disc_ty.kind(), rustc_middle::ty::TyKind::Bool);

                if is_bool && arms.len() == 1 {
                    let (val, branch_bb) = arms[0];
                    let other_bb = targets.otherwise();
                    // val == 0 means "if false goto branch; else goto other"
                    // val == 1 means "if true goto branch; else goto other"
                    let (true_bb, false_bb) = if val == 0 {
                        (other_bb, branch_bb)
                    } else {
                        (branch_bb, other_bb)
                    };
                    w.write_line(&format!("if ({disc_cs})"));
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
                } else if arms.len() == 1 {
                    // Single arm on a non-bool (e.g. byte discriminant with one case).
                    let (val, branch_bb) = arms[0];
                    let other_bb = targets.otherwise();
                    w.write_line(&format!("if (({disc_cs}) == {val})"));
                    w.write_line("{");
                    w.indent();
                    w.write_line(&format!("goto {};", bb_label(branch_bb)));
                    w.dedent();
                    w.write_line("}");
                    w.write_line("else");
                    w.write_line("{");
                    w.indent();
                    w.write_line(&format!("goto {};", bb_label(other_bb)));
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
                // If the type has an explicit `Drop` impl, call the generated
                // static `m_drop` method.  Otherwise there is nothing to do.
                let ty = place.ty(self.body, self.tcx).ty;
                use rustc_middle::ty::TyKind;
                if let TyKind::Adt(adt_def, _) = ty.kind() {
                    if self.tcx.adt_destructor(adt_def.did()).is_some() {
                        let place_cs = self.place_cs(place);
                        if let Some(ty_str) = ty_to_cs(self.tcx, ty) {
                            w.write_line(&format!("{ty_str}.m_drop(&{place_cs});"));
                        }
                    }
                }
                w.write_line(&format!("goto {};", bb_label(*target)));
            }
            TerminatorKind::Unreachable => {
                w.write_line("throw new global::System.InvalidOperationException(\"unreachable\");");
            }
            TerminatorKind::UnwindResume | TerminatorKind::UnwindTerminate(_) => {
                w.write_line("throw new global::r2CsRuntime.PanicException(\"unwind\");");
            }
            TerminatorKind::Assert { cond, expected, target, .. } => {
                // Runtime assertion (e.g. overflow check).  In debug mode we
                // could emit a real check; for now just goto the success block.
                let cond_cs = self.operand_cs(cond);
                let expected_cs = if *expected { "true" } else { "false" };
                w.write_line(&format!(
                    "global::System.Diagnostics.Debug.Assert(({cond_cs}) == {expected_cs});"
                ));
                w.write_line(&format!("goto {};", bb_label(*target)));
            }
            _ => {
                w.write_line("/* TODO: unsupported terminator */");
            }
        }
    }

    // ── Place / Rvalue / Operand → C# strings ────────────────────────────

    fn place_cs(&self, place: &Place<'tcx>) -> String {
        // All locals are plain C# value-type locals; no Var<T> boxing needed.
        let mut s = local_cs_name(place.local);
        // Track the accumulated type so we can name struct fields correctly.
        let mut cur_ty = self.body.local_decls[place.local].ty;
        // Track which enum variant is active (set by Downcast projections).
        let mut cur_variant_idx: Option<rustc_abi::VariantIdx> = None;
        // Track whether the last Deref was on a slice reference (&[T]).
        // When true, the next Index/ConstantIndex projection should use .GetElement()
        // instead of [idx], because LenRef<Slice<T>> doesn't support operator[].
        let mut slice_ref_base: Option<String> = None;

        for proj in place.projection.iter() {
            match proj {
                PlaceElem::Deref => {
                    use rustc_middle::ty::TyKind;
                    let (new_ty, is_ptr) = match cur_ty.kind() {
                        TyKind::Ref(_, inner, _) => (*inner, true),
                        TyKind::RawPtr(inner, _) => (*inner, true),
                        _ => (cur_ty, false),
                    };
                    if is_ptr && matches!(new_ty.kind(), TyKind::Slice(_)) {
                        // &[T] / *[T]: LenRef<Slice<T>> — element access goes
                        // through .GetElement(); record the base for the Index arm.
                        slice_ref_base = Some(s.clone());
                    } else if is_ptr {
                        // Thin pointer (*T): emit C# pointer dereference.
                        slice_ref_base = None;
                        s = format!("(*{s})");
                    } else {
                        slice_ref_base = None;
                    }
                    cur_ty = new_ty;
                    cur_variant_idx = None;
                }
                PlaceElem::Field(field_idx, field_ty) => {
                    use rustc_middle::ty::TyKind;
                    let name = match cur_ty.kind() {
                        TyKind::Adt(adt_def, _) => {
                            // Use the active variant (from a preceding Downcast),
                            // or fall back to variant 0 for structs.
                            let v_idx = cur_variant_idx
                                .unwrap_or(rustc_abi::VariantIdx::ZERO);
                            let variant = adt_def.variant(v_idx);
                            if field_idx.index() < variant.fields.len() {
                                crate::codegen::naming::field_name(
                                    variant.fields[field_idx].name.as_str()
                                )
                            } else {
                                format!("f_{}", field_idx.index())
                            }
                        }
                        TyKind::Tuple(_) => {
                            format!("Item{}", field_idx.index() + 1)
                        }
                        TyKind::Closure(..) => {
                            format!("_{}", field_idx.index())
                        }
                        _ => format!("f_{}", field_idx.index()),
                    };
                    s = format!("{s}.{name}");
                    cur_ty = field_ty;
                    cur_variant_idx = None;
                    slice_ref_base = None;
                }
                PlaceElem::Index(local) => {
                    if let Some(ref base) = slice_ref_base {
                        // Slice element access through a LenRef<Slice<T>>: use GetElement.
                        // Use the fully-qualified static call form so the extension method
                        // is found without requiring a `using r2CsRuntime;` directive.
                        s = format!(
                            "global::r2CsRuntime.SliceExt.GetElement({base}, (nint){})",
                            local_cs_name(local)
                        );
                    } else {
                        s = format!("{s}[{}]", local_cs_name(local));
                    }
                    use rustc_middle::ty::TyKind;
                    cur_ty = match cur_ty.kind() {
                        TyKind::Array(inner, _) | TyKind::Slice(inner) => *inner,
                        _ => cur_ty,
                    };
                    cur_variant_idx = None;
                    slice_ref_base = None;
                }
                PlaceElem::ConstantIndex { offset, .. } => {
                    if let Some(ref base) = slice_ref_base {
                        s = format!(
                            "global::r2CsRuntime.SliceExt.GetElement({base}, {offset})"
                        );
                    } else {
                        s = format!("{s}[{offset}]");
                    }
                    cur_variant_idx = None;
                    slice_ref_base = None;
                }
                PlaceElem::Downcast(_, variant_idx) => {
                    // Downcast selects a variant; emit access to the payload field.
                    use rustc_middle::ty::TyKind;
                    if let TyKind::Adt(adt_def, _) = cur_ty.kind() {
                        let variant = &adt_def.variant(variant_idx);
                        if !variant.fields.is_empty() {
                            let vname = variant.name.as_str();
                            s = format!("{s}.f_{vname}");
                        }
                        // Record which variant is active for subsequent Field projections.
                        cur_variant_idx = Some(variant_idx);
                    }
                    slice_ref_base = None;
                }
                _ => {
                    s = format!("{s} /* proj */");
                    cur_variant_idx = None;
                    slice_ref_base = None;
                }
            }
        }
        s
    }

    fn rvalue_cs(&self, rv: &Rvalue<'tcx>) -> String {
        match rv {
            Rvalue::Use(op) => self.operand_cs(op),
            Rvalue::Ref(_, _, place) => {
                // &T / &mut T → T*: take the address of the place.
                // For slice references (&[T]) we keep LenRef<Slice<T>> for now
                // (the place is already a LenRef produced by a prior call).
                use rustc_middle::ty::TyKind;
                let inner_ty = place.ty(self.body, self.tcx).ty;
                if matches!(inner_ty.kind(), TyKind::Slice(_)) {
                    // LenRef is produced by external calls (e.g. as_bytes); pass through.
                    self.place_cs(place)
                } else {
                    format!("&{}", self.place_cs(place))
                }
            }
            Rvalue::BinaryOp(op, operands) => {
                let (lhs, rhs) = operands.as_ref();
                let l = self.operand_cs(lhs);
                let r = self.operand_cs(rhs);
                use rustc_middle::mir::BinOp;
                // WithOverflow variants return a (value, bool) tuple in MIR.
                // In C# we represent these as (value, false) — no overflow check.
                let op_str = match op {
                    BinOp::Add | BinOp::AddUnchecked | BinOp::AddWithOverflow => "+",
                    BinOp::Sub | BinOp::SubUnchecked | BinOp::SubWithOverflow => "-",
                    BinOp::Mul | BinOp::MulUnchecked | BinOp::MulWithOverflow => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr  => "|",
                    BinOp::BitXor => "^",
                    // C# requires the shift count to be int.
                    BinOp::Shl | BinOp::ShlUnchecked => {
                        return format!("({l} << (int){r})");
                    }
                    BinOp::Shr | BinOp::ShrUnchecked => {
                        return format!("({l} >> (int){r})");
                    }
                    BinOp::Eq  => "==",
                    BinOp::Ne  => "!=",
                    BinOp::Lt  => "<",
                    BinOp::Le  => "<=",
                    BinOp::Gt  => ">",
                    BinOp::Ge  => ">=",
                    BinOp::Cmp => return format!("global::r2CsRuntime.Intrinsics.Cmp({l}, {r})"),
                    BinOp::Offset => return format!("({l} + {r})"),
                };
                // WithOverflow ops produce a tuple; emit `(result, false)`.
                // Also cast the result back to the operand type: C# arithmetic on
                // narrow integer types (byte, sbyte, short, ushort) promotes to int,
                // so we need an explicit cast to keep the tuple element type correct.
                if matches!(op, BinOp::AddWithOverflow | BinOp::SubWithOverflow | BinOp::MulWithOverflow) {
                    let lhs_ty = lhs.ty(self.body, self.tcx);
                    match ty_to_cs(self.tcx, lhs_ty) {
                        Some(ty_str) => format!("(({ty_str})({l} {op_str} {r}), false)"),
                        None         => format!("({l} {op_str} {r}, false)"),
                    }
                } else {
                    format!("({l} {op_str} {r})")
                }
            }
            Rvalue::UnaryOp(op, operand) => {
                let a = self.operand_cs(operand);
                use rustc_middle::mir::UnOp;
                match op {
                    UnOp::Not => {
                        // In Rust, `!` means logical NOT for bool, bitwise NOT for integers.
                        // In C#, `!` is only logical NOT; bitwise NOT is `~`.
                        let ty = operand.ty(self.body, self.tcx);
                        if ty.is_bool() {
                            format!("(!{a})")
                        } else {
                            format!("(~{a})")
                        }
                    }
                    UnOp::Neg => format!("(-{a})"),
                    UnOp::PtrMetadata => {
                        // Extract metadata from a fat pointer.
                        // For slice references (&[T]): the metadata is the element count.
                        let op_ty = operand.ty(self.body, self.tcx);
                        use rustc_middle::ty::TyKind;
                        match op_ty.kind() {
                            TyKind::Ref(_, inner, _) | TyKind::RawPtr(inner, _)
                                if matches!(inner.kind(), TyKind::Slice(_)) =>
                            {
                                format!("(nuint){a}.Length")
                            }
                            _ => format!("/* PtrMetadata */{a}"),
                        }
                    }
                }
            }
            Rvalue::Aggregate(kind, fields) => {
                use rustc_middle::mir::AggregateKind;
                match kind.as_ref() {
                    AggregateKind::Tuple => {
                        let parts: Vec<_> = fields.iter()
                            .map(|f| self.operand_cs(f))
                            .collect();
                        if parts.len() == 1 {
                            // C# has no single-element tuple literal; use the factory
                            // method which infers the type argument.
                            format!("global::System.ValueTuple.Create({})", parts[0])
                        } else {
                            format!("({})", parts.join(", "))
                        }
                    }
                    AggregateKind::Adt(def_id, variant_idx, substs, _, _) => {
                        let adt_def = self.tcx.adt_def(*def_id);
                        let variant = &adt_def.variant(*variant_idx);
                        // Use the substituted type path (includes generic args like <int, bool>).
                        let ty_str = {
                            let ty = rustc_middle::ty::Ty::new_adt(
                                self.tcx, adt_def, substs
                            );
                            crate::codegen::types::ty_to_cs(self.tcx, ty)
                                .unwrap_or_else(|| {
                                    crate::codegen::types::def_id_to_cs_path(self.tcx, *def_id)
                                })
                        };
                        let mut field_inits: Vec<String> = Vec::new();

                        if adt_def.is_enum() {
                            // Enum construction: set discriminant + payload wrapper field.
                            let v_idx = variant_idx.index();
                            let vname = variant.name.as_str();
                            field_inits.push(format!("f_discriminant = {v_idx}"));
                            if !variant.fields.is_empty() {
                                // The payload struct is nested inside the enum struct:
                                // e.g. `s_Shape.s_Shape_Circle` not `s_Shape_Circle`.
                                let base_ty = crate::codegen::types::def_id_to_cs_path(self.tcx, *def_id);
                                let enum_short = base_ty.split('.').last().unwrap_or("").to_string();
                                let payload_inner = format!("{enum_short}_{vname}");
                                let payload_ty = format!("{base_ty}.{payload_inner}");
                                let inner_fields: Vec<String> = variant.fields.iter()
                                    .zip(fields.iter())
                                    .map(|(f, op)| {
                                        let fname = crate::codegen::naming::field_name(f.name.as_str());
                                        let val = self.operand_cs(op);
                                        format!("{fname} = {val}")
                                    })
                                    .collect();
                                field_inits.push(format!(
                                    "f_{vname} = new {payload_ty} {{ {} }}",
                                    inner_fields.join(", ")
                                ));
                            }
                        } else {
                            // Struct construction.
                            for (field, op) in variant.fields.iter().zip(fields.iter()) {
                                let f_name = crate::codegen::naming::field_name(field.name.as_str());
                                let val = self.operand_cs(op);
                                field_inits.push(format!("{f_name} = {val}"));
                            }
                        }
                        format!("new {ty_str} {{ {} }}", field_inits.join(", "))
                    }
                    _ => format!("/* agg */default"),
                }
            }
            Rvalue::Discriminant(place) => {
                // Access the discriminant field on the enum struct.
                let p = self.place_cs(place);
                format!("{p}.f_discriminant")
            }
            Rvalue::CopyForDeref(place) => self.place_cs(place),
            Rvalue::Cast(_, operand, ty) => {
                // Cast-to-function-pointer: C# requires `&MethodName` syntax, not `(T)method`.
                if matches!(ty.kind(), rustc_middle::ty::TyKind::FnPtr(..)) {
                    let op_cs = self.operand_cs(operand);
                    let ty_str = ty_to_cs(self.tcx, *ty)
                        .unwrap_or_else(|| "object /* unknown */".into());
                    // If the operand is already an address expression (starts with `&`), keep it.
                    // Otherwise emit `&operand` for function items.
                    if op_cs.starts_with('&') {
                        return format!("({ty_str}){op_cs}");
                    }
                    return format!("({ty_str})&{op_cs}");
                }
                // All other casts: emit a C# explicit cast: `(TargetType)operand`.
                let op_cs = self.operand_cs(operand);
                let ty_str = ty_to_cs(self.tcx, *ty)
                    .unwrap_or_else(|| "object /* unknown */".into());
                format!("({ty_str}){op_cs}")
            }
            Rvalue::Repeat(operand, count) => {
                // `[expr; N]` — emit a runtime array fill.
                let val = self.operand_cs(operand);
                let n = count.try_to_target_usize(self.tcx).unwrap_or(0);
                format!("global::r2CsRuntime.Intrinsics.Repeat({val}, {n})")
            }
            Rvalue::RawPtr(_, place) => {
                // *const T / *mut T → T*: take the address of the place.
                format!("&{}", self.place_cs(place))
            }
            Rvalue::ThreadLocalRef(def_id) => {
                // Thread-local static — emit a reference to the static field.
                crate::codegen::types::def_id_to_cs_path(self.tcx, *def_id)
            }
            Rvalue::WrapUnsafeBinder(operand, _) => {
                // Unsafe binder — pass through the inner value.
                self.operand_cs(operand)
            }
            #[allow(unreachable_patterns)]
            _ => format!("/* rvalue */default"),
        }
    }

    fn operand_cs(&self, op: &Operand<'tcx>) -> String {
        match op {
            Operand::Copy(place) | Operand::Move(place) => self.place_cs(place),
            Operand::Constant(c) => const_cs(self.tcx, &c.const_),
            Operand::RuntimeChecks(_) => {
                // RuntimeChecks is a compile-time flag (overflow/UB checks).
                // In C# we represent it as a bool literal.
                "false".into()
            }
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

fn const_cs<'tcx>(tcx: TyCtxt<'tcx>, c: &rustc_middle::mir::Const<'tcx>) -> String {
    use rustc_middle::mir::Const;
    match c {
        Const::Ty(ty, ty_const) => {
            // Constants from the type system (e.g. range start/end in SwitchInt).
            // `try_to_leaf` gives a ScalarInt; sign-extend based on the type.
            if let Some(si) = ty_const.try_to_leaf() {
                use rustc_middle::ty::TyKind;
                use rustc_middle::ty::IntTy;
                match ty.kind() {
                    TyKind::Int(k) => {
                        let bits = si.to_bits(si.size());
                        let signed: i128 = match k {
                            IntTy::I8    => (bits as i8)   as i128,
                            IntTy::I16   => (bits as i16)  as i128,
                            IntTy::I32   => (bits as i32)  as i128,
                            IntTy::I64   => (bits as i64)  as i128,
                            IntTy::I128  => bits as i128,
                            IntTy::Isize => (bits as i64)  as i128,
                        };
                        let cs_ty = crate::codegen::types::int_ty_cs(k);
                        return if signed < i32::MIN as i128 || signed > i32::MAX as i128 {
                            format!("({cs_ty}){signed}L")
                        } else {
                            format!("{signed}")
                        };
                    }
                    _ => {
                        let bits = si.to_bits_unchecked();
                        return format!("{bits}");
                    }
                }
            }
            "/* ty const */default".into()
        }
        Const::Val(val, ty) => {
            use rustc_middle::mir::ConstValue;
            use rustc_middle::ty::TyKind;
            match val {
                ConstValue::Scalar(scalar) => {
                    use rustc_middle::mir::interpret::Scalar;
                    match scalar {
                        Scalar::Int(si) => {
                            let bits = si.to_bits(si.size());
                            // Interpret the bits according to the Ty.
                            match ty.kind() {
                                TyKind::Float(rustc_middle::ty::FloatTy::F32) => {
                                    let v = f32::from_bits(bits as u32);
                                    format!("{v}f")
                                }
                                TyKind::Float(rustc_middle::ty::FloatTy::F64) => {
                                    let v = f64::from_bits(bits as u64);
                                    format!("{v}")
                                }
                                TyKind::Bool => {
                                    if bits == 0 { "false".into() } else { "true".into() }
                                }
                                TyKind::Char => {
                                    let ch = char::from_u32(bits as u32)
                                        .map(|c| format!("'{c}'"))
                                        .unwrap_or_else(|| format!("(char){bits}"));
                                    ch
                                }
                                // Signed integers: reinterpret bit pattern as signed.
                                TyKind::Int(k) => {
                                    use rustc_middle::ty::IntTy;
                                    let signed: i128 = match k {
                                        IntTy::I8    => (bits as i8)   as i128,
                                        IntTy::I16   => (bits as i16)  as i128,
                                        IntTy::I32   => (bits as i32)  as i128,
                                        IntTy::I64   => (bits as i64)  as i128,
                                        IntTy::I128  => bits as i128,
                                        IntTy::Isize => (bits as i64)  as i128, // assume 64-bit
                                    };
                                    // Use cast to avoid C# out-of-range literal warnings.
                                    let cs_ty = crate::codegen::types::int_ty_cs(k);
                                    if signed < i32::MIN as i128 || signed > i32::MAX as i128 {
                                        format!("({cs_ty}){signed}L")
                                    } else {
                                        format!("{signed}")
                                    }
                                }
                                _ => format!("{bits}"),
                            }
                        }
                        Scalar::Ptr(_, _) => "/* ptr const */default".into(),
                    }
                }
                ConstValue::ZeroSized => {
                    // Function items are zero-sized values identified by their type.
                    use rustc_middle::ty::TyKind;
                    match ty.kind() {
                        TyKind::FnDef(def_id, substs) => {
                            // Resolve the concrete instance (handles trait method dispatch).
                            let instance = rustc_middle::ty::Instance::try_resolve(
                                tcx,
                                rustc_middle::ty::TypingEnv::fully_monomorphized(),
                                *def_id,
                                substs,
                            );
                            match instance {
                                Ok(Some(inst)) => {
                                    crate::codegen::types::fn_instance_to_cs_path(tcx, &inst)
                                }
                                _ => {
                                    crate::codegen::types::def_id_to_cs_path(tcx, *def_id)
                                }
                            }
                        }
                        _ => "default".into(),
                    }
                }
                _ => "/* const */default".into(),
            }
        }
        Const::Unevaluated(unevaluated, ty) => {
            // Try to evaluate the constant at compile time.
            let result = tcx.const_eval_resolve(
                rustc_middle::ty::TypingEnv::fully_monomorphized(),
                *unevaluated,
                rustc_span::DUMMY_SP,
            );
            match result {
                Ok(val) => {
                    // Re-use the value branch by constructing a temporary Const::Val.
                    let temp = rustc_middle::mir::Const::Val(val, *ty);
                    const_cs(tcx, &temp)
                }
                Err(_) => {
                    // Fall back to the definition path.
                    crate::codegen::types::def_id_to_cs_path(tcx, unevaluated.def)
                }
            }
        }
    }
}
