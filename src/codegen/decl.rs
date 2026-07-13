use super::{CodeGenerator, expr::BodyGen, names, output::Code};
use crate::codegen::debug_display::DebugDisplay;
use crate::codegen::expr::ItemInBody;
use crate::codegen::ty::TraitExt;
use cfg::CfgExpr;
/// Generates C# type declarations from Rust HIR types.
use hir::{
    Adt, AssocItem, GenericDef, HasAttrs, HasContainer, HasCrate, HasSource, Impl, ItemContainer,
    Trait, db::HirDatabase,
};
use std::collections::HashMap;
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
    /// Emit a trait → C# interface (non-dyn form, with Self F-bound).
    pub fn emit_trait(&self, out: &mut Code, t: Trait) {
        let db = self.db;

        let name = t.name(db);
        let cs_iface = names::trait_name(name.as_str());
        let _cs_dyn_iface = names::dyn_trait_name(name.as_str());

        let gen_params = GenericDef::from(t).params(db);
        let (mut all_params, mut constraints) = self.generic_params_cs(&gen_params);

        if let Some(compatibility) = t.dyn_compatibility_all_violations(db) {
            for x in compatibility {
                out.wln(format!("// dyn incompatible with {x:?}"));
            }
        } else {
            out.wln("// dyn compatible");
        }

        for a in t.assoc_types(db) {
            let a_name = names::assoc_type_param(a.name(db).as_str());
            all_params.push(a_name);
        }

        // For non-dyn compatible trait, we insert 'Self' type parameter
        if self.with_self_in_cs(t) {
            all_params.insert(0, "P_Self".into());
            constraints.push(format!("P_Self : {}<{}>", cs_iface, all_params.join(", ")));
        }

        let generics = if all_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", all_params.join(", "))
        };
        writeln!(out, "public interface {cs_iface}{generics}");
        out.indent();
        for constraint in constraints {
            out.w("where ").wln(constraint);
        }
        out.dedent();
        out.open_brace();

        for item in t.items(db) {
            match item {
                AssocItem::Function(f) => {
                    self.emit_trait_method_sig(out, f);
                }
                AssocItem::Const(c) => {
                    if let Some(cn) = c.name(db) {
                        let c_name = names::const_name(cn.as_str());
                        let c_ty = self.rust_type_to_cs(&c.ty(db));
                        out.wln(format!("{} {}(); // const", c_ty, c_name));
                    }
                }
                AssocItem::TypeAlias(a) => {
                    let a_name = names::assoc_type_param(a.name(db).as_str());
                    out.wln(format!("// type {};", a_name));
                }
            }
        }

        out.close_brace();
        out.blank_line();
    }

    fn emit_trait_method_sig(&self, out: &mut Code, f: hir::Function) {
        let db = self.db;

        let gen_params = GenericDef::from(f).params(db);
        let (all_params, _constraints) = self.generic_params_cs(&gen_params);

        let is_async = f.is_async(db);
        let ret_ty = f.ret_type(db);
        let cs_ret = self.cs_ret_type(is_async, &ret_ty);
        let has_self = f.has_self_param(db);
        let is_static_kw = if !has_self { "static " } else { "" };
        let m_name = self.function_name(f);
        let params = self.build_param_list(f);
        let static_comment = if !has_self { "// " } else { "" };

        let generics = if all_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", all_params.join(", "))
        };
        out.wln(format!(
            "{static_comment}{is_static_kw}{cs_ret} {m_name}{generics}({params});"
        ));
    }

    /// Emit a function/method with a stub body (TODO: real body generation).
    pub fn emit_function(&self, out: &mut Code, f: hir::Function, impl_ctx: Option<Impl>) {
        let db = self.db;
        let _scope = tracing::info_span!(
            "emit_function",
            f = %f.debug_display(db),
        )
        .entered();

        if is_fn_cfg_disabled(f, db) {
            return;
        }

        let gen_params = if let ItemContainer::Impl(impl_) = f.container(db)
            && let Some(trait_) = impl_.trait_(db)
            && Some(trait_.into()) == self.lang_items.Hash
            && f.name(db) == hir::sym::hash
        {
            // it's hash. Derive method has <H> but `GenericDef::from` returns empty array
            // so we retrieve the H from parameter instead of GenericDef
            let param = f.params_without_self(db).swap_remove(0);
            let param_type = param.ty();
            let param_type = param_type.remove_ref().unwrap();
            let type_param = param_type
                .as_type_param(db)
                .unwrap_or_else(|| panic!("type {param_type:?} is not type_param"));
            vec![type_param.into()]
        } else {
            GenericDef::from(f).params(db)
        };
        let (tp_names, constraints) = self.generic_params_cs(&gen_params);
        let generics = self.format_generics(&tp_names);

        let is_async = f.is_async(db);
        let ret_ty = f.async_ret_type(db).unwrap_or(f.ret_type(db));
        let cs_ret = self.cs_ret_type(is_async, &ret_ty);
        let async_kw = if is_async { "async " } else { "" };
        let m_name = self.function_name(f);
        let has_self = f.has_self_param(db);
        let is_static_kw = if !has_self { "static " } else { "" };
        let params = self.build_param_list(f);

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
                "public {is_static_kw} partial {cs_ret} {m_name}{generics}({params})",
            );
            out.indent();
            for constraint in constraints {
                out.w("where ").wln(constraint);
            }
            out.dedent();
            out.wln(";");
            return;
        }
        writeln!(out, "// method on {}", cs_self);
        writeln!(
            out,
            "public {is_static_kw}{async_kw}{cs_ret} {m_name}{generics}({params})",
        );
        out.indent();
        for constraint in constraints {
            out.w("where ").wln(constraint);
        }
        out.dedent();
        out.open_brace();

        // Try to generate a real body using BodyGen
        let mut body_gen = BodyGen::new(self, is_async);
        body_gen.emit_function_body(self.sem.source(f).unwrap().value, out);

        out.dedent();
        out.wln("}");

        self.deferred(out, &body_gen.deferred, f.module(self.db));
        out.blank_line();
    }

    fn deferred(&self, out: &mut Code, deferred: &[ItemInBody], parent: hir::Module) {
        struct BlockModule {
            module: hir::Module,
            children: Vec<BlockModule>,
            items: Vec<ItemInBody>,
        }

        let mut root = BlockModule {
            module: parent,
            children: vec![],
            items: vec![],
        };

        let mut module_path = vec![];
        for &item in deferred {
            module_path.clear();
            let hir::ItemContainer::Module(mut mod_) = item.container(self.db) else {
                panic!("deferred item is not a module");
            };
            while {
                module_path.push(mod_);
                mod_ = mod_.parent(self.db).unwrap();
                mod_ != parent
            } {}
            module_path.reverse();

            let mut path = &mut root;

            assert_eq!(path.module, parent);

            for &module in &module_path {
                let index = path
                    .children
                    .iter()
                    .position(|x| x.module == module)
                    .unwrap_or_else(|| {
                        path.children.push(BlockModule {
                            module,
                            children: vec![],
                            items: vec![],
                        });
                        path.children.len() - 1
                    });
                path = &mut path.children[index];
            }

            path.items.push(item);
        }

        assert!(root.items.is_empty());

        if root.children.is_empty() {
            return;
        }

        let mut adt_impls = HashMap::<_, Vec<_>>::default();
        for &item in deferred {
            if let ItemInBody::Impl(impl_) = item {
                let self_ty = impl_.self_ty(self.db);
                eprintln!("impl: {:?}", self_ty);
                if let Some(adt) = self_ty.as_adt() {
                    eprintln!("impl for {:?}", adt);
                    adt_impls.entry(adt).or_default().push(impl_);
                }
            }
        }

        out.wln("// function local items");
        for module in root.children {
            emit_module(self, out, &module, &adt_impls, true);
        }

        fn emit_module(
            this: &CodeGenerator,
            out: &mut Code,
            module: &BlockModule,
            adt_impls: &HashMap<hir::Adt, Vec<hir::Impl>>,
            root: bool,
        ) {
            let module_name = this.mod_simple_name(module.module);
            out.wln(format!(
                "{access} static partial class {} {{",
                module_name,
                access = if root { "private" } else { "public" }
            ));
            out.indent();
            for item in &module.items {
                match *item {
                    ItemInBody::Function(f) => {
                        this.emit_function(out, f, None);
                    }
                    ItemInBody::Adt(adt) => {
                        this.emit_adt_with_impls(
                            out,
                            adt,
                            adt_impls.get(&adt).map(Vec::as_slice).unwrap_or(&[]),
                        );
                    }
                    ItemInBody::Impl(_) => {}
                }
            }
            for child in &module.children {
                emit_module(this, out, child, adt_impls, false);
            }
            out.dedent();
            out.wln("}");
        }
    }

    fn cs_ret_type(&self, is_async: bool, ret_ty: &hir::Type<'db>) -> String {
        if is_async {
            format!("r2CsRuntime.RustTask<{}>", self.rust_type_to_cs(ret_ty))
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
