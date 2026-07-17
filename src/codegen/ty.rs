pub mod generic_params;
pub mod trait_assoc_types;

use super::{CodeGenerator, generic_args, names};
use crate::codegen::constructable::{Constructable, ConstructableDef};
use crate::codegen::simple_extensions::*;
use crate::codegen::ty::generic_params::resolve_cs_type_param_source;
/// Converts Rust HIR types to C# type strings.
use hir::db::HirDatabase;
use hir::{Adt, BuiltinType, GenericDef, ItemContainer, Module, Name, Symbol, Trait, Type};
use hir::{HasContainer, HasCrate, sym};
use itertools::Either;
use ra_internal::*;
use std::iter;
use tracing::*;

#[derive(Debug)]
pub struct CsTypeOption {
    pub apply_special: bool,
    pub is_static_container: bool,
    pub is_static_access: bool,
}

impl Default for CsTypeOption {
    fn default() -> Self {
        Self {
            apply_special: true,
            is_static_container: false,
            is_static_access: false,
        }
    }
}

impl CsTypeOption {
    pub fn no_special(mut self) -> Self {
        self.apply_special = false;
        self
    }

    pub fn static_container(mut self, static_container: bool) -> Self {
        self.is_static_container = static_container;
        self
    }

    pub fn static_access(mut self, is_static_access: bool) -> Self {
        self.is_static_access = is_static_access;
        self
    }
}

impl<'db> CodeGenerator<'db> {
    pub fn rust_type_to_cs(&self, ty: &Type<'db>) -> String {
        self.rust_type_to_cs_options(ty, CsTypeOption::default())
    }

