use super::{CodeGenerator, expr::BodyGen, generic_args, names, output::Code};
use crate::codegen::expr::ItemInBody;
use crate::codegen::item_exclusion::should_emit;
use crate::codegen::simple_extensions::*;
use crate::codegen::ty::generic_types;
use cfg::CfgExpr;
/// Generates C# type declarations from Rust HIR types.
use hir::{Adt, AssocItem, HasContainer, HasSource, Impl, Trait, db::HirDatabase};
use itertools::Itertools;
use ra_internal::function::FunctionExt;
use ra_internal::*;
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
    #[tracing::instrument(skip(self, out), fields(trait = %t.debug_display(self.db)))]
    pub fn emit_trait(&self, out: &mut Code, t: Trait) {
        let db = self.db;

        let name = t.name(db);
        let cs_iface = names::trait_name(name.as_str());
        let _cs_dyn_iface = names::dyn_trait_name(name.as_str());

        let (all_params, constraints) = self.generic_def_params_cs(t.into());

        if let Some(compatibility) = t.dyn_compatibility_all_violations(db) {
            for x in compatibility {
                out.wln(format!("// dyn incompatible with {x:?}"));
            }
        } else {
            out.wln("// dyn compatible");
        }

        let self_ty_param = generic_types(&hir::GenericDef::from(t).params0(db))
            .next()
            .unwrap();
        let super_traits = (self_ty_param.trait_bounds_with_args(db).into_iter())
            .filter(|&(super_trait, _)| super_trait != t);

        let generics = if all_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", all_params.join(", "))
        };
        write!(out, "public interface {cs_iface}{generics}");
        {
            // super traits
            let mut super_traits = super_traits.clone();
            if let Some(first) = super_traits.next() {
                out.w(" : ")
                    .w(self.cs_path_with_args(first.0, first.1.clone()));
                for element in super_traits {
                    out.w(", ")
                        .w(self.cs_path_with_args(element.0, element.1.clone()));
                }
            }
        }
        writeln!(out);
        out.indent();
        for constraint in constraints {
            out.w("where ").wln(constraint);
        }
        out.dedent();
        out.open_brace();

        for item in t.items(db) {
            match item {
                AssocItem::Function(f) => {
                    if f.has_self_param(db) {
                        self.emit_function_signature(out, f, "", "", CsFunctionType::Normal, |x| x);
                        out.wln(";");
                    }
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

        if t.needs_statics(db) {
            out.w("public interface Statics");
            {
                // super traits
                let mut super_traits = super_traits.filter(|(t, _)| t.needs_statics(db));
                if let Some(first) = super_traits.next() {
                    out.w(" : ")
                        .w(self.cs_path_with_args(first.0, first.1.clone()))
                        .w(".Statics");
                    for element in super_traits {
                        out.w(", ")
                            .w(self.cs_path_with_args(element.0, element.1.clone()))
                            .w(".Statics");
                    }
                }
            }
            out.open_brace();
            for item in t.items(db) {
                if let AssocItem::Function(f) = item
                    && !f.has_self_param(db)
                    && !f.is_explicit_sized_self(db)
                {
                    self.emit_function_signature(
                        out,
                        f,
                        "",
                        "",
                        CsFunctionType::TraitStaticStruct,
                        |x| x,
                    );
                    out.wln(";");
                }
            }
            out.close_brace();
        }

        out.wln("public static class Defaults");
        out.open_brace();

        for item in t.items(db) {
            let AssocItem::Function(f) = item else {
                continue;
            };
            if !f.has_body(db) {
                continue;
            }
            self.emit_function_inner(out, f, None, CsFunctionType::TraitDefaultImpl);
        }
        out.close_brace();

        out.close_brace();
        out.blank_line();
    }

    pub fn emit_function(&self, out: &mut Code, f: hir::Function, impl_ctx: Option<Impl>) {
        self.emit_function_inner(out, f, impl_ctx, CsFunctionType::Normal);
    }

    #[tracing::instrument(skip(self, out), fields(function = %f.debug_display(self.db)))]
    pub fn emit_function_inner(
        &self,
        out: &mut Code,
        f: hir::Function,
        impl_ctx: Option<Impl>,
        cs_type: CsFunctionType,
    ) {
        let db = self.db;
        let _scope = tracing::info_span!(
            "emit_function",
            f = %f.debug_display(db),
        )
        .entered();

        if !should_emit(f, db) {
            return;
        }

        // Determine the class this method belongs to
        let cs_self = impl_ctx
            .map(|i| self.rust_type_to_cs(&i.self_ty(db)))
            .unwrap_or_else(|| "/* top-level */".to_string());

        if is_fn_r2cs_native(f, self.krate, db) {
            writeln!(out, "// [r2cs_native] - add implementation in r2CsNative/",);
            self.emit_function_signature(out, f, "public ", "partial ", cs_type, |x| x);
            out.wln(";");
            return;
        }

        let is_async = f.is_async(db);

        writeln!(out, "// method on {}", cs_self);
        self.emit_function_signature(
            out,
            f,
            "public ",
            if is_async { "async " } else { "" },
            cs_type,
            |x| x,
        );
        out.wln("");
        out.open_brace();

        // Try to generate a real body using BodyGen
        let mut body_gen = BodyGen::new(self, is_async, cs_type);
        body_gen.emit_function_body(self.sem.source(f).unwrap().value, out);

        out.dedent();
        out.wln("}");

        self.deferred(out, body_gen.deferred(), f.module(self.db));
        out.blank_line();
    }

    pub fn emit_function_signature(
        &self,
        out: &mut Code,
        f: hir::Function,
        access: &str,
        additional_modifier: &str,
        function_type: CsFunctionType,
        type_mapper: impl Fn(hir::Type<'db>) -> hir::Type<'db>,
    ) {
        let db = self.db;

        let (tp_names, constraints) = self.generic_def_params_cs(f.into());
        let generics = self.format_generics(&tp_names);

        let is_async = f.is_async(db);
        let ret_ty = type_mapper(f.async_ret_type(db).unwrap_or(f.ret_type(db)));
        let cs_ret = self.cs_ret_type(is_async, &ret_ty);
        let m_name = self.function_name(f);
        let has_self = f.has_self_param(db);
        let is_static = match function_type {
            CsFunctionType::Normal => !has_self,
            CsFunctionType::TraitStaticStruct => false,
            CsFunctionType::TraitDefaultImpl => true,
        };
        let is_static_kw = if is_static { "static " } else { "" };
        let params = self.build_param_list(f, type_mapper, function_type);

        write!(
            out,
            "{access}{is_static_kw}{additional_modifier}{cs_ret} {m_name}{generics}({params})",
        );
        out.indent();
        for constraint in constraints {
            out.wln("");
            out.w("where ").w(constraint);
        }
        out.dedent();
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
        let all_impls = deferred.iter().flat_map(|&item| match item {
            ItemInBody::Impl(impl_) => vec![impl_],
            ItemInBody::Adt(adt) => adt.module(self.db).impl_defs(self.db),
            _ => vec![],
        });
        for impl_ in all_impls {
            let self_ty = impl_.self_ty(self.db);
            //eprintln!("impl: {:?}", self_ty);
            if let Some(adt) = self_ty.as_adt() {
                //eprintln!("impl for {:?}", adt);
                let impls = adt_impls.entry(adt).or_default();
                if !impls.contains(&impl_) {
                    impls.push(impl_);
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

    fn build_param_list(
        &self,
        f: hir::Function,
        type_mapper: impl Fn(hir::Type<'db>) -> hir::Type<'db>,
        function_type: CsFunctionType,
    ) -> String {
        let db = self.db;

        let params = f.params_without_self(db);
        let params = params.iter().enumerate().map(|(i, param)| {
            let cs_ty = self.rust_type_to_cs(&type_mapper(param.ty().clone()));
            let p_name = param
                .name(db)
                .map(|n| names::local_name(n.as_str(), 0))
                .unwrap_or_else(|| format!("p_{}", i));
            format!("{} {}", cs_ty, p_name)
        });
        (matches!(function_type, CsFunctionType::TraitDefaultImpl) && f.has_self_param(db))
            .then(|| {
                let hir::ItemContainer::Trait(trait_) = f.container(db) else {
                    unreachable!()
                };

                let (all_params, _) = self.generic_def_params_cs(trait_.into());

                format!(
                    "{} self",
                    generic_args(self.trait_itf_cs(trait_), all_params)
                )
            })
            .into_iter()
            .chain(params)
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

#[derive(Debug, Copy, Clone)]
pub enum CsFunctionType {
    Normal,
    TraitStaticStruct,
    TraitDefaultImpl,
}
