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

    w.write_line(&format!("public struct {cs_name}"));
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

    w.write_line(&format!("public struct {cs_name}"));
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

    // Emit free items (fns, structs, enums, …).
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
                // Skip trait impls for now; only handle inherent impls.
                if impl_block.of_trait.is_some() {
                    continue;
                }
                w.write_line("");
                compile_impl(tcx, item_id.owner_id.def_id, &impl_block, w);
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
    // Determine the C# class name from the self type.
    let self_ty_name = {
        use rustc_hir::TyKind;
        match impl_block.self_ty.kind {
            TyKind::Path(rustc_hir::QPath::Resolved(_, path)) => {
                if let Some(def_id) = path.res.opt_def_id() {
                    crate::codegen::types::def_id_to_cs_path(tcx, def_id)
                        .split('.')
                        .last()
                        .unwrap_or("Impl")
                        .to_string()
                } else {
                    "Impl".to_string()
                }
            }
            _ => "Impl".to_string(),
        }
    };

    w.write_line(&format!("public partial struct {self_ty_name}"));
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