    #[tracing::instrument(skip(self, ty), fields(ty = %ty.debug_display(self.db), option))]
    pub fn rust_type_to_cs_options(&self, ty: &Type<'db>, option: CsTypeOption) -> String {
        let db = self.db;
        if ty.is_unit() {
            return "global::System.ValueTuple".to_string();
        }
        if ty.is_never() {
            return "void".to_string();
        }

        let mapped = self.type_map.map_type_recursively(ty, db);
        let ty = mapped.as_ref().unwrap_or(ty);
        let ty = &ty.resolve_associated_type(db);

        // Primitives
        if let Some(builtin) = ty.as_builtin() {
            return self.builtin_to_cs(builtin);
        }

        // Reference → just the inner type (C# is reference semantics; we use Slot<T> for mutability)
        if let Some((inner, _mutability)) = ty.as_reference() {
            return self.rust_type_to_cs(&inner);
        }

        // Raw pointer → Ref<T>
        if let Some((inner, _)) = ty.as_raw_ptr() {
            return format!("Ref<{}>", self.rust_type_to_cs(&inner));
        }

        // Slice
        if let Some(inner) = ty.as_slice() {
            return format!("System.Memory<{}>", self.rust_type_to_cs(&inner));
        }

        // Array
        if let Some(inner) = ty.as_array_unsized(db) {
            return format!("{}[]", self.rust_type_to_cs(&inner));
        }

        // Tuple
        if ty.is_tuple() {
            let fields = ty.tuple_fields(db);
            if fields.is_empty() {
                return "global::System.ValueTuple".to_string();
            }
            if fields.len() == 1 {
                return format!(
                    "global::System.ValueTuple<{}>",
                    self.rust_type_to_cs(&fields[0])
                );
            }
            let parts: Vec<String> = fields.iter().map(|t| self.rust_type_to_cs(t)).collect();
            return format!("({})", parts.join(", "));
        }

        // ADT (struct/enum/union)
        if let Some((adt, args)) = ty.as_adt_with_args() {
            // Check std library mappings first
            if let Some(mapped) = self.map_std_type(adt.name(db).as_str(), &args, db) {
                return mapped;
            }

            let path = self.cs_path_with_args(adt, args);
            if option.is_static_container {
                format!("{}.Statics", path)
            } else {
                path
            }
        } else if let Some(trait_) = ty.as_dyn_trait() {
            // dyn Trait → T_TraitName (dyn interface)
            self.trait_itf_cs(trait_)
        } else if let Some(param) = ty.as_type_param(db) {
            if option.apply_special
                && let Some(cs) = self.special_types.borrow().get(&param)
            {
                return cs(self);
            }

            // Generic parameter
            let name = param.name(db).as_str().to_string();
            let mut cs_name = if name == "Self" {
                "P_Self".to_string()
            } else if !param.is_implicit(db) {
                names::generic_param(&name)
            } else {
                format!("/* implicit */ {}", self.impl_ty_param_id.id_name(&param))
            };

            if option.is_static_container {
                cs_name.push_str("_statics");
            } else if option.is_static_access {
                cs_name = format!("default({cs_name}_statics)");
            }

            cs_name
        } else if let Some((param, aliases)) = ty.as_assoc_of_type_param(db) {
            if is_omit_trait_assoc_type(db, *aliases.last().unwrap()) {
                let mut traits = param
                    .trait_bounds_of_nested_type_with_args(ty, db)
                    .expect_left("Bounds of ArgOnlyTrait is projection");
                traits.retain(|&(trait_, _)| !ignored_trait(trait_, db));
                let (trait_, args) = { traits }.swap_remove(0);
                return self.cs_path_with_args(trait_, args);
            }

            let mut type_name = if param.is_implicit(db) && param.name(db) == sym::Self_ {
                "A".to_string()
            } else {
                self.rust_type_to_cs_options(&param.ty(db), CsTypeOption::default().no_special())
            };

            for alias in aliases {
                type_name.push('_');
                type_name.push_str(alias.name(db).as_str());
            }

            if option.is_static_container {
                type_name.push_str("_statics");
            } else if option.is_static_access {
                type_name = format!("default({type_name}_statics)");
            }
            //type_name.push_str(&format!(" /* {} */", ty.display(db, self.display_target())));

            type_name
        } else if let Some(mut traits) = ty.as_impl_traits_with_params(db) {
            // Unfortunately hir crate does not provide us generic parameters of trait so we access
            // new solver's ty

            traits.retain_mut(|&mut (t, _)| !ignored_trait(t, db));

            if traits.len() > 1 {
                eprintln!(
                    "Multiple traits are used(r2cs): {}",
                    ty.debug_display(self.db)
                );
            }
            if traits.is_empty() {
                eprintln!("empty impl traits: {:?}", ty);
                "object".to_string()
            } else {
                let (trait_, args) = { traits }.swap_remove(0);

                self.cs_path_with_args(trait_, args)
            }
        } else if ty.is_error() {
            "object /* error type */".to_string()
        } else if ty.is_fn() || ty.is_closure() {
            // Closure types, fn pointers, etc. — use Action/Func
            "Action".to_string()
        } else if let normalized = ty.resolve_associated_type(db)
            && &normalized != ty
        {
            tracing::debug!(
                "Normalized! {} => {} ({normalized:?})\n",
                ty.debug_display(self.db),
                normalized.debug_display(self.db)
            );
            self.rust_type_to_cs(&normalized)
        } else {
            eprintln!(
                "Unsupported type: {}: {ty:?}\n",
                ty.debug_display(self.db),
                ty = ty.ty_string(self.db),
            );
            format!(
                "object /*Unsupported type: {} */",
                ty.debug_display(self.db)
            )
            .to_string()
        }
    }

    pub fn builtin_to_cs(&self, builtin: BuiltinType) -> String {
        let name = builtin.name();
        match name.as_str() {
            "bool" => "bool",
            "char" => "char",
            "str" => "string",
            "i8" => "sbyte",
            "i16" => "short",
            "i32" => "int",
            "i64" => "long",
            "i128" => "global::VrcGetVpm.Int128",
            "isize" => "nint",
            "u8" => "byte",
            "u16" => "ushort",
            "u32" => "uint",
            "u64" => "ulong",
            "u128" => "global::VrcGetVpm.UInt128",
            "usize" => "nuint",
            "f16" => "global::System.Half",
            "f32" => "float",
            "f64" => "double",
            _ => {
                eprintln!("Unsupported builtin type: {}", name.as_str());
                "object"
            }
        }
        .to_string()
    }

