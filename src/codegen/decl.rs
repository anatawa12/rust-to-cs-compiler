/// Generates C# type declarations from Rust HIR types.
use hir::{
    Adt, AssocItem, DefWithBody, Enum, GenericDef, HasName, HasSource, Impl, Struct, Trait,
    db::HirDatabase,
};
use hir_def::{DefWithBodyId, expr_store::Body};
use syntax::ast::HasAttrs as AstHasAttrs;

use super::{expr::BodyGen, names, output::Output, ty};

/// Returns true if the item carries `#[r2cs_native]` or `#[r2cs::native]`.
/// Checks the raw source AST so that `#[cfg_attr(r2cs, r2cs_native)]` —
/// which rust-analyzer resolves to `#[r2cs_native]` when cfg(r2cs) is active —
/// is detected correctly.
pub fn is_adt_r2cs_native(adt: Adt, db: &dyn HirDatabase) -> bool {
    match adt {
        Adt::Struct(s) => s.source(db).map_or(false, |src| ast_has_r2cs_native(&src.value)),
        Adt::Enum(e) => e.source(db).map_or(false, |src| ast_has_r2cs_native(&src.value)),
        Adt::Union(u) => u.source(db).map_or(false, |src| ast_has_r2cs_native(&src.value)),
    }
}

pub fn is_fn_r2cs_native(f: hir::Function, db: &dyn HirDatabase) -> bool {
    f.source(db).map_or(false, |src| ast_has_r2cs_native(&src.value))
}

fn ast_has_r2cs_native(node: &impl AstHasAttrs) -> bool {
    node.attrs().any(|attr| {
        attr.path().map_or(false, |path| {
            let segs: Vec<_> = path.segments().collect();
            match segs.len() {
                1 => segs[0].name_ref().map_or(false, |n| n.text() == "r2cs_native"),
                2 => {
                    segs[0].name_ref().map_or(false, |n| n.text() == "r2cs")
                        && segs[1].name_ref().map_or(false, |n| n.text() == "native")
                }
                _ => false,
            }
        })
    })
}

/// Emit a struct → C# class.
pub fn emit_struct(out: &mut Output, s: Struct, db: &dyn HirDatabase) {
    let name = s.name(db);
    let cs_name = names::struct_name(name.as_str());
    let gen_params = GenericDef::from(s).params(db);
    let (tp_names, _) = ty::generic_params_cs(&gen_params, db);
    let generics = format_generics(&tp_names);

    out.writeln(&format!("public class {}{}", cs_name, generics));
    out.open_brace();

    for field in s.fields(db) {
        let f_name = names::field_name(field.name(db).as_str());
        let f_ty_ns = field.ty(db);
        let f_ty = f_ty_ns.to_type(db);
        let cs_ty = ty::rust_type_to_cs(&f_ty, db);
        out.writeln(&format!("public r2CsRuntime.Slot<{}> {};", cs_ty, f_name));
    }

    out.close_brace();
    out.blank_line();
}

/// Emit an enum → C# sealed class hierarchy.
pub fn emit_enum(out: &mut Output, e: Enum, db: &dyn HirDatabase) {
    let name = e.name(db);
    let cs_name = names::struct_name(name.as_str());
    let gen_params = GenericDef::from(e).params(db);
    let (tp_names, _) = ty::generic_params_cs(&gen_params, db);
    let generics = format_generics(&tp_names);

    out.writeln(&format!("public class {}{}", cs_name, generics));
    out.open_brace();
    out.writeln(&format!("private {}() {{}}", cs_name));
    out.blank_line();

    for variant in e.variants(db) {
        let v_name = names::variant_name(variant.name(db).as_str());
        let fields = variant.fields(db);

        out.writeln(&format!("public class {} : {}{}", v_name, cs_name, generics));
        out.open_brace();
        for field in &fields {
            let f_ty_ns = field.ty(db);
            let f_ty = f_ty_ns.to_type(db);
            let cs_ty = ty::rust_type_to_cs(&f_ty, db);
            let f_name = names::field_name(field.name(db).as_str());
            out.writeln(&format!("public r2CsRuntime.Slot<{}> {};", cs_ty, f_name));
        }
        out.close_brace();
        out.blank_line();
    }

    out.close_brace();
    out.blank_line();
}

