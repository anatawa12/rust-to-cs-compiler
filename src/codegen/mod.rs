use crate::codegen::simple_extensions::*;
#[macro_use]
pub mod output;
#[macro_use]
mod impl_from;
mod constructable;
pub mod decl;
mod dyn_compatibility;
pub mod expr;
mod function_resolution;
mod id_map;
pub mod item_exclusion;
pub mod names;
pub mod ty;

#[path = "."]
mod simple_extensions {
    #![allow(unused_imports)]

    #[path = "simple_extensions.rs"]
    mod impl_;

    pub use impl_::GenericDefExt as _;
    pub use impl_::HasTraitBase as _;
    pub use impl_::TraitExt as _;
    pub use impl_::TypeExt as _;
    pub use impl_::TypeParamExt as _;
}

use self::output::Code;
use crate::codegen::constructable::ConstructableDef;
use crate::codegen::decl::CsFunctionType;
use crate::codegen::id_map::IdMap;
use crate::codegen::item_exclusion::{is_r2cs_native, should_emit};
use crate::codegen::ty::generic_types;
use hir::{
    Adt, AssocItem, Crate, Impl, InFile, Module, ModuleDef, Semantics, StructKind, TypeParam,
    db::HirDatabase, sym,
};
use ide_db::line_index;
use ra_internal::function::FunctionExt;
use ra_internal::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use vfs::Vfs;