    /// Maps well-known std types to C# equivalents.
    fn map_std_type(
        &self,
        rust_name: &str,
        args: &[Option<Type<'db>>],
        _db: &'db dyn HirDatabase,
    ) -> Option<String> {
        match rust_name {
            // There is no CoW in this world
            "Cow" => Some(self.rust_type_to_cs(args.iter().flatten().nth(0).unwrap())),
            "String" | "str" => Some("string".to_string()),
            "Vec" => {
                let inner = args.first()?.as_ref()?;
                Some(format!(
                    "System.Collections.Generic.List<{}>",
                    self.rust_type_to_cs(inner)
                ))
            }
            "Box" => {
                let inner = args.first()?.as_ref()?;
                Some(self.rust_type_to_cs(inner))
            }
            "Arc" | "Rc" | "Mutex" | "RwLock" => {
                let inner = args.first()?.as_ref()?;
                Some(self.rust_type_to_cs(inner))
            }
            // indexmap is orderedm but Dictionary is not
            "HashMap" | "BTreeMap" | /*"IndexMap" | */"AHashMap" => {
                let k = args.first()?.as_ref()?;
                let v = args.get(1)?.as_ref()?;
                Some(format!(
                    "System.Collections.Generic.Dictionary<{}, {}>",
                    self.rust_type_to_cs(k),
                    self.rust_type_to_cs(v)
                ))
            }
            "HashSet" => {
                let inner = args.first()?.as_ref()?;
                Some(format!(
                    "System.Collections.Generic.HashSet<{}>",
                    self.rust_type_to_cs(inner)
                ))
            }
            "OsString" | "OsStr" | "CString" | "CStr" => Some("string".to_string()),
            "Duration" => Some("System.TimeSpan".to_string()),
            "Url" => Some("System.Uri".to_string()),
            "Bytes" | "BytesMut" => Some("byte[]".to_string()),
            "RustTask" => {
                // Already mapped
                let inner = args.first()?.as_ref()?;
                let t = self.rust_type_to_cs(inner);
                if t == "void" {
                    Some("r2CsRuntime.RustTask<int>".to_string()) // unit tasks use int
                } else {
                    Some(format!("r2CsRuntime.RustTask<{}>", t))
                }
            }
            _ => None,
        }
    }

    /// Format a C# generic argument list for a function/type.
    pub fn generic_def_params_cs(&self, def: hir::GenericDef) -> (Vec<String>, Vec<String>) {
        let db = self.db;
        let params = &def.params0(db);
        self.register_special_impls(params);
        let sources = self.generic_params_cs_sources(def);
        let instances = generic_types(params)
            .map(|param| param.ty(db))
            .collect::<Vec<_>>();
        let type_params = self
            .map_cs_type_param_source(&sources, &instances)
            .collect();
        let constraints = self.generic_params_cs_constraints(&sources, params);

        (type_params, constraints)
    }
}

pub fn generic_types(params: &[hir::GenericParam]) -> impl Iterator<Item = hir::TypeParam> + Clone {
    (params.iter()).filter_map(|&x| variant_or_none!(x, hir::GenericParam::TypeParam))
}

impl<'db> CodeGenerator<'db> {
    pub fn register_special_impls(&self, params: &[hir::GenericParam]) {
        let db = self.db;

        for param in generic_types(params) {
            if param.is_implicit(db) && param.name(db) == sym::Self_ {
                continue;
            }

            match self.special_type_param(param) {
                SpecialImplBounds::Func(output, parameters) => {
                    if output.is_unit() {
                        self.special_types.borrow_mut().insert(
                            param,
                            Box::new(move |this| {
                                generic_args(
                                    "global::System.Action".to_string(),
                                    parameters.iter().map(|x| this.rust_type_to_cs(x)),
                                )
                            }),
                        );
                    } else {
                        let mut types = parameters;
                        types.push(output);

                        self.special_types.borrow_mut().insert(
                            param,
                            Box::new(move |this| {
                                generic_args(
                                    "global::System.Func".to_string(),
                                    types.iter().map(|x| this.rust_type_to_cs(x)),
                                )
                            }),
                        );
                    }
                }
                SpecialImplBounds::Future(output) => {
                    self.special_types.borrow_mut().insert(
                        param,
                        Box::new(move |this| {
                            generic_args(
                                "r2CsRuntime.RustTask".to_string(),
                                [this.rust_type_to_cs(&output)],
                            )
                        }),
                    );
                }
                SpecialImplBounds::ArgOnlyTrait(param_type) => {
                    let traits = param
                        .trait_bounds_of_nested_type_with_args(&param_type, db)
                        .expect_left("Bounds of ArgOnlyTrait is projection");
                    let (trait_, args) = { traits }.swap_remove(0);

                    assert!(!self.with_self_in_cs(trait_));

                    self.special_types.borrow_mut().insert(
                        param,
                        Box::new(move |this| this.cs_path_with_args(trait_, args.clone())),
                    );
                }
                SpecialImplBounds::None => {}
            }
        }
    }

