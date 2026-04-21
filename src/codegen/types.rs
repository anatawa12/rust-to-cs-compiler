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

    for (i, seg) in data.iter().enumerate() {
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
            DefPathData::Impl | DefPathData::CrateRoot => {
                // Impl blocks and the crate root don't add segments.
            }
            _ => {}
        }
    }

    format!("global::{}", segments.join("."))
}

// ── primitive helpers ─────────────────────────────────────────────────────

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