/// Emit a trait → C# interface (non-dyn form, with Self F-bound).
pub fn emit_trait(out: &mut Output, t: Trait, db: &dyn HirDatabase) {
    let name = t.name(db);
    let cs_iface = names::trait_name(name.as_str());
    let cs_dyn_iface = names::dyn_trait_name(name.as_str());

    let gen_params = GenericDef::from(t).params(db);
    let (tp_names, _) = ty::generic_params_cs(&gen_params, db);

    // Build generic list with "Self" first
    let mut all_params = vec!["Self".to_string()];
    all_params.extend(tp_names.iter().cloned());
    let generics = format!("<{}>", all_params.join(", "));

    // F-bounded self constraint
    let self_iface_args = if tp_names.is_empty() {
        "Self".to_string()
    } else {
        format!("Self, {}", tp_names.join(", "))
    };
    let self_bound = format!("Self : {}<{}>", cs_iface, self_iface_args);

    out.writeln(&format!(
        "public interface {}{} where {}",
        cs_iface, generics, self_bound
    ));
    out.open_brace();

    for item in t.items(db) {
        match item {
            AssocItem::Function(f) => {
                emit_trait_method_sig(out, f, db);
            }
            AssocItem::Const(c) => {
                if let Some(cn) = c.name(db) {
                    let c_name = names::method_name(cn.as_str());
                    let c_ty = ty::rust_type_to_cs(&c.ty(db), db);
                    out.writeln(&format!("{} {}(); // const", c_ty, c_name));
                }
            }
            AssocItem::TypeAlias(a) => {
                let a_name = names::assoc_type_param(a.name(db).as_str());
                out.writeln(&format!("// type {};", a_name));
            }
        }
    }

    out.close_brace();
    out.blank_line();

    // Dyn-compatible interface (marker for dyn Trait usage)
    out.writeln(&format!("public interface {} {{", cs_dyn_iface));
    out.writeln("    // dyn-compatible marker");
    out.writeln("}");
    out.blank_line();
}

fn emit_trait_method_sig(out: &mut Output, f: hir::Function, db: &dyn HirDatabase) {
    let is_async = f.is_async(db);
    let ret_ty = f.ret_type(db);
    let cs_ret = cs_ret_type(is_async, &ret_ty, db);
    let m_name = names::method_name(f.name(db).as_str());
    let params = build_param_list(f, db);
    out.writeln(&format!("{} {}({});", cs_ret, m_name, params));
}

/// Emit methods from an impl block onto the appropriate class.
pub fn emit_impl(out: &mut Output, impl_: Impl, db: &dyn HirDatabase) {
    // Determine self type name for the class
    let self_ty = impl_.self_ty(db);
    let cs_self_ty = ty::rust_type_to_cs(&self_ty, db);

    // Comment header
    out.writeln(&format!("// impl for {}", cs_self_ty));
    out.blank_line();

    for item in impl_.items(db) {
        match item {
            AssocItem::Function(f) => {
                emit_function(out, f, db, Some(impl_));
            }
            AssocItem::Const(_) | AssocItem::TypeAlias(_) => {}
        }
    }
}

/// Emit a function/method with a stub body (TODO: real body generation).
pub fn emit_function(
    out: &mut Output,
    f: hir::Function,
    db: &dyn HirDatabase,
    impl_ctx: Option<Impl>,
) {
    if is_fn_r2cs_native(f, db) {
        let m_name = names::method_name(f.name(db).as_str());
        out.writeln(&format!("// [r2cs_native] {} — add implementation in r2CsNative/", m_name));
        return;
    }

    let is_async = f.is_async(db);
    let ret_ty = f.ret_type(db);
    let cs_ret = cs_ret_type(is_async, &ret_ty, db);
    let async_kw = if is_async { "async " } else { "" };
    let m_name = names::method_name(f.name(db).as_str());
    let has_self = f.has_self_param(db);
    let is_static_kw = if !has_self { "static " } else { "" };
    let params = build_param_list(f, db);

    let gen_params = GenericDef::from(f).params(db);
    let (tp_names, _) = ty::generic_params_cs(&gen_params, db);
    let generics = format_generics(&tp_names);

    // Determine the class this method belongs to
    let cs_self = impl_ctx
        .map(|i| ty::rust_type_to_cs(&i.self_ty(db), db))
        .unwrap_or_else(|| "/* top-level */".to_string());

    out.writeln(&format!("// method on {}", cs_self));
    out.writeln(&format!(
        "public {}{}{}{} {}({}) {{",
        is_static_kw, async_kw, cs_ret, generics, m_name, params
    ));
    out.indent();

    // Try to generate a real body using BodyGen
    let def_with_body = DefWithBody::Function(f);
    if let Ok(id) = DefWithBodyId::try_from(def_with_body) {
        let body = Body::of(db, id);
        let mut body_gen = BodyGen::new(db, body, is_async);
        body_gen.emit_body(out);
    } else {
        out.writeln("throw new System.NotImplementedException(\"builtin-derive\");");
    }

    out.dedent();
    out.writeln("}");
    out.blank_line();
}

fn cs_ret_type(is_async: bool, ret_ty: &hir::Type<'_>, db: &dyn HirDatabase) -> String {
    if is_async {
        let t = ty::rust_type_to_cs(ret_ty, db);
        if t == "void" {
            "r2CsRuntime.RustTask<int>".to_string()
        } else {
            format!("r2CsRuntime.RustTask<{}>", t)
        }
    } else if ret_ty.is_unit() {
        "void".to_string()
    } else {
        ty::rust_type_to_cs(ret_ty, db)
    }
}

fn build_param_list(f: hir::Function, db: &dyn HirDatabase) -> String {
    let params = f.params_without_self(db);
    params
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let cs_ty = ty::rust_type_to_cs(param.ty(), db);
            let p_name = param
                .name(db)
                .map(|n| names::local_name(n.as_str(), 0))
                .unwrap_or_else(|| format!("p_{}", i));
            format!("{} {}", cs_ty, p_name)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_generics(tp_names: &[String]) -> String {
    if tp_names.is_empty() {
        String::new()
    } else {
        format!("<{}>", tp_names.join(", "))
    }
}
