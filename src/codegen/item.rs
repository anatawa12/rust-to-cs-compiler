/// Top-level item (function, struct) code generation.
///
/// # THIR vs MIR for function bodies
///
/// THIR is the ideal representation because it preserves async/await structure,
/// match expressions, and other high-level constructs.  However, THIR is
/// "stolen" (consumed) by the borrow checker during the `analysis` phase, which
/// runs before `rustc_driver::Callbacks::after_analysis` is called.  Attempting
/// to call `tcx.thir_body(def_id)` after analysis panics with "attempted to
/// read from stolen value".
///
/// We therefore use **MIR** for function bodies.  See `docs/mir-vs-thir.md` for
/// a detailed comparison and the implications for async support.
///
/// THIR is still used in `compile_fn` for one small purpose: extracting
/// parameter names (which MIR does not preserve for named parameters).
/// `thir_body` must be called before analysis steals it, but since we only use
/// it here for names, the approach that works is to get param names from
/// `body.var_debug_info` in MIR instead (which does survive analysis).

use rustc_hir::ItemKind;
use rustc_middle::ty::TyCtxt;

use crate::codegen::mir::compile_mir_body;
use crate::codegen::naming;
use crate::codegen::types::ty_to_cs;
use crate::codegen::writer::CsWriter;

/// Compile a single free function item using its MIR body.
pub fn compile_fn(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    fn_name: &str,
    w: &mut CsWriter,
) {
    let cs_name = naming::method_name(fn_name);

    // Get the optimized MIR body (always available after analysis).
    let body = tcx.optimized_mir(def_id.to_def_id());

    // Build parameter list from MIR local_decls + fn_sig.
    // _0 is the return slot; _1.._arg_count are the parameters.
    // We use `_N` names in the signature to match what the MIR body uses.
    // VarDebugInfo names are emitted as comments for readability.
    let fn_sig = tcx.fn_sig(def_id).skip_binder().skip_binder();
    let param_tys = fn_sig.inputs();
    let ret_ty = fn_sig.output();

    let ret_str = ty_to_cs(tcx, ret_ty)
        .unwrap_or_else(|| "object /* unknown */".into());

    // Build human-readable name map from VarDebugInfo.
    let debug_names: std::collections::HashMap<usize, String> = body
        .var_debug_info
        .iter()
        .filter_map(|dbg| {
            if let rustc_middle::mir::VarDebugInfoContents::Place(p) = &dbg.value {
                if p.projection.is_empty() {
                    let idx = p.local.index();
                    if idx >= 1 && idx <= body.arg_count {
                        return Some((idx, dbg.name.to_string()));
                    }
                }
            }
            None
        })
        .collect();

    let params: Vec<String> = param_tys
        .iter()
        .enumerate()
        .map(|(i, &ty)| {
            let ty_str = ty_to_cs(tcx, ty)
                .unwrap_or_else(|| "object /* unknown */".into());
            let local_idx = i + 1;
            // Use `_N` to match what MIR body emits; add a comment with the source name.
            if let Some(src_name) = debug_names.get(&local_idx) {
                format!("{ty_str} _{local_idx} /* {src_name} */")
            } else {
                format!("{ty_str} _{local_idx}")
            }
        })
        .collect();

    w.write_line(&format!(
        "public static {ret_str} {cs_name}({})",
        params.join(", ")
    ));
    w.write_line("{");
    w.indent();

    compile_mir_body(tcx, body, w);

    w.dedent();
    w.write_line("}");
}

/// Compile a struct definition.
pub fn compile_struct(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    struct_name: &str,
    w: &mut CsWriter,
) {
    let cs_name = naming::struct_name(struct_name);
    let adt_def = tcx.adt_def(def_id);
    let variant = adt_def.non_enum_variant();
    let generics_str = cs_generic_params(tcx, def_id.to_def_id());

    // Always emit `partial` so that an `impl` block extension can be added later.
    w.write_line(&format!("public partial struct {cs_name}{generics_str}"));
    w.write_line("{");
    w.indent();

    for field in variant.fields.iter() {
        let field_ty = tcx.type_of(field.did).skip_binder();
        let ty_str   = ty_to_cs(tcx, field_ty)
            .unwrap_or_else(|| "object /* unknown */".into());
        let f_name   = naming::field_name(field.name.as_str());
        w.write_line(&format!("public {ty_str} {f_name};"));
    }

    w.dedent();
    w.write_line("}");
}

