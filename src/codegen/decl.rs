use super::{CodeGenerator, expr::BodyGen, names, output::Code};
use cfg::CfgExpr;
/// Generates C# type declarations from Rust HIR types.
use hir::{
    Adt, AssocItem, DefWithBody, Enum, GenericDef, HasAttrs, HasCrate, HasSource, Impl, Struct,
    Trait, db::HirDatabase,
};
use syntax::ast::HasAttrs as AstHasAttrs;

/// Returns true if the item carries `#[r2cs_native]` or `#[r2cs::native]`.
/// Checks the raw source AST so that `#[cfg_attr(r2cs, r2cs_native)]` —
/// which rust-analyzer resolves to `#[r2cs_native]` when cfg(r2cs) is active —
/// is detected correctly.
pub fn is_adt_r2cs_native(adt: Adt, krate: hir::Crate, db: &dyn HirDatabase) -> bool {
    adt.source(db)
        .is_some_and(|src| ast_has_r2cs_native(&src.value, krate, db))
}

pub fn is_fn_r2cs_native(f: hir::Function, krate: hir::Crate, db: &dyn HirDatabase) -> bool {
    f.source(db)
        .is_some_and(|src| ast_has_r2cs_native(&src.value, krate, db))
}

/// Returns true if the item's `#[cfg(...)]` condition evaluates to false under
/// the current crate's CfgOptions (i.e., the item should be excluded).
/// Items with no cfg attribute, or with an undecidable cfg, are included.
pub fn is_adt_cfg_disabled(adt: Adt, db: &dyn HirDatabase) -> bool {
    cfg_disabled(adt.attrs(db), adt.krate(db), db)
}

pub fn is_fn_cfg_disabled(f: hir::Function, db: &dyn HirDatabase) -> bool {
    cfg_disabled(f.attrs(db), f.krate(db), db)
}

pub fn is_impl_cfg_disabled(impl_: Impl, db: &dyn HirDatabase) -> bool {
    cfg_disabled(impl_.attrs(db), impl_.krate(db), db)
}

/// Returns true when this module or any of its ancestor modules carries a `#[cfg(...)]`
/// that evaluates to false.  Items nested inside cfg-gated modules have no cfg attribute
/// of their own, so we must walk the full parent chain.
pub fn is_module_cfg_disabled(module: hir::Module, db: &dyn HirDatabase) -> bool {
    let mut cur = module;
    loop {
        if cfg_disabled(cur.attrs(db), cur.krate(db), db) {
            return true;
        }
        match cur.parent(db) {
            Some(parent) => cur = parent,
            None => return false,
        }
    }
}

fn cfg_disabled(attrs: hir::AttrsWithOwner, krate: hir::Crate, db: &dyn HirDatabase) -> bool {
    match attrs.cfgs(db) {
        Some(cfg_expr) => krate.cfg(db).check(cfg_expr) == Some(false),
        None => false,
    }
}

fn ast_has_r2cs_native(node: &impl AstHasAttrs, krate: hir::Crate, db: &dyn HirDatabase) -> bool {
    use syntax::ast::Meta;
    node.attrs().any(|attr| {
        let attrs = match attr.meta() {
            Some(Meta::CfgAttrMeta(cfg))
                if let Some(cfg_predicate) = cfg.cfg_predicate()
                    && krate.cfg(db).check(&CfgExpr::parse_from_ast(cfg_predicate))
                        == Some(true) =>
            {
                cfg.metas().collect::<Vec<_>>()
            }
            Some(meta) => vec![meta],
            None => vec![],
        };

        attrs.iter().any(|attr| {
            attr.path().is_some_and(|path| {
                let segs: Vec<_> = path.segments().collect();
                match segs.len() {
                    1 => segs[0]
                        .name_ref()
                        .is_some_and(|n| n.text() == "r2cs_native"),
                    2 => {
                        segs[0].name_ref().is_some_and(|n| n.text() == "r2cs")
                            && segs[1].name_ref().is_some_and(|n| n.text() == "native")
                    }
                    _ => false,
                }
            })
        })
    })
}

