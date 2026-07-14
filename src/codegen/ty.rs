use super::{CodeGenerator, generic_args, names};
use crate::codegen::constructable::{Constructable, ConstructableDef};
use crate::codegen::simple_extensions::{TraitExt, TypeExt as _};
/// Converts Rust HIR types to C# type strings.
use hir::db::HirDatabase;
use hir::{
    Adt, AssocItem, BuiltinType, GenericDef, GenericSubstitution, ItemContainer,
    MethodViolationCode, Module, Name, Symbol, Trait, Type,
};
use hir::{HasContainer, HasCrate, HirDisplay, sym};
use itertools::Either;
use ra_internal::*;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Not;
use tracing::*;

impl<'db> CodeGenerator<'db> {
    pub fn rust_type_to_cs(&self, ty: &Type<'db>) -> String {
        self.rust_type_to_cs_inner(ty, true)
    }

    fn rust_type_to_cs_inner(&self, ty: &Type<'db>, apply_special: bool) -> String {
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

            let cs_path = self.adt_name_cs(adt);
            let param_sources = self.generic_params_cs_sources(&GenericDef::from(adt).params(db));
            let type_args = self.map_type_param_source(
                &param_sources,
                &args.into_iter().flatten().collect::<Vec<_>>(),
            );

            if type_args.is_empty() {
                cs_path
            } else {
                format!("{}<{}>", cs_path, type_args.join(", "))
            }
        } else if let Some(trait_) = ty.as_dyn_trait() {
            // dyn Trait → T_TraitName (dyn interface)
            self.trait_itf_cs(trait_)
        } else if let Some(param) = ty.as_type_param(db) {
            if apply_special && let Some(cs) = self.special_types.borrow().get(&param) {
                return cs.format(|t| self.rust_type_to_cs(t));
            }

            // Generic parameter
            let name = param.name(db).as_str().to_string();
            if name == "Self" {
                "P_Self".to_string()
            } else if !param.is_implicit(db) {
                names::generic_param(&name)
            } else {
                format!("/* implicit */ {}", self.impl_ty_param_id.id_name(&param))
            }
        } else if let Some((param, aliases)) = ty.as_assoc_of_type_param(db) {
            let mut type_name = if param.is_implicit(db) && param.name(db) == sym::Self_ {
                "A".to_string()
            } else {
                self.rust_type_to_cs_inner(&param.ty(db), false)
            };

            for alias in aliases {
                type_name.push('_');
                type_name.push_str(alias.name(db).as_str());
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
                let rs_generic_args = args.into_iter().flatten().collect::<Vec<_>>();

                let param_sources =
                    self.generic_params_cs_sources(&GenericDef::from(trait_).params(db));
                let mut cs_generic_args =
                    self.map_type_param_source(&param_sources, &rs_generic_args);
                for alias in trait_.assoc_types(db) {
                    if is_omit_trait_assoc_type(db, alias) {
                        continue;
                    }
                    match ty
                        .normalize_trait_assoc_type(db, &rs_generic_args, alias)
                        .map(|x| x.resolve_associated_type(db))
                    {
                        None => {
                            eprintln!(
                                "Failed to resolve type {alias} of {ty}",
                                alias = alias.debug_display(self.db),
                                ty = ty.debug_display(self.db)
                            );
                            cs_generic_args.push("void/* Type */".into());
                        }
                        Some(type_) => {
                            cs_generic_args.push(self.rust_type_to_cs(&type_));
                        }
                    }
                }
                generic_args(self.trait_itf_cs(trait_), cs_generic_args)
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
            "Instant" => Some("long".to_string()), // ticks
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
    pub fn generic_params_cs(&self, params: &[hir::GenericParam]) -> (Vec<String>, Vec<String>) {
        self.register_special_impls(params);
        let sources = self.generic_params_cs_sources(params);
        let instances = generic_types(params)
            .map(|param| param.ty(self.db))
            .collect::<Vec<_>>();
        let type_params = self.map_type_param_source(&sources, &instances);
        let constraints = self.generic_params_cs_constraints(&sources, params);

        (type_params, constraints)
    }
}

pub enum CsTypeParamSource {
    TypeParam(usize),
    AliasOfParam(usize, Vec<hir::TypeAlias>),
}

impl CsTypeParamSource {
    fn index(&self) -> usize {
        match *self {
            CsTypeParamSource::TypeParam(idx) => idx,
            CsTypeParamSource::AliasOfParam(idx, _) => idx,
        }
    }
}

fn generic_types(params: &[hir::GenericParam]) -> impl Iterator<Item = hir::TypeParam> {
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
                    let cs_type = if output.is_unit() {
                        if parameters.is_empty() {
                            delayed_format!("global::System.Action")
                        } else {
                            delayed_format!("global::System.Action<", join(parameters, ", "), ">")
                        }
                    } else {
                        //let output = self.rust_type_to_cs(&output);
                        let mut types = parameters;
                        types.push(output);
                        //format!("global::System.Func<{}>", types.join(", "))
                        delayed_format!("global::System.Func<", join(types, ", "), ">")
                    };

                    //eprintln!("{param:?}: {cs_type:?}");
                    self.special_types.borrow_mut().insert(param, cs_type);
                }
                SpecialImplBounds::Future(output) => {
                    let cs_type = delayed_format!("r2CsRuntime.RustTask<", output, ">");

                    self.special_types.borrow_mut().insert(param, cs_type);
                }
                SpecialImplBounds::ArgOnlyTrait(param_type) => {
                    let traits = param
                        .trait_bounds_of_nested_type_with_args(&param_type, db)
                        .expect_left("Bounds of ArgOnlyTrait is projection");
                    let (trait_, ref args) = traits[0];

                    assert!(!self.with_self_in_cs(trait_));

                    let args = self
                        .trait_type_args(trait_, &args[0], &args[1..])
                        //.map(|t| self.rust_type_to_cs(&t))
                        .collect::<Vec<_>>();

                    self.special_types.borrow_mut().insert(
                        param,
                        if args.is_empty() {
                            delayed_format!(str(&self.trait_itf_cs(trait_)))
                        } else {
                            delayed_format!(
                                str(&self.trait_itf_cs(trait_)),
                                "<",
                                join(args, ","),
                                ">"
                            )
                        },
                    );
                }
                SpecialImplBounds::None => {}
            }
        }
    }

    pub fn generic_params_cs_sources(
        &self,
        params: &[hir::GenericParam],
    ) -> Vec<CsTypeParamSource> {
        let db = self.db;

        let mut type_params = Vec::new();

        for (index, param) in generic_types(params).enumerate() {
            if param.is_implicit(db) && param.name(db) == sym::Self_ {
                continue;
            }

            if param.is_unstable(db) {
                continue;
            }

            if param
                .default(db)
                .and_then(|x| x.as_adt())
                .map(|x| x.name(db))
                .as_ref()
                .map(|x| x.as_str())
                == Some("RandomState")
            {
                continue;
            }

            if !self.special_type_param(param).is_special_impl() {
                type_params.push(CsTypeParamSource::TypeParam(index));
            }

            for instance in collect_assoc_type_params(param, param.ty(db), self.db) {
                let (param_instance, aliases) = instance.as_assoc_of_type_param(db).unwrap();

                assert_eq!(param_instance, param);

                type_params.push(CsTypeParamSource::AliasOfParam(index, aliases.clone()));
            }
        }

        type_params
    }

    pub fn resolve_cs_type_param_source(
        &self,
        source: &CsTypeParamSource,
        generic_types: &[Type<'db>],
    ) -> Type<'db> {
        match *source {
            CsTypeParamSource::TypeParam(i) => generic_types[i].clone(),
            CsTypeParamSource::AliasOfParam(i, ref alias) => {
                generic_types[i].new_associated_type(alias, self.db)
            }
        }
    }

    pub fn map_type_param_source(
        &self,
        params: &[CsTypeParamSource],
        instances: &[Type<'db>],
    ) -> Vec<String> {
        params
            .iter()
            .map(|x| self.resolve_cs_type_param_source(x, instances))
            .map(|x| self.rust_type_to_cs(&x))
            .collect()
    }

    pub fn generic_params_cs_constraints(
        &self,
        cs_sources: &[CsTypeParamSource],
        params: &[hir::GenericParam],
    ) -> Vec<String> {
        let db = self.db;

        let mut constraints = Vec::new();

        let type_prams = generic_types(params).collect::<Vec<_>>();
        let types = type_prams.iter().map(|x| x.ty(db)).collect::<Vec<_>>();

        for source in cs_sources {
            let param_type = self.resolve_cs_type_param_source(source, &types);
            let param = type_prams[source.index()];

            let name = self.rust_type_to_cs(&param_type);

            match param.trait_bounds_of_nested_type_with_args(&param_type, db) {
                Either::Right(_) => {
                    // nothing to do for projection
                }
                Either::Left(traits) => {
                    if !traits.is_empty() {
                        let mut cs_constraints = vec![];
                        for &(trait_, ref args) in &traits {
                            let args = self
                                .trait_type_args(
                                    trait_,
                                    &args[0],
                                    &args[(!self.with_self_in_cs(trait_)) as usize..],
                                )
                                .map(|t| self.rust_type_to_cs(&t))
                                .collect::<Vec<_>>();

                            let constraint = self::generic_args(self.trait_itf_cs(trait_), args);
                            cs_constraints.push(constraint);
                        }
                        constraints.push(format!("{name} : {}", cs_constraints.join(", ")));
                    }
                }
            }
        }

        constraints
    }

    fn trait_type_args(
        &self,
        trait_: Trait,
        self_ty: &Type<'db>,
        args: &[Type<'db>],
    ) -> impl Iterator<Item = Type<'db>> {
        let generic_args = args.iter().cloned();
        let assoc_types = trait_
            .assoc_types(self.db)
            .into_iter()
            .filter(|&alias| !is_omit_trait_assoc_type(self.db, alias))
            .map(|alias| {
                self_ty
                    .normalize_trait_assoc_type(self.db, &[], alias)
                    .unwrap()
                    .resolve_associated_type(self.db)
            });

        generic_args.chain(assoc_types)
    }
}