/// Compile an enum definition.
///
/// Strategy: an enum is represented as a tagged union (a C# struct containing
/// a discriminant field plus one payload field per variant that carries data).
///
/// ```csharp
/// public struct s_MyEnum {
///     public byte f_discriminant;
///     // variant payloads (only one active at a time)
///     public s_MyEnum_Variant_A f_variant_A;
///     public s_MyEnum_Variant_B f_variant_B;
///
///     // Variant payload structs:
///     public struct s_MyEnum_Variant_A { public int f_field0; }
/// }
/// ```
pub fn compile_enum(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    enum_name: &str,
    w: &mut CsWriter,
) {
    let cs_name = naming::struct_name(enum_name);
    let adt_def = tcx.adt_def(def_id);
    let generics_str = cs_generic_params(tcx, def_id.to_def_id());

    w.write_line(&format!("public partial struct {cs_name}{generics_str}"));
    w.write_line("{");
    w.indent();

    // Discriminant field.
    w.write_line("public byte f_discriminant;");

    // One payload struct + field per variant that has fields.
    for variant in adt_def.variants().iter() {
        let variant_name = variant.name.as_str();
        let payload_ty   = format!("{cs_name}_{variant_name}");
        let field_name   = format!("f_{variant_name}");

        if variant.fields.is_empty() {
            // Unit variant — no payload.
            continue;
        }

        w.write_line(&format!("public {payload_ty} {field_name};"));
    }

    // Emit payload structs for variants with fields.
    for variant in adt_def.variants().iter() {
        if variant.fields.is_empty() {
            continue;
        }
        let variant_name = variant.name.as_str();
        let payload_ty   = format!("{cs_name}_{variant_name}");

        w.write_line("");
        w.write_line(&format!("public struct {payload_ty}"));
        w.write_line("{");
        w.indent();
        for field in variant.fields.iter() {
            let field_ty = tcx.type_of(field.did).skip_binder();
            let ty_str   = ty_to_cs(tcx, field_ty)
                .unwrap_or_else(|| "object /* unknown */".into());
            let f_name   = naming::field_name(field.name.as_str());
            w.write_line(&format!("public {ty_str} {f_name};"));
        }
        w.dedent();
        w.write_line("}");
    }

    // Emit discriminant constants.
    w.write_line("");
    for (idx, variant) in adt_def.variants().iter().enumerate() {
        let variant_name = variant.name.as_str();
        w.write_line(&format!("public const byte k_{variant_name} = {idx};"));
    }

    w.dedent();
    w.write_line("}");
}

/// Compile all local free items in the crate, wrapped in a `mod_<crate>` class.
pub fn compile_crate(tcx: TyCtxt<'_>, w: &mut CsWriter) {
    let crate_name = tcx.crate_name(rustc_hir::def_id::LOCAL_CRATE);
    let class_name = naming::module_name(crate_name.as_str());

    w.write_line("// <auto-generated/>");
    w.write_line("#nullable enable");
    w.write_line("");
    w.write_line(&format!("public static partial class {class_name}"));
    w.write_line("{");
    w.indent();

    // Recursively compile the crate root module.
    compile_module(tcx, rustc_hir::def_id::LocalModDefId::CRATE_DEF_ID, w);

    w.dedent();
    w.write_line("}");
}

/// Recursively compile the items in a single HIR module into a C# static class.
fn compile_module(tcx: TyCtxt<'_>, module_id: rustc_hir::def_id::LocalModDefId, w: &mut CsWriter) {
    let mod_items = tcx.hir_module_items(module_id);

    // Emit free items (fns, structs, enums, consts, statics, …).
    for item_id in mod_items.free_items() {
        let item = tcx.hir_item(item_id);
        match item.kind {
            ItemKind::Fn { ident, .. } => {
                w.write_line("");
                compile_fn(tcx, item_id.owner_id.def_id, ident.name.as_str(), w);
            }
            ItemKind::Struct(ident, _, _) => {
                w.write_line("");
                compile_struct(tcx, item_id.owner_id.def_id, ident.name.as_str(), w);
            }
            ItemKind::Enum(ident, _, _) => {
                w.write_line("");
                compile_enum(tcx, item_id.owner_id.def_id, ident.name.as_str(), w);
            }
            ItemKind::Mod(ident, _) => {
                w.write_line("");
                let mod_name = naming::module_name(ident.name.as_str());
                w.write_line(&format!("public static partial class {mod_name}"));
                w.write_line("{");
                w.indent();
                // Recurse into the sub-module.
                let sub_mod_id = rustc_hir::def_id::LocalModDefId::new_unchecked(
                    item_id.owner_id.def_id
                );
                compile_module(tcx, sub_mod_id, w);
                w.dedent();
                w.write_line("}");
            }
            ItemKind::Impl(impl_block) => {
                w.write_line("");
                compile_impl(tcx, item_id.owner_id.def_id, &impl_block, w);
            }
            ItemKind::Const(ident, _, _, _) => {
                w.write_line("");
                compile_const(tcx, item_id.owner_id.def_id, ident.name.as_str(), w);
            }
            ItemKind::Static(_, ident, _, _) => {
                w.write_line("");
                compile_static(tcx, item_id.owner_id.def_id, ident.name.as_str(), w);
            }
            ItemKind::TyAlias(ident, _, _) => {
                w.write_line("");
                compile_type_alias(tcx, item_id.owner_id.def_id, ident.name.as_str(), w);
            }
            // use / extern crate — no C# equivalent needed.
            ItemKind::Use(..) | ItemKind::ExternCrate(..) => {}
            // Trait definitions — emit as a comment for now.
            ItemKind::Trait(_, _, _, _, ident, _, _, _) => {
                w.write_line(&format!("// trait {}", ident.name.as_str()));
            }
            _ => {}
        }
    }
}