impl<'db> CodeGenerator<'db> {
    /// Emit a struct → C# class.
    pub fn emit_struct(&self, out: &mut Code, s: Struct) {
        let db = self.db;

        let name = s.name(db);
        let cs_name = names::struct_name(name.as_str());
        let gen_params = GenericDef::from(s).params(db);
        let (tp_names, _) = self.generic_params_cs(&gen_params, db);
        let generics = self.format_generics(&tp_names);

        out.wln(&format!("public class {}{}", cs_name, generics));
        out.open_brace();

        for field in s.fields(db) {
            let f_name = names::field_name(field.name(db).as_str());
            let f_ty_ns = field.ty(db);
            let f_ty = f_ty_ns.to_type(db);
            let cs_ty = self.rust_type_to_cs(&f_ty);
            out.wln(&format!("public r2CsRuntime.Slot<{}> {};", cs_ty, f_name));
        }

        out.close_brace();
        out.blank_line();
    }

    /// Emit an enum → C# sealed class hierarchy.
    pub fn emit_enum(&self, out: &mut Code, e: Enum) {
        let db = self.db;

        let name = e.name(db);
        let cs_name = names::struct_name(name.as_str());
        let gen_params = GenericDef::from(e).params(db);
        let (tp_names, _) = self.generic_params_cs(&gen_params, db);
        let generics = self.format_generics(&tp_names);

        out.wln(format!("public class {}{}", cs_name, generics));
        out.open_brace();
        out.wln(format!("private {}() {{}}", cs_name));
        out.blank_line();

        for variant in e.variants(db) {
            let v_name = names::variant_name(variant.name(db).as_str());
            let fields = variant.fields(db);

            out.wln(format!("public class {} : {}{}", v_name, cs_name, generics));
            out.open_brace();
            for field in &fields {
                let f_ty_ns = field.ty(db);
                let f_ty = f_ty_ns.to_type(db);
                let cs_ty = self.rust_type_to_cs(&f_ty);
                let f_name = names::field_name(field.name(db).as_str());
                out.wln(format!("public r2CsRuntime.Slot<{}> {};", cs_ty, f_name));
            }
            out.close_brace();
            out.blank_line();
        }

        out.close_brace();
        out.blank_line();
    }

    /// Emit a trait → C# interface (non-dyn form, with Self F-bound).
    pub fn emit_trait(&self, out: &mut Code, t: Trait) {
        let db = self.db;

        let name = t.name(db);
        let cs_iface = names::trait_name(name.as_str());
        let cs_dyn_iface = names::dyn_trait_name(name.as_str());

        let gen_params = GenericDef::from(t).params(db);
        let (tp_names, _) = self.generic_params_cs(&gen_params, db);

        // Build generic list with "Self" first
        let mut all_params = vec![];
        all_params.extend(tp_names.iter().cloned());
        let generics = format!("<{}>", all_params.join(", "));

        // F-bounded self constraint
        let self_iface_args = tp_names.join(", ");
        let self_bound = format!("P_Self : {}<{}>", cs_iface, self_iface_args);

        writeln!(
            out,
            "public interface {cs_iface}{generics} where {self_bound}"
        );
        out.open_brace();

        for item in t.items(db) {
            match item {
                AssocItem::Function(f) => {
                    self.emit_trait_method_sig(out, f);
                }
                AssocItem::Const(c) => {
                    if let Some(cn) = c.name(db) {
                        let c_name = names::method_name(cn.as_str());
                        let c_ty = self.rust_type_to_cs(&c.ty(db));
                        out.wln(&format!("{} {}(); // const", c_ty, c_name));
                    }
                }
                AssocItem::TypeAlias(a) => {
                    let a_name = names::assoc_type_param(a.name(db).as_str());
                    out.wln(&format!("// type {};", a_name));
                }
            }
        }

        out.close_brace();
        out.blank_line();

        // Dyn-compatible interface (marker for dyn Trait usage)
        out.wln(&format!("public interface {} {{", cs_dyn_iface));
        out.wln("    // dyn-compatible marker");
        out.wln("}");
        out.blank_line();
    }