fn collect_assoc_type_params<'db>(
    param: hir::TypeParam,
    type_: hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> impl Iterator<Item = Type<'db>> + 'db {
    let traits = param.trait_bounds_with_args(db);
    collect_assoc_type_params_impl(param, traits, type_, db)
}

fn collect_assoc_type_params_impl<'db>(
    param: hir::TypeParam,
    traits: Vec<(hir::Trait, Vec<hir::Type<'db>>)>,
    outer_instance: hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> impl Iterator<Item = Type<'db>> + 'db {
    let span = tracing::info_span!(
        "collect_assoc_type_params_impl",
        assoc_ty = %outer_instance.debug_display(db),
    );
    traits
        .into_iter()
        .flat_map(move |(trait_, _)| trait_.assoc_types(db))
        .flat_map(move |alias| {
            let _scope = tracing::info_span!(
                parent: &span,
                "alias",
                alias = alias.name(db).as_str(),
            )
            .entered();
            if is_omit_trait_assoc_type(db, alias) {
                return None;
            }
            let Some(instance) = outer_instance.normalize_trait_assoc_type(db, &[], alias) else {
                let target = alias.module(db).krate(db).to_display_target(db);
                panic!("unable to resolve {alias:?} of {type}",
                       alias = alias.name(db).as_str(),
                       type = outer_instance.display(db, target),
                );
            };
            if let Some((param_instance, alias_instance)) = instance.as_associated_type()
                && param_instance == outer_instance
                && alias_instance == alias
            {
                match param.trait_bounds_of_nested_type_with_args(&instance, db) {
                    Either::Right(_projected) => {
                        panic!("Projection should be resolved by normalize_trait_assoc_type")
                    }
                    Either::Left(traits) => Some(
                        std::iter::once(instance.clone()).chain(
                            traits
                                .is_empty()
                                .not()
                                .then(|| {
                                    Box::new(collect_assoc_type_params_impl(
                                        param, traits, instance, db,
                                    ))
                                        as Box<dyn Iterator<Item = Type<'db>> + 'db>
                                })
                                .into_iter()
                                .flatten(),
                        ),
                    ),
                }
            } else {
                None
            }
        })
        .flatten()
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
        let _scope = tracing::info_span!(
            "special_type_param",
            param = %param.debug_display(self.db),
        )
        .entered();
        let db = self.db;

        if let bounds = param
            .trait_bounds_with_args(db)
            .into_iter()
            .filter(|&(t, _)| !ignored_trait(t, db))
            .collect::<Vec<_>>()
            && let &[(trait_, ref args)] = bounds.as_slice()
        {
            if Some(trait_) == self.lang_items.Fn()
                || Some(trait_) == self.lang_items.FnMut()
                || Some(trait_) == self.lang_items.FnOnce()
            {
                assert_eq!(args.len(), 2); // one for self, one for parameters
                let self_ty = &args[0];
                let parameters = args[1].tuple_fields(db);
                let output = self_ty
                    .normalize_trait_assoc_type(
                        db,
                        args,
                        self.lang_items.FnOnceOutput().unwrap().into(),
                    )
                    .expect("No output for fn");
                let output = output.resolve_associated_type(db);

                return SpecialImplBounds::Func(output, parameters);
            } else if Some(trait_) == self.lang_items.Future() {
                assert_eq!(args.len(), 1); // one for self
                let self_ty = &args[0];
                let output = self_ty
                    .normalize_trait_assoc_type(
                        db,
                        args,
                        self.lang_items.FutureOutput().unwrap().into(),
                    )
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

    pub fn trait_itf_cs2(&self, trait_: Trait, args: Vec<Type<'db>>) -> String {
        let db = self.db;

        let mut cs_type_params = Vec::new();

        for x in &args {
            cs_type_params.push(self.rust_type_to_cs_inner(x, false));
        }

        if !self.with_self_in_cs(trait_) && !cs_type_params.is_empty() {
            cs_type_params.remove(0);
        }

        let self_ty = &args[0];
        for alias in trait_.assoc_types(db) {
            if is_omit_trait_assoc_type(db, alias) {
                continue;
            }
            match self_ty
                .normalize_trait_assoc_type(db, &args, alias)
                .map(|x| x.resolve_associated_type(db))
            {
                None => {
                    eprintln!(
                        "Failed to resolve type {alias} of {ty}",
                        alias = alias.debug_display(self.db),
                        ty = self_ty.debug_display(self.db)
                    );
                    cs_type_params.push(format!(
                        "void /* {alias} of {ty} */",
                        alias = alias.debug_display(self.db),
                        ty = self_ty.debug_display(self.db),
                    ));
                }
                Some(ty) => {
                    cs_type_params.push(self.rust_type_to_cs(&ty));
                }
            }
        }

        generic_args(self.trait_itf_cs(trait_), cs_type_params)
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

    pub fn params1(&self, types: &[Type<'db>], def: GenericDef) -> Vec<String> {
        let type_params = generic_types(&def.params(self.db))
            .enumerate()
            .map(|(i, _)| {
                (types.get(i).cloned()).unwrap_or_else(|| Type::error(self.db, self.krate.into()))
            })
            .collect::<Vec<_>>();
        let param_sources = self.generic_params_cs_sources(&def.params(self.db));
        self.map_type_param_source(&param_sources, &type_params)
    }

    fn params(&self, types: &HashMap<&Symbol, &Type<'db>>, def: GenericDef) -> Vec<String> {
        let type_params = generic_types(&def.params(self.db))
            .map(|x| {
                types
                    .get(x.name(self.db).symbol())
                    .copied()
                    .cloned()
                    .or_else(|| x.default(self.db))
                    .unwrap_or_else(|| Type::error(self.db, self.krate.into()))
            })
            .collect::<Vec<_>>();
        let param_sources = self.generic_params_cs_sources(&def.params(self.db));
        self.map_type_param_source(&param_sources, &type_params)
    }

    pub fn fn_path_cs1(&self, f: hir::Function, args: &[(Symbol, Type<'db>)]) -> String {
        let args_by_symbol = args.iter().map(|(x, y)| (x, y)).collect::<HashMap<_, _>>();

        fn resolve_param<'a, 'db>(
            db: &'db dyn HirDatabase,
            ty: &'a Type<'db>,
            args_by_symbol: &HashMap<&Symbol, &'a Type<'db>>,
        ) -> &'a Type<'db> {
            if let Some(param) = ty.as_type_param(db)
                && let Some(adt) = args_by_symbol.get(param.name(db).symbol())
            {
                adt
            } else {
                ty
            }
        }

        match f.container(self.db) {
            ItemContainer::Impl(impl_)
                if let Some(adt) =
                    resolve_param(self.db, &impl_.self_ty(self.db), &args_by_symbol).as_adt() =>
            {
                if f.self_param(self.db).is_some() {
                    // TODO: explicit types
                    let num_params = f.num_params(self.db);
                    let mut path = String::new();
                    path.push_str("((");
                    {
                        let mut peekable = (0..num_params).peekable();
                        while let Some(index) = peekable.next() {
                            write!(path, "_p_{}", index).unwrap();
                            if peekable.peek().is_some() {
                                path.push_str(", ");
                            }
                        }
                    }
                    path.push_str(") => _p_0.");
                    path.push_str(&self.function_name(f));
                    path = generic_args(path, self.params(&args_by_symbol, f.into()));
                    path.push('(');
                    {
                        let mut peekable = (1..num_params).peekable();
                        while let Some(index) = peekable.next() {
                            write!(path, "_p_{}", index).unwrap();
                            if peekable.peek().is_some() {
                                path.push_str(", ");
                            }
                        }
                    }
                    path.push_str("))");

                    path
                } else {
                    let mut path = self.adt_name_cs(adt);
                    path = generic_args(path, self.params(&args_by_symbol, adt.into()));
                    path.push('.');
                    path.push_str(&self.function_name(f));
                    path = generic_args(path, self.params(&args_by_symbol, f.into()));
                    path
                }
            }
            ItemContainer::Impl(impl_)
                if let Some(primitive) =
                    resolve_param(self.db, &impl_.self_ty(self.db), &args_by_symbol)
                        .as_builtin() =>
            {
                let mut path = String::from(primitive.name().as_str());
                path.push('.');
                path.push_str(&self.function_name(f));
                path
            }
            ItemContainer::Module(module) => {
                let mut path = self.module_class_cs(module);
                path.push('.');
                path.push_str(&self.function_name(f));
                path
            }
            ItemContainer::Trait(trait_) => {
                let mut path = self.trait_itf_cs(trait_);
                path.push('.');
                path.push_str(&self.function_name(f));
                path
            }
            //ItemContainer::ExternBlock(_) => {}
            //ItemContainer::Crate(_) => {}
            ItemContainer::Impl(impl_) => {
                eprintln!(
                    "Unsupported function type with self: {:?}",
                    impl_.self_ty(self.db)
                );
                let mut path = self.module_class_cs(f.module(self.db));
                path.push_str(&format!(
                    "/*Unsupported impl with {}*/",
                    impl_.self_ty(self.db).debug_display(self.db)
                ));
                path.push('.');
                path.push_str(&self.function_name(f));
                path
            }
            unsupported => {
                eprintln!("Unsupported function type: {:?}", unsupported);
                let mut path = self.module_class_cs(f.module(self.db));
                path.push('.');
                path.push_str(&self.function_name(f));
                path
            }
        }
    }

    pub fn extract_generic_args(
        &self,
        generic_def: impl Copy + HasContainer + DebugDisplay<'db> + Into<GenericDef>,
        substitution: GenericSubstitution<'db>,
    ) -> Vec<Type<'db>> {
        let container = generic_def.container(self.db);
        let resolved_as_generic_def = generic_def.into();

        let parent_def = match container {
            ItemContainer::Trait(trait_) => Some(GenericDef::Trait(trait_)),
            ItemContainer::Impl(impl_) => Some(GenericDef::Impl(impl_)),
            ItemContainer::Module(_) => None,
            ItemContainer::ExternBlock(_) => None,
            ItemContainer::Crate(_) => None,
        };
        let args = substitution.types(self.db);
        let parent_params = parent_def
            .map(|def| def.params(self.db))
            .as_deref()
            .into_iter()
            .flat_map(generic_types)
            .collect::<Vec<_>>();
        let self_params = generic_types(&resolved_as_generic_def.params(self.db))
            .filter(|x| {
                // implicit && missing => replacement parameter for impl trait
                !x.is_implicit(self.db) || !x.name(self.db).is_missing()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            args.len(),
            parent_params.len() + self_params.len(),
            "container: {container:?}, args: {args}, params: {self_params}, parent_params: {parent_params}, sum: {sum}, f: {f}",
            args = args.len(),
            parent_params = parent_params.len(),
            self_params = self_params.len(),
            sum = parent_params.len() + self_params.len(),
            f = generic_def.debug_display(self.db),
        );

        //let self_args = &args[parent_params.len()..];
        let self_args = {
            let mut tmp = { args };
            tmp.drain(0..parent_params.len());
            tmp
        };

        assert!(self_params.iter().zip(self_args.iter()).all(
            |(param, (arg_symbol, _arg_type))| { param.name(self.db).symbol() == arg_symbol }
        ));

        self_args.into_iter().map(|(_, ty)| ty).collect()
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

fn is_omit_trait_assoc_type(db: &dyn HirDatabase, alias: hir::TypeAlias) -> bool {
    let trait_ = match alias.container(db) {
        ItemContainer::Trait(t) => t,
        _ => panic!(),
    };
    if trait_.name(db).symbol() == &sym::IntoIterator && alias.name(db).symbol() == &sym::IntoIter {
        return true;
    }
    false
}

impl<'db> CodeGenerator<'db> {
    pub fn with_self_in_cs(&self, trait_: Trait) -> bool {
        if let Some(violations) = self.dyn_compatibility_all_violations_alt(trait_)
            && !violations.iter().all(|v| self.allowed_violation(v))
        {
            return true;
        }

        false
    }

    fn allowed_violation(&self, v: &DynCompatibilityViolation) -> bool {
        match v {
            DynCompatibilityViolation::SizedSelf => true, // interfaces are sized
            DynCompatibilityViolation::SelfReferential => false,
            DynCompatibilityViolation::Method(_f, v) => match v {
                MethodViolationCode::Generic => true, // Generic interface method is native in C#
                MethodViolationCode::ReferencesImplTraitInTrait => true, // return place impl trait
                MethodViolationCode::AsyncFn => true, // async fns are return place impl trait

                MethodViolationCode::StaticMethod => false, // TODO: Static method helper system
                MethodViolationCode::ReferencesSelfInput => false,
                MethodViolationCode::ReferencesSelfOutput => false,
                MethodViolationCode::WhereClauseReferencesSelf => false,
                MethodViolationCode::UndispatchableReceiver => false,
            },
            DynCompatibilityViolation::AssocConst(_) => false,
            //DynCompatibilityViolation::GAT(_) => false,
            //DynCompatibilityViolation::HasNonCompatibleSuperTrait(_) => false,
        }
    }

    fn dyn_compatibility_all_violations_alt(
        &self,
        trait_: Trait,
    ) -> Option<Vec<DynCompatibilityViolation>> {
        let _scope = tracing::info_span!(
            "dyn_compatibility_all_violations_alt",
            trait = %trait_.debug_display(self.db),
        )
        .entered();
        let db = self.db;
        let mut violations: Vec<DynCompatibilityViolation> = vec![];
        let mut cb = |violation: DynCompatibilityViolation| violations.push(violation);

        if trait_
            .all_supertraits(db)
            .iter()
            .any(|&x| Some(x.into()) == self.lang_items.Sized())
        {
            cb(DynCompatibilityViolation::SizedSelf);
        }

        if predicates_reference_self(self, self.db, trait_) {
            cb(DynCompatibilityViolation::SelfReferential);
        }

        // TODO: check for SelfReferential

        for assoc_item in trait_.items_with_supertraits(db) {
            // remove associated items with Self: Sized, but does not exclude Trait : Sized
            if generics_require_sized_self(assoc_item, db) {
                trace!(
                    "ignored item {assoc_item} of {trait} since guarded by Sized",
                    assoc_item = assoc_item.name(db).unwrap().as_str(),
                    trait = trait_.debug_display(self.db),
                );
                continue;
            }

            match assoc_item {
                AssocItem::Const(it) => cb(DynCompatibilityViolation::AssocConst(it.into())),
                AssocItem::Function(it) => {
                    let _scope = tracing::info_span!(
                        "dyn_compatibility_all_violations_alt for fn",
                        trait = %trait_.debug_display(self.db),
                        f = %it.debug_display(self.db),
                    )
                    .entered();
                    self.virtual_call_violations_for_method(it, &mut |mvc| {
                        cb(DynCompatibilityViolation::Method(it, mvc))
                    })
                }
                AssocItem::TypeAlias(_it) => {}
            }
        }

        fn predicates_reference_self<'db>(
            this: &CodeGenerator<'db>,
            db: &'db dyn HirDatabase,
            trait_: Trait,
        ) -> bool {
            trait_.predicate_types(db).any(|type_| {
                includes_type_in_type(&type_, db, &|ty| {
                    if let Some(param) = ty.as_type_param(this.db) {
                        // Generic parameter Self
                        *param.name(this.db).symbol() == sym::Self_
                    } else {
                        false
                    }
                })
            })
        }

        fn generics_require_sized_self(assoc_item: AssocItem, db: &dyn HirDatabase) -> bool {
            let includes_sized = assoc_item.includes_self_sized_bounds(db);
            if includes_sized {
                trace!(
                "ignored item {assoc_item} of {trait} since guarded by Sized",
                    assoc_item = assoc_item.name(db).unwrap().as_str(),
                    trait = assoc_item.container(db).debug_display(db),
                );
            }
            includes_sized
        }

        violations.is_empty().not().then_some(violations)
    }

    fn virtual_call_violations_for_method<F>(&self, func: hir::Function, cb: &mut F)
    where
        F: FnMut(MethodViolationCode),
    {
        let db = self.db;

        if !func.has_self_param(db) {
            cb(MethodViolationCode::StaticMethod);
        }

        if func.is_async(db) {
            cb(MethodViolationCode::AsyncFn);
        }

        let is_self_ty = |ty: &Type<'db>| {
            if let Some(param) = ty.as_type_param(self.db) {
                // Generic parameter Self
                *param.name(self.db).symbol() == sym::Self_
            } else {
                false
            }
        };

        if func
            .params_without_self(db)
            .iter()
            .any(|x| includes_type_in_type(x.ty(), db, &is_self_ty))
        {
            cb(MethodViolationCode::ReferencesSelfInput);
        }

        if includes_type_in_type(&func.ret_type(db), db, &is_self_ty) {
            cb(MethodViolationCode::ReferencesSelfOutput);
        }

        let params = GenericDef::from(func).params(db);

        if self.includes_type_in_generic_params_cs_constraints(&params, &is_self_ty) {
            cb(MethodViolationCode::WhereClauseReferencesSelf);
        }
    }

    pub fn includes_type_in_generic_params_cs_constraints(
        &self,
        params: &[hir::GenericParam],
        cond: &impl Fn(&hir::Type<'db>) -> bool,
    ) -> bool {
        generic_types(params)
            .any(|param| self.includes_type_in_generic_param_cs_constraints(param, cond))
    }

    pub fn includes_type_in_generic_param_cs_constraints(
        &self,
        param: hir::TypeParam,
        cond: &impl Fn(&hir::Type<'db>) -> bool,
    ) -> bool {
        let db = self.db;

        let param_type = param.ty(db);
        collect_assoc_type_params(param, param_type.clone(), db).any(|_assoc_ty| {
            includes_type_in_type(&param_type, db, &cond)
                || match param.trait_bounds_of_nested_type_with_args(&param_type, db) {
                    Either::Right(projection) => includes_type_in_type(&projection, db, &cond),
                    Either::Left(traits) => traits.iter().any(|&(trait_, ref args)| {
                        self.trait_type_args(trait_, &args[0], &args[1..])
                            .any(|t| includes_type_in_type(&t, db, &cond))
                    }),
                }
        })
    }
}

fn includes_type_in_type<'db>(
    ty: &Type<'db>,
    db: &'db dyn HirDatabase,
    cond: &impl Fn(&Type<'db>) -> bool,
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

fn ignored_trait(trait_: hir::Trait, db: &dyn HirDatabase) -> bool {
    let lang_item = LangItems::new(db, trait_.krate(db));
    Some(trait_) == lang_item.Sized()
        || Some(trait_) == lang_item.MetaSized()
        || Some(trait_) == lang_item.Send()
        || Some(trait_) == lang_item.Sync()
        || Some(trait_) == lang_item.Unpin()
        || Some(trait_) == lang_item.Copy()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynCompatibilityViolation {
    SizedSelf,
    SelfReferential,
    Method(hir::Function, MethodViolationCode),
    AssocConst(hir::Const),
    //GAT(hir::TypeAlias),
    // This doesn't exist in rustc, but added for better visualization
    //HasNonCompatibleSuperTrait(hir::Trait),
}