pub struct CodeGenerator<'db> {
    db: &'db dyn HirDatabase,
    vfs: &'db Vfs,
    krate: Crate,
    sem: Semantics<'db, dyn HirDatabase>,
    lang_items: &'db LangItems,
    // some internal information that hard is to determine
    impl_ty_param_id: IdMap<TypeParam>,
    // This map holds specially handled types like type arguments mirroring impl Fn()
    #[allow(clippy::type_complexity)]
    special_types: RefCell<HashMap<TypeParam, Box<dyn (Fn(&CodeGenerator<'db>) -> String) + 'db>>>,
    // This map holds 'replaced' types to map `Self` type in trait default impls.
    type_map: TyMap<'db>,
}

impl<'db> CodeGenerator<'db> {
    pub fn new(db: &'db dyn HirDatabase, vfs: &'db Vfs, krate: Crate) -> Self {
        Self {
            db,
            vfs,
            krate,
            sem: Semantics::new_dyn(db),
            lang_items: LangItems::new(db, krate),

            impl_ty_param_id: IdMap::new("impl_"),
            special_types: RefCell::new(HashMap::new()),
            type_map: TyMap::new(),
        }
    }

    pub fn location_with_file(
        &self,
        f: InFile<impl as_syntax_node_ptr::IntoSyntaxNodePtr>,
    ) -> String {
        let loc = self.sem.diagnostics_display_range(f.map(|x| x.as_ptr()));

        let path = self.vfs.file_path(loc.file_id);
        let line_index = line_index(self.db, loc.file_id);
        let line_col = line_index.line_col(loc.range.start());

        format!("{}:{}:{}", path, line_col.line + 1, line_col.col + 1)
    }
}

pub mod as_syntax_node_ptr {

    use hir::ModuleSource;
    use syntax::ast::Impl;
    use syntax::{AstNode, AstPtr, SyntaxNode, SyntaxNodePtr};

    pub trait IntoSyntaxNodePtr {
        fn as_ptr(&self) -> SyntaxNodePtr;
    }

    macro_rules! impls {
        (
            $(
                |$self: tt: $ty: ty| $body: expr
            ),*
            $(,)?
        ) => {
            $(
                impl IntoSyntaxNodePtr for $ty {
                    #[inline]
                    fn as_ptr(&$self) -> SyntaxNodePtr {
                        $body
                    }
                }
            )*
        };
    }

    impls!(
        |self: SyntaxNodePtr| *self,
        |self: SyntaxNode| SyntaxNodePtr::new(self),
        |self: ModuleSource| self.node().as_ptr(),
        |self: Impl| self.syntax().as_ptr(),
    );

    impl<T: AstNode> IntoSyntaxNodePtr for AstPtr<T> {
        fn as_ptr(&self) -> SyntaxNodePtr {
            self.syntax_node_ptr()
        }
    }
}

impl<'db> CodeGenerator<'db> {
    /// Generate C# code for all declarations in a crate.
    pub fn emit_crate(&self, krate: Crate, out: &mut Code) {
        let db = self.db;

        // Collect all impls keyed by ADT (by display name as a quick proxy)
        let mut adt_impls: HashMap<String, Vec<Impl>> = HashMap::new();

        for module in krate.modules(db) {
            if !should_emit(module, db) {
                continue;
            }
            for impl_ in module.impl_defs(db) {
                if !should_emit(impl_, db) {
                    continue;
                }
                let self_ty = impl_.self_ty(db);
                if let Some(adt) = self_ty.as_adt() {
                    let key = self.adt_key(adt);
                    adt_impls.entry(key).or_default().push(impl_);
                }
            }
        }

        self.emit_module(out, krate.root_module(db), &adt_impls);
    }

    fn emit_module(&self, out: &mut Code, module: Module, adt_impls: &HashMap<String, Vec<Impl>>) {
        let db = self.db;
        if !should_emit(module, db) {
            return;
        }

        let module_name = self.mod_simple_name(module);

        if is_r2cs_native(module, db) {
            out.wln("// r2cs native, manual implementation expected");
            out.wln(format!("public static partial class {} {{}}", module_name));
            return;
        }

        out.w(format!("public static partial class {} ", module_name));
        out.open_brace();

        // Emit traits first (no impl needed)
        for def in module.declarations(db) {
            if let ModuleDef::Trait(t) = def {
                self.emit_trait(out, t);
            }
        }

        // Emit consts first (no impl needed)
        for def in module.declarations(db) {
            if let ModuleDef::Const(c) = def {
                self.emit_const(out, c);
            }
        }

        // Emit type aliases and free functions
        for def in module.declarations(db) {
            if let ModuleDef::Function(f) = def {
                let mod_name = self.mod_simple_name(module);
                out.wln(format!(
                    "// function {} in module {}",
                    f.name(db).as_str(),
                    mod_name
                ));
                self.emit_function(out, f, None);
            }
        }

        // Emit each ADT with its impl blocks
        for def in module.declarations(db) {
            if let ModuleDef::Adt(adt) = def {
                let key = self.adt_key(adt);
                let impls = adt_impls.get(&key).cloned().unwrap_or_default();
                self.emit_adt_with_impls(out, adt, &impls);
            }
        }

        // Emit submodules
        for def in module.declarations(db) {
            if let ModuleDef::Module(module) = def {
                self.emit_module(out, module, adt_impls);
            }
        }

        out.close_brace();
    }

    #[tracing::instrument(skip(self, out), fields(adt = ?adt.debug_display(self.db)))]
    fn emit_adt_with_impls(&self, out: &mut Code, adt: Adt, impls: &[Impl]) {
        let db = self.db;

        if !should_emit(adt, db) {
            out.wln(format!("// Omit {}", adt.name(db).as_str()));
            return;
        }

        if is_r2cs_native(adt, db) {
            let cs_name = match adt {
                Adt::Struct(s) => names::struct_name(s.name(db).as_str()),
                Adt::Enum(e) => names::struct_name(e.name(db).as_str()),
                Adt::Union(u) => names::struct_name(u.name(db).as_str()),
            };
            out.wln(format!(
                "// [r2cs_native] {} — add implementation in r2CsNative/",
                cs_name
            ));
            out.wln(format!("public partial class {cs_name} {{}}"));
            out.blank_line();
            return;
        }

        let cs_name = names::struct_name(adt.name(db).as_str());

        let (tp_names, constraints) = self.generic_def_params_cs(adt.into());

        // Compute implemented interfaces
        let mut trait_interfaces: Vec<String> = Vec::new();
        for &impl_ in impls {
            //if let Some((trait_, args)) = impl_.trait_with_args(db) {
            if let Some(trait_ref) = impl_.trait_ref(db) {
                let trait_ = trait_ref.trait_();
                let cs_iface = self.cs_path_with_args(trait_, trait_ref.generic_types(db));

                trait_interfaces.push(cs_iface);
                if self.lang_items.Ord() == Some(trait_) {
                    trait_interfaces.push(format!("System.IComparable<{cs_name}>"));
                }
            }
        }

        let generics = if tp_names.is_empty() {
            String::new()
        } else {
            format!("<{}>", tp_names.join(", "))
        };

        // Build class header

        let interfaces_part = if trait_interfaces.is_empty() {
            String::new()
        } else {
            format!(" : {}", trait_interfaces.join(", "))
        };

        match adt {
            Adt::Struct(s) => {
                out.wln(format!(
                    "public partial class {cs_name}{generics}{interfaces_part}",
                ));
                out.indent();
                for constraint in constraints {
                    out.w("where ").wln(constraint);
                }
                out.dedent();
                out.open_brace();

                self.emit_constructable_def(
                    out,
                    &format!("{cs_name}{generics}"),
                    &cs_name,
                    s.into(),
                );
                out.blank_line();

                // Methods from all impl blocks
                for &impl_ in impls {
                    self.emit_impl_methods(out, &cs_name, impl_);
                }

                out.close_brace();
                out.blank_line();
            }
            Adt::Enum(e) => {
                out.wln("[global::System.Runtime.CompilerServices.UnionAttribute]");
                out.wln(format!(
                    "public sealed partial class {}{}{}",
                    cs_name, generics, interfaces_part
                ));
                out.indent();
                for constraint in constraints {
                    out.w("where ").wln(constraint);
                }
                out.dedent();
                out.open_brace();
                out.wln("private object inner;");
                out.wln("public object Value => inner;");
                out.wln(format!(
                    "public void set({cs_name}{generics} o) => inner = o.inner;"
                ));
                out.blank_line();

                // Variants
                for variant in e.variants(db) {
                    let v_name = names::variant_name(variant.name(db).as_str());
                    out.wln(format!(
                        "public {}({v_name} value) => inner = value;",
                        cs_name
                    ));
                    out.wln(format!("public sealed partial class {v_name}",));
                    out.open_brace();

                    self.emit_constructable_def(
                        out,
                        &format!("{cs_name}{generics}"),
                        &v_name,
                        variant.into(),
                    );
                    out.close_brace();
                    out.blank_line();
                }

                // Methods from all impl blocks
                for &impl_ in impls {
                    self.emit_impl_methods(out, &cs_name, impl_);
                }

                out.close_brace();
                out.blank_line();
            }
            Adt::Union(_) => {
                // Skip unions
            }
        }
    }

    fn emit_constructable_def(
        &self,
        out: &mut Code,
        cs_type: &str,
        cs_name: &str,
        s: ConstructableDef,
    ) {
        let db = self.db;

        // Fields
        for field in s.fields(db) {
            let f_ty_ns = field.ty(db);
            let f_ty = f_ty_ns.to_type(db);
            let cs_ty = self.rust_type_to_cs(&f_ty);
            let f_name = names::field_name(field.name(db).as_str());
            out.wln(format!("public {} {} = default!;", cs_ty, f_name));
        }

        match s.kind(self.db) {
            StructKind::Record => {}
            StructKind::Tuple => {
                if !s.fields(db).is_empty() {
                    out.blank_line();
                }
                // create tuple constructor
                out.w(code!(
                    "public static ",
                    cs_type,
                    " ctor(",
                    join(
                        s.fields(db).iter().map(|field| {
                            let f_ty_ns = field.ty(db);
                            let f_ty = f_ty_ns.to_type(db);
                            let cs_ty = self.rust_type_to_cs(&f_ty);
                            let f_name = names::field_name(field.name(db).as_str());
                            code!(cs_ty, " ", f_name)
                        }),
                        ", "
                    ),
                    ") => new ",
                    cs_name,
                    "() {\n",
                    indent,
                    join(
                        s.fields(db).iter().map(|field| {
                            let f_name = names::field_name(field.name(db).as_str());
                            code!(f_name, " = ", f_name, ",\n")
                        }),
                        ""
                    ),
                    dedent,
                    "};\n"
                ));
            }
            StructKind::Unit => {
                out.w("private ").w(cs_name).wln("(){}");
                out.w("public static ")
                    .w(cs_type)
                    .w(" instance = new ")
                    .w(cs_name)
                    .wln("();");
            }
        }
    }

    #[tracing::instrument(skip(self, out), fields(
        impl_trait = ?impl_.trait_(self.db).as_ref().map(|x| x.debug_display(self.db)),
        impl_self = %impl_.self_ty(self.db).debug_display(self.db),
    ))]
    fn emit_impl_methods(&self, out: &mut Code, cs_name: &str, impl_: Impl) {
        let db = self.db;
        if !should_emit(impl_, db) {
            return;
        }
        for item in impl_.items(db) {
            if let AssocItem::Function(f) = item {
                self.emit_function(out, f, Some(impl_));
            }
        }

        if let Some(trait_ref) = impl_.trait_ref(db)
            && matches!(
                trait_ref.trait_().name(db).as_str(),
                "Visitor"
                    | "Deserializer"
                    | "Serializer"
                    | "SeqAccess"
                    | "MapAccess"
                    | "Deserialize"
                    | "EnumAccess"
                    | "VariantAccess"
                    | "SerializeMap"
                    | "SerializeStruct"
                    | "SerializeStructVariant"
            )
        {
            // implement inherited default methods
            let _trait_super_impl_scope = tracing::debug_span!("emit_impl_methods of super methods", trait = trait_ref.trait_().name(db).as_str()).entered();

            let implemented_fns = (impl_.items(db).iter())
                .filter_map(|item| item.as_function())
                .map(|f| f.name(db).symbol().clone())
                .collect::<HashSet<_>>();

            out.wln("// default impls");

            let declared_class = format!(
                "{trait_}.Defaults",
                trait_ = self.cs_path_with_args(trait_ref.trait_(), trait_ref.generic_types(db))
            );

            for f in (trait_ref.trait_().items(db).iter())
                .filter_map(|x| x.as_function())
                .filter(|f| !implemented_fns.contains(f.name(db).symbol()))
            {
                self.emit_wrapper_fn(out, &trait_ref, f, &declared_class, CsFunctionType::Normal);
            }
        }

        if impl_
            .trait_(db)
            .is_some_and(|x| self.lang_items.PartialOrd() == Some(x))
        {
            let self_ty = impl_.self_ty(db);
            let self_ty_cs = self.rust_type_to_cs(&self_ty);
            out.wln(fcode!("public static bool operator<({self_ty_cs} self, {self_ty_cs} right) => self.m_PartialCmp(right).m_IsSomeAnd(x => x.m_IsLt());"));
            out.wln(fcode!("public static bool operator>({self_ty_cs} self, {self_ty_cs} right) => self.m_PartialCmp(right).m_IsSomeAnd(x => x.m_IsGt());"));
            out.wln(fcode!("public static bool operator<=({self_ty_cs} self, {self_ty_cs} right) => self.m_PartialCmp(right).m_IsSomeAnd(x => x.m_IsLe());"));
            out.wln(fcode!("public static bool operator>=({self_ty_cs} self, {self_ty_cs} right) => self.m_PartialCmp(right).m_IsSomeAnd(x => x.m_IsGe());"));
        } else if impl_
            .trait_(db)
            .is_some_and(|x| self.lang_items.Ord() == Some(x))
        {
            let self_ty = impl_.self_ty(db);
            let self_ty_cs = self.rust_type_to_cs(&self_ty);
            out.wln(fcode!(
                "public int CompareTo({self_ty_cs} other) => this.m_Cmp(other).CsOrder();"
            ));
        } else if let Some(trait_ref) = impl_.trait_ref(db)
            && self.lang_items.PartialEq() == Some(trait_ref.trait_())
        {
            let self_ty = impl_.self_ty(db);
            let arg_ty = trait_ref.get_type_argument(1).unwrap().to_type(db);
            let self_ty_cs = self.rust_type_to_cs(&self_ty);
            let arg_ty_cs = self.rust_type_to_cs(&arg_ty);
            out.wln(fcode!("public static bool operator==({self_ty_cs} self, {arg_ty_cs} right) => self.m_Eq(right);"));
            out.wln(fcode!("public static bool operator!=({self_ty_cs} self, {arg_ty_cs} right) => !self.m_Eq(right);"));
            if self_ty == arg_ty {
                out.wln(fcode!("public override bool Equals(object? obj) => obj is {self_ty_cs} cast && this == cast;"));
            }
        }

        if let Some(trait_ref) = impl_.trait_ref(db)
            && let static_fns = trait_ref
                .trait_()
                .items(db)
                .into_iter()
                .filter_map(|x| variant_or_none!(x, hir::AssocItem::Function))
                .filter(|x| !x.has_self_param(db) && !x.is_explicit_sized_self(db))
                .collect::<Vec<_>>()
            && !static_fns.is_empty()
        {
            out.wln(fcode!(
                "// Statics wrapper for {}",
                trait_ref.trait_().name(db).as_str()
            ));
            out.wln(fcode!("public partial struct Statics : {trait}.Statics {{", trait = self.cs_path_with_args(trait_ref.trait_(), trait_ref.generic_types(db))));
            out.indent();

            for f in static_fns {
                self.emit_wrapper_fn(
                    out,
                    &trait_ref,
                    f,
                    cs_name,
                    CsFunctionType::TraitStaticStruct,
                );
            }
            out.dedent();
            out.wln(fcode!("}}"));
        }
    }

    #[tracing::instrument(skip_all, fields(f = %f.debug_display(self.db), trait = %trait_ref.trait_().debug_display(self.db)))]
    fn emit_wrapper_fn(
        &self,
        out: &mut Code,
        trait_ref: &hir::TraitRef<'db>,
        f: hir::Function,
        declared_class: &str,
        function_type: CsFunctionType,
    ) {
        let db = self.db;
        let self_def = hir::GenericDef::from(f);

        let type_args = self_def.type_args_maps_trait_to_this_impl(db, trait_ref);
        let generics = generic_args(
            "".into(),
            self.map_cs_type_param_source(
                &self.generic_params_cs_sources(f.into()),
                &generic_types(&self_def.params0(db))
                    .map(|param| param.ty(db))
                    .collect::<Vec<_>>(),
            ),
        );
        let f_name = self.function_name(f);
        let params = (f.assoc_fn_params(db).iter().enumerate())
            .map(|(i, param)| {
                if param.name(db).is_some_and(|n| n == sym::self_) {
                    "this".into()
                } else {
                    param
                        .name(db)
                        .map(|n| names::local_name(n.as_str(), 0))
                        .unwrap_or_else(|| format!("p_{}", i - (f.has_self_param(db) as usize)))
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        self.emit_function_signature(out, f, "public ", "", function_type, |x| {
            x.instantiate(f.into(), &type_args, db)
        });
        out.wln("");
        out.indent();
        out.wln(fcode!("=> {declared_class}.{f_name}{generics}({params});"));
        out.dedent();
    }

    fn adt_key(&self, adt: Adt) -> String {
        self.adt_name_cs(adt)
    }
}

fn generic_args(mut base: String, args: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut iter = args.into_iter();

    if let Some(arg) = iter.next() {
        base.push('<');
        base.push_str(arg.as_ref());
        for arg in iter {
            base.push(',');
            base.push(' ');
            base.push_str(arg.as_ref());
        }
        base.push('>');
    }

    base
}

/// The function to be used for new_associated_type
fn bounds_provider<'db>(
    ty: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> Vec<(hir::Trait, Vec<hir::Type<'db>>)> {
    if let Some((param, _)) = ty.as_assoc_of_type_param(db) {
        param
            .trait_bounds_of_nested_type_with_args(ty, db)
            .left()
            .unwrap_or(vec![])
    } else if let Some(param) = ty.as_type_param(db) {
        param
            .trait_bounds_of_nested_type_with_args(ty, db)
            .left()
            .unwrap_or(vec![])
    } else {
        vec![]
    }
}