    fn emit_trait_method_sig(&self, out: &mut Code, f: hir::Function) {
        let db = self.db;

        let is_async = f.is_async(db);
        let ret_ty = f.ret_type(db);
        let cs_ret = self.cs_ret_type(is_async, &ret_ty);
        let m_name = names::method_name(f.name(db).as_str());
        let params = self.build_param_list(f);
        out.wln(&format!("{} {}({});", cs_ret, m_name, params));
    }

    /// Emit methods from an impl block onto the appropriate class.
    pub fn emit_impl(&self, out: &mut Code, impl_: Impl) {
        let db = self.db;

        // Determine self type name for the class
        let self_ty = impl_.self_ty(db);
        let cs_self_ty = self.rust_type_to_cs(&self_ty);

        // Comment header
        out.wln(&format!("// impl for {}", cs_self_ty));
        out.blank_line();

        for item in impl_.items(db) {
            match item {
                AssocItem::Function(f) => {
                    self.emit_function(out, f, Some(impl_));
                }
                AssocItem::Const(_) | AssocItem::TypeAlias(_) => {}
            }
        }
    }

    /// Emit a function/method with a stub body (TODO: real body generation).
    pub fn emit_function(&self, out: &mut Code, f: hir::Function, impl_ctx: Option<Impl>) {
        let db = self.db;

        if is_fn_cfg_disabled(f, db) {
            return;
        }

        let is_async = f.is_async(db);
        let ret_ty = f.async_ret_type(db).unwrap_or(f.ret_type(db));
        let cs_ret = self.cs_ret_type(is_async, &ret_ty);
        let async_kw = if is_async { "async " } else { "" };
        let m_name = names::method_name(f.name(db).as_str());
        let has_self = f.has_self_param(db);
        let is_static_kw = if !has_self { "static " } else { "" };
        let params = self.build_param_list(f);

        let gen_params = GenericDef::from(f).params(db);
        let (tp_names, _) = self.generic_params_cs(&gen_params, db);
        let generics = self.format_generics(&tp_names);

        // Determine the class this method belongs to
        let cs_self = impl_ctx
            .map(|i| self.rust_type_to_cs(&i.self_ty(db)))
            .unwrap_or_else(|| "/* top-level */".to_string());

        if is_fn_r2cs_native(f, self.krate, db) {
            writeln!(
                out,
                "// [r2cs_native] {m_name} — add implementation in r2CsNative/",
            );
            writeln!(
                out,
                "public {is_static_kw} partial {cs_ret} {m_name}{generics}({params});",
            );
            return;
        }
        writeln!(out, "// method on {}", cs_self);
        writeln!(
            out,
            "public {is_static_kw}{async_kw}{cs_ret} {m_name}{generics}({params}) {{",
        );
        out.indent();

        // Try to generate a real body using BodyGen
        let mut body_gen = BodyGen::new(self, is_async);
        body_gen.emit_function_body(self.sem.source(f).unwrap().value, out);

        out.dedent();
        out.wln("}");
        out.blank_line();
    }

    fn cs_ret_type(&self, is_async: bool, ret_ty: &hir::Type<'db>) -> String {
        if is_async {
            format!("r2CsRuntime.RustTask<{}>", self.rust_type_to_cs(&ret_ty))
        } else if ret_ty.is_unit() {
            "void".to_string()
        } else {
            self.rust_type_to_cs(ret_ty)
        }
    }

    fn build_param_list(&self, f: hir::Function) -> String {
        let db = self.db;

        let params = f.params_without_self(db);
        params
            .iter()
            .enumerate()
            .map(|(i, param)| {
                let cs_ty = self.rust_type_to_cs(param.ty());
                let p_name = param
                    .name(db)
                    .map(|n| names::local_name(n.as_str(), 0))
                    .unwrap_or_else(|| format!("p_{}", i));
                format!("{} {}", cs_ty, p_name)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn format_generics(&self, tp_names: &[String]) -> String {
        if tp_names.is_empty() {
            String::new()
        } else {
            format!("<{}>", tp_names.join(", "))
        }
    }
}
