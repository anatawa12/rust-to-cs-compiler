/// Rust `Ty<'tcx>` → C# type-string mapping.
///
/// All generated C# type names are fully qualified (prefixed with
/// `global::`) so they are unambiguous regardless of the C# namespace
/// context in which generated code is placed.

use rustc_middle::ty::{self, Ty, TyKind};
use rustc_middle::ty::TyCtxt;

/// Returns the fully-qualified C# type name for `ty`.
///
/// Returns `None` for types that the transpiler does not yet support,
/// allowing callers to emit a TODO comment and continue.
pub fn ty_to_cs<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<String> {
    Some(match ty.kind() {
        // ── primitives ────────────────────────────────────────────────────
        TyKind::Bool      => "bool".into(),
        TyKind::Char      => "char".into(),
        TyKind::Int(k)    => int_ty(k),
        TyKind::Uint(k)   => uint_ty(k),
        TyKind::Float(k)  => float_ty(k),
        TyKind::Str       => "string".into(),

        // ── unit / never ─────────────────────────────────────────────────
        TyKind::Tuple(fields) if fields.is_empty() => {
            "global::r2CsRuntime.Void".into()
        }
        TyKind::Never => "global::r2CsRuntime.Void".into(),

        // ── tuples (non-unit) ─────────────────────────────────────────────
        TyKind::Tuple(fields) => {
            let parts: Option<Vec<_>> = fields
                .iter()
                .map(|f| ty_to_cs(tcx, f))
                .collect();
            format!("({})", parts?.join(", "))
        }

        // ── references ───────────────────────────────────────────────────
        TyKind::Ref(_, inner, _) => {
            match inner.kind() {
                // &[T] / &mut [T]
                TyKind::Slice(elem) => {
                    let cs_elem = ty_to_cs(tcx, *elem)?;
                    format!(
                        "global::r2CsRuntime.LenRef<global::r2CsRuntime.Slice<{cs_elem}>>"
                    )
                }
                // &str
                TyKind::Str => "string".into(),
                // &dyn Trait — represented as DynRef<T_Trait>
                TyKind::Dynamic(..) => {
                    // TODO: extract trait name from the predicate list
                    "/* &dyn */ global::r2CsRuntime.DynRef<object>".into()
                }
                _ => {
                    let cs_inner = ty_to_cs(tcx, *inner)?;
                    format!("global::r2CsRuntime.Ref<{cs_inner}>")
                }
            }
        }

        // ── raw pointers ─────────────────────────────────────────────────
        TyKind::RawPtr(inner, _) => {
            match inner.kind() {
                TyKind::Slice(elem) => {
                    let cs_elem = ty_to_cs(tcx, *elem)?;
                    format!(
                        "global::r2CsRuntime.LenPointer<global::r2CsRuntime.Slice<{cs_elem}>>"
                    )
                }
                TyKind::Dynamic(..) => {
                    "/* *dyn */ global::r2CsRuntime.DynPointer<object>".into()
                }
                _ => {
                    let cs_inner = ty_to_cs(tcx, *inner)?;
                    format!("global::r2CsRuntime.Pointer<{cs_inner}>")
                }
            }
        }

        // ── arrays ────────────────────────────────────────────────────────
        TyKind::Array(elem, _len) => {
            let cs_elem = ty_to_cs(tcx, *elem)?;
            // Rust fixed-size arrays → C# arrays (no size encoding at type level).
            format!("{cs_elem}[]")
        }

        // ── slices (bare, e.g. as DST tail) ──────────────────────────────
        TyKind::Slice(elem) => {
            let cs_elem = ty_to_cs(tcx, *elem)?;
            format!("global::r2CsRuntime.Slice<{cs_elem}>")
        }

        // ── ADTs (structs / enums) ────────────────────────────────────────
        TyKind::Adt(adt_def, args) => {
            adt_to_cs(tcx, *adt_def, args)?
        }

        // ── generic parameters ────────────────────────────────────────────
        TyKind::Param(param) => {
            use crate::codegen::naming;
            naming::generic_param_name(param.name.as_str())
        }

        // ── unsupported ───────────────────────────────────────────────────
        _ => return None,
    })
}