    pub fn generic_params_cs_constraints(
        &self,
        cs_sources: &[generic_params::CsTypeParamSource],
        params: &[hir::GenericParam],
    ) -> Vec<String> {
        let db = self.db;

        let mut constraints = Vec::new();

        let type_prams = generic_types(params).collect::<Vec<_>>();
        let types = type_prams.iter().map(|x| x.ty(db)).collect::<Vec<_>>();

        for source in cs_sources {
            let param_type = resolve_cs_type_param_source(source, &types, db);
            let param = type_prams[source.index()];

            let name = self.rust_type_to_cs_options(
                &param_type,
                CsTypeOption::default().static_container(source.is_static_container()),
            );

            match param.trait_bounds_of_nested_type_with_args(&param_type, db) {
                Either::Right(_) if source.is_static_container() => {
                    constraints.push(format!("{name} : struct"));
                }
                Either::Left(traits) if source.is_static_container() => {
                    let mut cs_constraints = vec!["struct".into()];
                    for (trait_, args) in traits {
                        if trait_.needs_statics(db) {
                            cs_constraints.push(self.cs_path_with_args(trait_, args) + ".Statics");
                        }
                    }
                    constraints.push(format!("{name} : {}", cs_constraints.join(", ")));
                }
                Either::Right(_) => {
                    // nothing to do for projection
                }
                Either::Left(traits) => {
                    if !traits.is_empty() {
                        let mut cs_constraints = vec![];
                        for (trait_, args) in traits {
                            cs_constraints.push(self.cs_path_with_args(trait_, args));
                        }
                        constraints.push(format!("{name} : {}", cs_constraints.join(", ")));
                    }
                }
            }
        }

        constraints
    }
}

enum SpecialImplBounds<'db> {
    None,
    Func(Type<'db>, Vec<Type<'db>>),
    Future(Type<'db>),
    ArgOnlyTrait(Type<'db>),
}

impl<'db> SpecialImplBounds<'db> {
    fn is_special_impl(&self) -> bool {
        !matches!(self, SpecialImplBounds::None)
    }
}

impl<'db> CodeGenerator<'db> {
    fn special_type_param(&self, param: hir::TypeParam) -> SpecialImplBounds<'db> {
        let db = self.db;
        let _scope = tracing::info_span!(
            "special_type_param",
            param = %param.debug_display(db),
        )
        .entered();

        let lang_items = LangItems::new(db, param.module(db).krate(db));

        if let bounds = param
            .trait_bounds_with_args(db)
            .into_iter()
            .filter(|&(t, _)| !ignored_trait(t, db))
            .collect::<Vec<_>>()
            && let &[(trait_, ref args)] = bounds.as_slice()
        {
            if Some(trait_) == lang_items.Fn()
                || Some(trait_) == lang_items.FnMut()
                || Some(trait_) == lang_items.FnOnce()
            {
                assert_eq!(args.len(), 2); // one for self, one for parameters
                let self_ty = &args[0];
                let parameters = args[1].tuple_fields(db);
                let output = self_ty
                    .normalize_trait_assoc_type(db, args, lang_items.FnOnceOutput().unwrap())
                    .expect("No output for fn");
                let output = output.resolve_associated_type(db);

                return SpecialImplBounds::Func(output, parameters);
            } else if Some(trait_) == lang_items.Future() {
                assert_eq!(args.len(), 1); // one for self
                let self_ty = &args[0];
                let output = self_ty
                    .normalize_trait_assoc_type(db, args, lang_items.FutureOutput().unwrap())
                    .expect("No output for fn");
                let output = output.resolve_associated_type(db);

                return SpecialImplBounds::Future(output);
            }

            //*
            let param_ty = param.ty(db);
            if !self.with_self_in_cs(trait_)
                && let GenericDef::Function(f) = param.parent(db)
                && !includes_type_in_type(&f.ret_type(db), db, &|ty| ty == &param_ty)
                && f.params_without_self(db)
                    .iter()
                    .any(|p| includes_type_in_type(p.ty(), db, &|ty| ty == &param_ty))
            // TODO: consider generic params
            {
                //self.includes_type_in_generic_params_cs_constraints()
                return SpecialImplBounds::ArgOnlyTrait(param_ty);
            }
            // */
        }

        SpecialImplBounds::None
    }

    pub fn trait_itf_cs(&self, t: Trait) -> String {
        if Some(t) == self.lang_items.Future() {
            return "r2CsRuntime.RustTask".into();
        }
        let mut path = self.module_class_cs(t.module(self.db));
        path.push('.');
        path.push_str(&names::trait_name(t.name(self.db).as_str()));
        path
    }

    pub fn module_class_cs(&self, module: Module) -> String {
        if let Some(parent) = module.parent(self.db) {
            if matches!(
                module.definition_source(self.db).value,
                hir::ModuleSource::BlockExpr(_)
            ) && !matches!(
                parent.definition_source(self.db).value,
                hir::ModuleSource::BlockExpr(_)
            ) {
                // end of block module. return block_N
                return self.mod_simple_name(module);
            }
            let mut path = self.module_class_cs(parent);
            path.push('.');
            path.push_str(&self.mod_simple_name(module));
            path
        } else {
            let mut path = String::new();
            /*
            path.push_str("global::");
            path.push_str(&self.root_namespace);
            path.push_str(".");
            // */
            path.push_str(self.mod_simple_name(module).as_str());
            path
        }
    }

    pub fn extract_generic_args(
        &self,
        generic_def: impl Copy + HasContainer + DebugDisplay<'db> + Into<GenericDef>,
        args: Vec<(Symbol, Type<'db>)>,
    ) -> (Option<Vec<Type<'db>>>, Vec<Type<'db>>) {
        let db = self.db;
        let container = generic_def.container(self.db);
        let resolved_as_generic_def = generic_def.into();

        let resolved_as_generic_def_len =
            generic_types(&resolved_as_generic_def.params0(self.db)).count();

        let parent_def = match container {
            ItemContainer::Trait(trait_) => Some(GenericDef::Trait(trait_)),
            ItemContainer::Impl(impl_) => Some(GenericDef::Impl(impl_)),
            ItemContainer::Module(_) => None,
            ItemContainer::ExternBlock(_) => None,
            ItemContainer::Crate(_) => None,
        };

        let parent_params_len = parent_def
            .map(|def| def.params0(self.db))
            .as_deref()
            .map(generic_types)
            .into_iter()
            .flatten()
            .count();
        // implicit are replacement parameter for impl trait
        let self_params = generic_types(&resolved_as_generic_def.params0(self.db))
            .filter(|x| !x.is_implicit(self.db))
            .collect::<Vec<_>>();
        let self_params_len = self_params.len();
        let implicit_args_len = generic_types(&resolved_as_generic_def.params0(self.db))
            .filter(|x| x.is_implicit(self.db))
            .count();

        assert_eq!(
            args.len(),
            parent_params_len + self_params_len,
            "container: {container:?}, params: {self_params_len}, parent_params: {parent_params_len}, f: {f}",
            f = generic_def.debug_display(self.db),
        );

        assert_eq!(
            self_params_len + implicit_args_len,
            resolved_as_generic_def_len,
            "container: {container:?}, params: {self_params_len}, parent_params: {parent_params}, sum: {sum}, f: {f}, filtered_args: {implicit_args_len}",
            parent_params = parent_params_len,
            sum = self_params_len + parent_params_len,
            f = generic_def.debug_display(self.db),
        );

        let (parent_params, self_args) = {
            let mut parent_params = { args };
            let self_args = parent_params.drain(parent_params_len..).collect::<Vec<_>>();
            (parent_params, self_args)
        };

        assert!(self_params.iter().zip(self_args.iter()).all(
            |(param, (arg_symbol, _arg_type))| { param.name(self.db).symbol() == arg_symbol }
        ));

        let implicit_args = iter::repeat_n(
            hir::Type::error(self.db, resolved_as_generic_def.module(db).krate(db)),
            implicit_args_len,
        );

        let self_type_args = (self_args.into_iter().map(|(_, ty)| ty))
            .chain(implicit_args)
            .collect();
        let parent_type_args = (parent_params.into_iter().map(|(_, ty)| ty)).collect();

        (parent_def.map(|_| parent_type_args), self_type_args)
    }

    pub fn const_path_cs(&self, adt: hir::Const) -> String {
        let mut path = self.module_class_cs(adt.module(self.db));
        path.push('.');
        path.push_str(&names::const_name(
            adt.name(self.db)
                .as_ref()
                .map(Name::as_str)
                .unwrap_or_else(|| {
                    eprintln!("Unnamed const at of {}", path);
                    "(unnamed_const)"
                }),
        ));
        path
    }

    pub fn adt_name_cs(&self, adt: hir::Adt) -> String {
        let mut path = self.module_class_cs(adt.module(self.db));
        path.push('.');
        path.push_str(&names::struct_name(adt.name(self.db).as_str()));
        path
    }

    pub fn enum_variant_cs2(&self, v: hir::EnumVariant, generic: Vec<Type<'db>>) -> String {
        let mut path =
            self.rust_type_to_cs(&Adt::Enum(v.parent_enum(self.db)).ty_with_args(self.db, generic));
        path.push('.');
        path.push_str(&names::variant_name(v.name(self.db).as_str()));
        path
    }

    pub fn constructable_name_cs(&self, c: &Constructable<'db>) -> String {
        match c.def {
            ConstructableDef::Struct(s) => {
                self.rust_type_to_cs(&Adt::Struct(s).ty_with_args(self.db, c.args.clone()))
            }
            ConstructableDef::EnumVariant(v) => self.enum_variant_cs2(v, c.args.clone()),
            //hir::Variant::Union(u) => {
            //    self.rust_type_to_cs(&Adt::Union(u).ty_with_args(self.db, c.args.clone()))
            //}
        }
    }
}

pub trait CsPathWithArgsMember {
    fn cs_path(self, code_gen: &CodeGenerator<'_>) -> String;
}

macro_rules! cs_path {
    ($ty: ty => $f: ident) => {
        impl CsPathWithArgsMember for $ty {
            fn cs_path(self, code_gen: &CodeGenerator<'_>) -> String {
                code_gen.$f(self)
            }
        }
    };
}

cs_path!(hir::Adt => adt_name_cs);
cs_path!(hir::Trait => trait_itf_cs);

impl<'db> CodeGenerator<'db> {
    pub fn cs_path_with_args(
        &self,
        value: impl CsPathWithArgsMember + Into<hir::GenericDef> + Copy,
        args: impl IntoIterator<Item = impl Into<Option<hir::Type<'db>>>>,
    ) -> String {
        let as_def = value.into();

        generic_args(
            value.cs_path(self),
            self.map_cs_type_param_source(
                &self.generic_params_cs_sources(as_def),
                &args
                    .into_iter()
                    .filter_map(|x| x.into())
                    .collect::<Vec<_>>(),
            ),
        )
    }
}

pub fn includes_type_in_type<'db>(
    ty: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
    cond: &impl Fn(&hir::Type<'db>) -> bool,
) -> bool {
    let ty = &ty.resolve_associated_type(db);

    if cond(ty) {
        return true;
    }

    if let Some((inner, _mutability)) = ty.as_reference() {
        includes_type_in_type(&inner, db, cond)
    } else if let Some((inner, _)) = ty.as_raw_ptr() {
        includes_type_in_type(&inner, db, cond)
    } else if let Some(inner) = ty.as_slice() {
        includes_type_in_type(&inner, db, cond)
    } else if let Some(inner) = ty.as_array_unsized(db) {
        includes_type_in_type(&inner, db, cond)
    } else if ty.is_tuple() {
        let fields = ty.tuple_fields(db);
        (fields.iter()).any(|t| includes_type_in_type(t, db, cond))
    } else if let Some((_, args)) = ty.as_adt_with_args() {
        args.iter()
            .flatten()
            .any(|t| includes_type_in_type(t, db, cond))
    } else if let Some(mut traits) = ty.as_impl_traits_with_params(db) {
        // items_with_supertraits filter removes marker traits including lang items like Send, Sized
        traits.retain_mut(|(t, _)| t.items_with_supertraits(db).is_empty());
        traits.retain_mut(|&mut (t, _)| !ignored_trait(t, db));

        if traits.len() > 1 {
            eprintln!(
                "Multiple traits are used(includes_type_in_type): {} ({traits:?})",
                ty.debug_display(db),
                traits = traits
                    .iter()
                    .map(|(t, _)| t.debug_display(db).to_string())
                    .collect::<Vec<_>>(),
            );
        }

        if traits.is_empty() {
            false
        } else {
            let (_, args) = { traits }.swap_remove(0);

            args.iter()
                .skip(1)
                .flatten()
                .any(|x| includes_type_in_type(x, db, cond))
        }
    } else {
        false
    }
}

pub fn ignored_trait(trait_: hir::Trait, db: &dyn HirDatabase) -> bool {
    let lang_item = LangItems::new(db, trait_.krate(db));
    Some(trait_) == lang_item.Sized()
        || Some(trait_) == lang_item.MetaSized()
        || Some(trait_) == lang_item.Send()
        || Some(trait_) == lang_item.Sync()
        || Some(trait_) == lang_item.Unpin()
        || Some(trait_) == lang_item.Copy()
}

pub fn is_omit_trait_assoc_type(db: &dyn HirDatabase, alias: hir::TypeAlias) -> bool {
    let trait_ = match alias.container(db) {
        ItemContainer::Trait(t) => t,
        _ => panic!(),
    };
    if trait_.name(db).symbol() == &sym::IntoIterator && alias.name(db).symbol() == &sym::IntoIter {
        return true;
    }
    if trait_.name(db).as_str() == "IoTrait"
        && matches!(
            alias.name(db).as_str(),
            "DirEntry" | "ReadDirStream" | "FileStream"
        )
    {
        return true;
    }
    false
}