/// Compile an inherent `impl` block.
///
/// Methods are emitted as `static` functions inside the type's `partial struct`.
fn compile_impl<'hir>(
    tcx: TyCtxt<'_>,
    _impl_def_id: rustc_hir::def_id::LocalDefId,
    impl_block: &rustc_hir::Impl<'hir>,
    w: &mut CsWriter,
) {
    // Determine the C# class name from the self type, including generic params.
    let (self_ty_name, generics_str) = {
        use rustc_hir::TyKind;
        match impl_block.self_ty.kind {
            TyKind::Path(rustc_hir::QPath::Resolved(_, path)) => {
                if let Some(def_id) = path.res.opt_def_id() {
                    let base = crate::codegen::types::def_id_to_cs_path(tcx, def_id)
                        .split('.')
                        .last()
                        .unwrap_or("Impl")
                        .to_string();
                    let gparams = cs_generic_params(tcx, def_id);
                    (base, gparams)
                } else {
                    ("Impl".to_string(), String::new())
                }
            }
            _ => ("Impl".to_string(), String::new()),
        }
    };

    w.write_line(&format!("public partial struct {self_ty_name}{generics_str}"));
    w.write_line("{");
    w.indent();

    for impl_item_id in impl_block.items {
        let impl_item = tcx.hir_impl_item(*impl_item_id);
        if let rustc_hir::ImplItemKind::Fn(_, _) = impl_item.kind {
            let method_name = impl_item.ident.name.as_str();
            w.write_line("");
            compile_method(tcx, impl_item_id.owner_id.def_id, method_name, w);
        }
    }

    w.dedent();
    w.write_line("}");
}

/// Compile a single method from an inherent impl.
fn compile_method(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    method_name: &str,
    w: &mut CsWriter,
) {
    let cs_name = naming::method_name(method_name);

    let body = tcx.optimized_mir(def_id.to_def_id());
    let fn_sig = tcx.fn_sig(def_id).skip_binder().skip_binder();
    let param_tys = fn_sig.inputs();
    let ret_ty = fn_sig.output();

    let ret_str = ty_to_cs(tcx, ret_ty)
        .unwrap_or_else(|| "object /* unknown */".into());

    let debug_names: std::collections::HashMap<usize, String> = body
        .var_debug_info
        .iter()
        .filter_map(|dbg| {
            if let rustc_middle::mir::VarDebugInfoContents::Place(p) = &dbg.value {
                if p.projection.is_empty() {
                    let idx = p.local.index();
                    if idx >= 1 && idx <= body.arg_count {
                        return Some((idx, dbg.name.to_string()));
                    }
                }
            }
            None
        })
        .collect();

    let params: Vec<String> = param_tys
        .iter()
        .enumerate()
        .map(|(i, &ty)| {
            let ty_str = ty_to_cs(tcx, ty)
                .unwrap_or_else(|| "object /* unknown */".into());
            let local_idx = i + 1;
            if let Some(src_name) = debug_names.get(&local_idx) {
                format!("{ty_str} _{local_idx} /* {src_name} */")
            } else {
                format!("{ty_str} _{local_idx}")
            }
        })
        .collect();

    w.write_line(&format!(
        "public static {ret_str} {cs_name}({})",
        params.join(", ")
    ));
    w.write_line("{");
    w.indent();
    compile_mir_body(tcx, body, w);
    w.dedent();
    w.write_line("}");
}

/// Compile a `const` item.
///
/// The constant value is evaluated at compile time by rustc; we read it from
/// the MIR `promoted` or via `eval_static_initializer`.  For simplicity we
/// emit a `static readonly` C# field with the value from MIR.
fn compile_const(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    const_name: &str,
    w: &mut CsWriter,
) {
    let cs_name = format!("k_{const_name}");
    let ty = tcx.type_of(def_id).skip_binder();
    let ty_str = ty_to_cs(tcx, ty).unwrap_or_else(|| "object /* unknown */".into());

    // Evaluate the constant and emit its C# representation.
    let val_str = eval_const_to_cs(tcx, def_id.to_def_id());
    w.write_line(&format!("public static readonly {ty_str} {cs_name} = {val_str};"));
}