/// Maps a named ADT (struct / enum) to its C# name.
fn adt_to_cs<'tcx>(
    tcx: TyCtxt<'tcx>,
    adt_def: ty::AdtDef<'tcx>,
    args: &ty::GenericArgs<'tcx>,
) -> Option<String> {
    let def_id  = adt_def.did();
    let cs_path = def_id_to_cs_path(tcx, def_id);

    // Generic arguments
    if args.is_empty() {
        return Some(cs_path);
    }
    let cs_args: Option<Vec<_>> = args
        .iter()
        .filter_map(|arg| match arg.kind() {
            ty::GenericArgKind::Type(t) => Some(ty_to_cs(tcx, t)),
            _ => None,
        })
        .collect();
    let cs_args_str = cs_args?.join(", ");

    // Check for Option<T> → T? mapping would go here in the future.
    // For now emit the prefixed struct name with generics.
    Some(format!("{cs_path}<{cs_args_str}>"))
}

/// Converts a `DefId` (struct / enum definition) to its fully-qualified C# path,
/// following the naming conventions (module → `mod_`, struct → `s_`).
pub fn def_id_to_cs_path(tcx: TyCtxt<'_>, def_id: rustc_hir::def_id::DefId) -> String {
    use crate::codegen::naming;
    use rustc_hir::definitions::DefPathData;

    // Crate name becomes the root partial-class: `mod_<crate>`.
    let crate_name = tcx.crate_name(def_id.krate);
    let mut segments: Vec<String> = vec![naming::module_name(crate_name.as_str())];

    let path = tcx.def_path(def_id);
    let data = &path.data;

    let mut i = 0;
    while i < data.len() {
        let seg = &data[i];
        let is_last = i + 1 == data.len();
        match &seg.data {
            DefPathData::TypeNs(sym) => {
                let s = sym.as_str();
                if is_last {
                    // Final segment — struct (or enum) name.
                    segments.push(naming::struct_name(s));
                } else {
                    // Intermediate module-like namespace.
                    segments.push(naming::module_name(s));
                }
            }
            DefPathData::ValueNs(sym) => {
                // Free function or constant.
                segments.push(naming::method_name(sym.as_str()));
            }
            DefPathData::MacroNs(sym) => {
                segments.push(sym.to_string());
            }
            DefPathData::Impl => {
                // For inherent/trait impl methods, include the implementing type name.
                // Look up the impl's `self` type to find the struct name.
                if let Some(impl_def_id) = find_impl_def_id(tcx, def_id, i) {
                    let self_ty = tcx.type_of(impl_def_id).skip_binder();
                    use rustc_middle::ty::TyKind;
                    if let TyKind::Adt(adt_def, _) = self_ty.kind() {
                        let adt_path = tcx.def_path(adt_def.did());
                        if let Some(last) = adt_path.data.last() {
                            if let DefPathData::TypeNs(sym) = &last.data {
                                segments.push(naming::struct_name(sym.as_str()));
                            }
                        }
                    }
                }
                // Skip the Impl segment itself.
            }
            DefPathData::CrateRoot => {
                // Already added.
            }
            _ => {}
        }
        i += 1;
    }

    format!("global::{}", segments.join("."))
}

/// Walk the def_id's ancestor chain to find the impl block def_id that corresponds
/// to the `Impl` segment at `impl_seg_idx` in the def path.
fn find_impl_def_id(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::DefId,
    impl_seg_idx: usize,
) -> Option<rustc_hir::def_id::DefId> {
    use rustc_hir::definitions::DefPathData;
    // Walk ancestors of def_id until we find one whose def path has length == impl_seg_idx + 1
    // (i.e. the impl block itself).
    let path = tcx.def_path(def_id);
    if impl_seg_idx >= path.data.len() {
        return None;
    }
    // The def at impl_seg_idx is the Impl block — we need to find its DefId.
    // Reconstruct by walking tcx.parent().
    let mut cur = def_id;
    let target_len = impl_seg_idx + 1; // path length for the impl block
    loop {
        let cur_path = tcx.def_path(cur);
        if cur_path.data.len() == target_len {
            if matches!(cur_path.data.last().map(|s| &s.data), Some(DefPathData::Impl)) {
                return Some(cur);
            }
        }
        if cur_path.data.is_empty() {
            break;
        }
        match tcx.opt_parent(cur) {
            Some(parent) => cur = parent,
            None => break,
        }
    }
    None
}

// ── primitive helpers ─────────────────────────────────────────────────────