/// Compile a `static` item.
fn compile_static(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    static_name: &str,
    w: &mut CsWriter,
) {
    let cs_name = format!("s_{static_name}");
    let ty = tcx.type_of(def_id).skip_binder();
    let ty_str = ty_to_cs(tcx, ty).unwrap_or_else(|| "object /* unknown */".into());
    let val_str = eval_const_to_cs(tcx, def_id.to_def_id());
    w.write_line(&format!("public static {ty_str} {cs_name} = {val_str};"));
}

/// Compile a `type` alias.
fn compile_type_alias(
    tcx: TyCtxt<'_>,
    def_id: rustc_hir::def_id::LocalDefId,
    alias_name: &str,
    w: &mut CsWriter,
) {
    // C# does not have a direct equivalent of `type Foo = Bar`.
    // Emit a comment documenting the alias.
    let aliased = tcx.type_of(def_id).skip_binder();
    let aliased_str = ty_to_cs(tcx, aliased).unwrap_or_else(|| "/* unknown */".into());
    w.write_line(&format!("// type alias: {alias_name} = {aliased_str}"));
}

/// Evaluate a constant / static initializer and return a C# literal string.
fn eval_const_to_cs(tcx: TyCtxt<'_>, def_id: rustc_hir::def_id::DefId) -> String {
    // Try to evaluate the constant.
    let instance = rustc_middle::ty::Instance::mono(tcx, def_id);
    let ty = tcx.type_of(def_id).skip_binder();
    let result = tcx.const_eval_instance(
        rustc_middle::ty::TypingEnv::fully_monomorphized(),
        instance,
        rustc_span::DUMMY_SP,
    );

    match result {
        Ok(val) => {
            use rustc_middle::mir::ConstValue;
            use rustc_middle::mir::interpret::{GlobalAlloc, Scalar};
            match val {
                ConstValue::Scalar(scalar) => {
                    match scalar {
                        Scalar::Int(si) => {
                            // Re-use the typed path from mir.rs to get signed/float interpretation.
                            let bits = si.to_bits(si.size());
                            use rustc_middle::ty::TyKind;
                            match ty.kind() {
                                TyKind::Float(rustc_middle::ty::FloatTy::F32) => {
                                    format!("{}f", f32::from_bits(bits as u32))
                                }
                                TyKind::Float(rustc_middle::ty::FloatTy::F64) => {
                                    format!("{}", f64::from_bits(bits as u64))
                                }
                                TyKind::Bool => {
                                    if bits == 0 { "false".into() } else { "true".into() }
                                }
                                TyKind::Int(k) => {
                                    use rustc_middle::ty::IntTy;
                                    let signed: i128 = match k {
                                        IntTy::I8    => (bits as i8)   as i128,
                                        IntTy::I16   => (bits as i16)  as i128,
                                        IntTy::I32   => (bits as i32)  as i128,
                                        IntTy::I64   => (bits as i64)  as i128,
                                        IntTy::I128  => bits as i128,
                                        IntTy::Isize => (bits as i64)  as i128,
                                    };
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
                        Scalar::Ptr(ptr, _) => {
                            match tcx.global_alloc(ptr.provenance.alloc_id()) {
                                GlobalAlloc::Memory(alloc) => {
                                    let bytes = alloc.inner().inspect_with_uninit_and_ptr_outside_interpreter(
                                        0..alloc.inner().len()
                                    );
                                    if let Ok(s) = std::str::from_utf8(bytes) {
                                        return format!("\"{}\"", s.escape_default());
                                    }
                                    "/* ptr */default".into()
                                }
                                _ => "/* ptr */default".into(),
                            }
                        }
                    }
                }
                _ => "default".into(),
            }
        }
        Err(rustc_middle::mir::interpret::ErrorHandled::Reported(_, _))
        | Err(rustc_middle::mir::interpret::ErrorHandled::TooGeneric(_)) => {
            "/* unevaluated */default".into()
        }
    }
}

/// Returns the C# generic parameter list for a definition, e.g. `<T, U>`.
/// Returns an empty string if there are no type parameters.
fn cs_generic_params(tcx: TyCtxt<'_>, def_id: rustc_hir::def_id::DefId) -> String {
    let generics = tcx.generics_of(def_id);
    let type_params: Vec<String> = generics
        .own_params
        .iter()
        .filter_map(|p| {
            use rustc_middle::ty::GenericParamDefKind;
            match p.kind {
                GenericParamDefKind::Type { .. } => {
                    Some(crate::codegen::naming::generic_param_name(p.name.as_str()))
                }
                _ => None,
            }
        })
        .collect();
    if type_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", type_params.join(", "))
    }
}