/// Converts a resolved function `Instance` to its fully-qualified C# method path.
///
/// Unlike `def_id_to_cs_path`, this version also appends the concrete generic
/// type arguments for the *enclosing type* when the function lives inside an impl.
/// For example `Pair::<int, long>::swap` becomes `global::mod_crate.s_Pair<int, long>.m_swap`.
pub fn fn_instance_to_cs_path<'tcx>(
    tcx: TyCtxt<'tcx>,
    instance: &ty::Instance<'tcx>,
) -> String {
    use crate::codegen::naming;
    use rustc_hir::definitions::DefPathData;

    let def_id = instance.def.def_id();
    let crate_name = tcx.crate_name(def_id.krate);
    let mut segments: Vec<String> = vec![naming::module_name(crate_name.as_str())];

    let path = tcx.def_path(def_id);
    let data = &path.data;

    let mut i = 0;
    while i < data.len() {
        let seg = &data[i];
        let is_last = i + 1 == data.len();
        match &seg.data {
            DefPathData::TypeNs(sym) => {
                let s = sym.as_str();
                if is_last {
                    segments.push(naming::struct_name(s));
                } else {
                    segments.push(naming::module_name(s));
                }
            }
            DefPathData::ValueNs(sym) => {
                segments.push(naming::method_name(sym.as_str()));
            }
            DefPathData::MacroNs(sym) => {
                segments.push(sym.to_string());
            }
            DefPathData::Impl => {
                if let Some(impl_def_id) = find_impl_def_id(tcx, def_id, i) {
                    let self_ty = tcx.type_of(impl_def_id)
                        .instantiate(tcx, instance.args)
                        .skip_normalization();
                    use rustc_middle::ty::TyKind;
                    if let TyKind::Adt(adt_def, adt_args) = self_ty.kind() {
                        let adt_path = tcx.def_path(adt_def.did());
                        if let Some(last) = adt_path.data.last() {
                            if let DefPathData::TypeNs(sym) = &last.data {
                                let base_name = naming::struct_name(sym.as_str());
                                // Add generic args if any.
                                if adt_args.is_empty() {
                                    segments.push(base_name);
                                } else {
                                    let cs_args: Vec<String> = adt_args
                                        .iter()
                                        .filter_map(|arg| match arg.kind() {
                                            ty::GenericArgKind::Type(t) => ty_to_cs(tcx, t),
                                            _ => None,
                                        })
                                        .collect();
                                    if cs_args.is_empty() {
                                        segments.push(base_name);
                                    } else {
                                        segments.push(format!("{base_name}<{}>", cs_args.join(", ")));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DefPathData::CrateRoot => {}
            _ => {}
        }
        i += 1;
    }

    format!("global::{}", segments.join("."))
}

/// Public helper so `mir.rs` can format a signed-int cast in constants.
pub fn int_ty_cs(k: &ty::IntTy) -> String {
    int_ty(k)
}

fn int_ty(k: &ty::IntTy) -> String {
    use ty::IntTy::*;
    match k {
        Isize => "nint".into(),
        I8    => "sbyte".into(),
        I16   => "short".into(),
        I32   => "int".into(),
        I64   => "long".into(),
        I128  => "global::System.Int128".into(),
    }
}

fn uint_ty(k: &ty::UintTy) -> String {
    use ty::UintTy::*;
    match k {
        Usize => "nuint".into(),
        U8    => "byte".into(),
        U16   => "ushort".into(),
        U32   => "uint".into(),
        U64   => "ulong".into(),
        U128  => "global::System.UInt128".into(),
    }
}

fn float_ty(k: &ty::FloatTy) -> String {
    use ty::FloatTy::*;
    match k {
        F16  => "global::System.Half".into(),
        F32  => "float".into(),
        F64  => "double".into(),
        F128 => "global::System.Double".into(), // no direct C# equivalent; map to double
    }
}

#[cfg(test)]
mod tests {
    // Unit-test the primitive helpers directly (no TyCtxt needed).
    use super::*;
    use rustc_middle::ty::{IntTy, UintTy, FloatTy};

    #[test]
    fn int_types_mapped() {
        assert_eq!(int_ty(&IntTy::I32),  "int");
        assert_eq!(int_ty(&IntTy::I64),  "long");
        assert_eq!(int_ty(&IntTy::I8),   "sbyte");
        assert_eq!(int_ty(&IntTy::I128), "global::System.Int128");
        assert_eq!(int_ty(&IntTy::Isize),"nint");
    }

    #[test]
    fn uint_types_mapped() {
        assert_eq!(uint_ty(&UintTy::U32),  "uint");
        assert_eq!(uint_ty(&UintTy::U8),   "byte");
        assert_eq!(uint_ty(&UintTy::U128), "global::System.UInt128");
        assert_eq!(uint_ty(&UintTy::Usize),"nuint");
    }

    #[test]
    fn float_types_mapped() {
        assert_eq!(float_ty(&FloatTy::F32), "float");
        assert_eq!(float_ty(&FloatTy::F64), "double");
    }
}
