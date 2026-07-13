mod type_map;

use super::{CodeGenerator, generic_args, names};
/// Converts Rust HIR types to C# type strings.
use hir::db::HirDatabase;
use hir::next_solver::GenericArgs;
use hir::{
    Adt, AssocItem, BuiltinType, GenericDef, GenericSubstitution, HasContainer, ItemContainer,
    Module, Name, Symbol, Trait, Type, sym,
};
use hir_ty::display::HirDisplay;
use hir_ty::dyn_compatibility::{DynCompatibilityViolation, MethodViolationCode};
use ide_db::base_db;
use itertools::Either;
use rustc_type_ir::inherent::IntoKind;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Not;
use tracing::*;

use crate::codegen::debug_display::DebugDisplay;
use crate::codegen::simple_extensions::TraitExt;
pub(crate) use type_map::TypeMap;

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

        let ty = {
            if self.type_map.is_empty() {
                ty
            } else {
                let (ty, env) = (ty.ns_ty(), ty.env());
                let ty = hir_ty::next_solver::fold::fold_tys(
                    hir_ty::next_solver::DbInterner::new_with(db, env.krate),
                    ty,
                    |ty| self.type_map.get(ty).unwrap_or(ty),
                );
                &Type::from_ty_env(ty, env)
            }
        };

        let ty = &self.resolve_assoc_of_impl(ty);

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
        //if let Some((inner, _size)) = ty.as_array(db) {
        if let rustc_type_ir::TyKind::Array(inner, _size) = ty.ns_ty().kind() {
            return format!("{}[]", self.rust_type_to_cs(&self.new_type(inner)));
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
        } else if let Some((param, aliases)) = rustc_ty::assoc_of_type_param(ty) {
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
        } else if ty.as_impl_traits(db).is_some() {
            use hir_ty::next_solver::ClauseKind;
            // Unfortunately hir crate does not provide us generic parameters of trait so we access
            // new solver's ty
            let traits = ty
                .ns_ty()
                .impl_trait_bounds(db)
                .unwrap()
                .into_iter()
                .filter_map(|pred| match pred.kind().skip_binder() {
                    ClauseKind::Trait(trait_ref) => {
                        Some((Trait::from(trait_ref.def_id().0), trait_ref.trait_ref.args))
                    }
                    _ => None,
                })
                // remove marker traits including lang items like Send, Sized
                .filter(|&(t, _)| !t.items_with_supertraits(db).is_empty())
                .collect::<Vec<_>>();
            if traits.len() > 1 {
                eprintln!("Multiple traits are used: {}", ty.debug_display(self.db));
            }
            if traits.is_empty() {
                eprintln!("empty impl traits: {:?}", ty);
                "object".to_string()
            } else {
                let (trait_, args) = traits[0];
                let rs_generic_args = self.generic_args_to_types(args).collect::<Vec<_>>();

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
                        .map(|x| self.resolve_assoc_of_impl(&x))
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
        } else if let rustc_type_ir::TyKind::Error(_) = ty.ns_ty().kind() {
            "object /* error type */".to_string()
        } else if ty.is_fn() || ty.is_closure() {
            // Closure types, fn pointers, etc. — use Action/Func
            "Action".to_string()
        } else if let normalized = self.resolve_assoc_of_impl(ty)
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
                ty = self.ty_to_str(ty.ns_ty()),
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
    params.iter().filter_map(|&x| match x {
        hir::GenericParam::TypeParam(param) => Some(param),
        _ => None,
    })
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
                let (param_instance, aliases) = rustc_ty::assoc_of_type_param(&instance).unwrap();

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
                self.new_alias_ty(&generic_types[i], alias)
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
                self.resolve_assoc_of_impl(
                    &self_ty
                        .normalize_trait_assoc_type(self.db, &[], alias)
                        .unwrap(),
                )
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
            if let Some((param_instance, alias_instance)) =
                rustc_ty::associated_type_of_some(&instance)
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
            .filter(|&(t, _)| Some(t.into()) != self.lang_items.Sized)
            .filter(|&(t, _)| Some(t.into()) != self.lang_items.Sync)
            .filter(|&(t, _)| Some(t.into()) != self.lang_items.Unpin)
            .filter(|&(t, _)| Some(t.into()) != self.lang_items.Copy)
            //.filter(|&(t, _)| Some(t.into()) != self.lang_items.Send)
            .collect::<Vec<_>>()
            && let &[(trait_, ref args)] = bounds.as_slice()
            && let trait_id = trait_.into()
        {
            if Some(trait_id) == self.lang_items.Fn
                || Some(trait_id) == self.lang_items.FnMut
                || Some(trait_id) == self.lang_items.FnOnce
            {
                assert_eq!(args.len(), 2); // one for self, one for parameters
                let self_ty = &args[0];
                let parameters = args[1].tuple_fields(db);
                let output = self_ty
                    .normalize_trait_assoc_type(
                        db,
                        args,
                        self.lang_items.FnOnceOutput.unwrap().into(),
                    )
                    .expect("No output for fn");
                let output = self.resolve_assoc_of_impl(&output);

                return SpecialImplBounds::Func(output, parameters);
            } else if Some(trait_id) == self.lang_items.Future {
                assert_eq!(args.len(), 1); // one for self
                let self_ty = &args[0];
                let output = self_ty
                    .normalize_trait_assoc_type(
                        db,
                        args,
                        self.lang_items.FutureOutput.unwrap().into(),
                    )
                    .expect("No output for fn");
                let output = self.resolve_assoc_of_impl(&output);

                return SpecialImplBounds::Future(output);
            }

            //*
            let param_ty = param.ty(db);
            if !self.with_self_in_cs(trait_)
                && let GenericDef::Function(f) = param.parent(db)
                && !self.includes_type_in_type(&f.ret_type(db), &|ty| ty == &param_ty)
                && f.params_without_self(db)
                    .iter()
                    .any(|p| self.includes_type_in_type(p.ty(), &|ty| ty == &param_ty))
            // TODO: consider generic params
            {
                //self.includes_type_in_generic_params_cs_constraints()
                return SpecialImplBounds::ArgOnlyTrait(param_ty);
            }
            // */
        }

        SpecialImplBounds::None
    }

    pub fn trait_itf_cs1(&self, trait_: Trait, args: GenericArgs<'db>) -> String {
        self.trait_itf_cs2(trait_, self.generic_args_to_types(args).collect())
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
                .map(|x| self.resolve_assoc_of_impl(&x))
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
        if Some(t.into()) == self.lang_items.Future {
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

    pub fn generic_args_to_types(
        &self,
        generic: GenericArgs<'db>,
    ) -> impl Iterator<Item = Type<'db>> {
        generic.as_slice().iter().filter_map(|x| match x.kind() {
            hir_ty::next_solver::GenericArgKind::Type(t) => Some(self.new_type(t)),
            _ => None,
        })
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

mod rustc_ty {
    //! Next solver ty related methods
    //!
    //! since rustc_type is NOT part of rustc_hir crate which is API boundary,
    //! we don't want to use those type generally

    use crate::codegen::CodeGenerator;
    use crate::codegen::ty::ty_param_ext::{ParsedProjection, parse_bounds_for};
    use crate::codegen::ty::{DebugDisplay, TyFromType};
    use hir::{Adt, Type};
    use hir_def::resolver::HasResolver;
    use hir_def::signatures::TypeAliasSignature;
    use hir_def::{
        AdtId, AssocItemId, GenericDefId, GenericParamId, ImplId, ItemContainerId, Lookup,
        TypeAliasId,
    };
    use hir_ty::display::HirDisplay;
    use hir_ty::next_solver::{
        AnyImplId, DbInterner, ErrorGuaranteed, GenericArg, GenericArgKind, GenericArgs,
        SolverDefId, Term, Ty,
    };
    use hir_ty::{GenericPredicates, ImplTraitId};
    use rustc_type_ir::inherent::IntoKind;

    use rustc_type_ir::{AliasTy, AliasTyKind, Interner, TyKind};
    use std::fmt::Debug;
    use tracing::{debug, trace};

    pub(super) trait TypeLike<'db>: Debug + Clone {
        fn ty(&self) -> Ty<'db>;
        fn from_type(t: Type<'db>) -> Self;
    }

    impl<'db> TypeLike<'db> for Ty<'db> {
        fn ty(&self) -> Ty<'db> {
            *self
        }

        fn from_type(t: Type<'db>) -> Self {
            t.ns_ty()
        }
    }

    impl<'db> TypeLike<'db> for Type<'db> {
        fn ty(&self) -> Ty<'db> {
            self.ns_ty()
        }

        fn from_type(t: Type<'db>) -> Self {
            t
        }
    }

    impl<'db> CodeGenerator<'db> {
        // This tries to resolve `<impl SomeTrait<Assoc = SomeType> as SomeTrait>::Assoc`
        pub(super) fn resolve_assoc_of_impl(&self, assoc_ty: &Type<'db>) -> Type<'db> {
            trace!("resolve_assoc_of_impl: {}", assoc_ty.debug_display(self.db));
            self.resolve_assoc_of_impl_impl(assoc_ty)
        }

        pub(super) fn new_alias_ty(
            &self,
            self_type: &Type<'db>,
            aliases: &[hir::TypeAlias],
        ) -> Type<'db> {
            let self_ty = self_type.ns_ty();
            let mut ty = self_ty;
            for &alias in aliases {
                let alias_id = alias.into();
                ty = Ty::new(
                    self.interner,
                    TyKind::Alias(
                        AliasTy::new_from_args(
                            self.interner,
                            AliasTyKind::Projection {
                                def_id: SolverDefId::TypeAliasId(alias_id),
                            },
                            GenericArgs::error_for_item(self.interner, alias_id.into()),
                        )
                        .with_replaced_self_ty(self.interner, ty),
                    ),
                );
            }
            Type::from_ty_env(ty, self_type.env())
        }

        #[tracing::instrument(skip(self))]
        pub(super) fn resolve_assoc_of_impl_impl<T: TypeLike<'db>>(&self, assoc_type: &T) -> T {
            let mut alias_list = vec![];
            let assoc_ty = assoc_type.ty();

            let self_ty = {
                let mut cur = assoc_ty;
                while let TyKind::Alias(
                    alias @ hir_ty::next_solver::AliasTy {
                        kind:
                            AliasTyKind::Projection {
                                def_id: SolverDefId::TypeAliasId(alias_id),
                            },
                        ..
                    },
                ) = cur.kind()
                {
                    alias_list.push(hir::TypeAlias::from(alias_id));
                    cur = alias.self_ty();
                }
                cur
            };

            if alias_list.is_empty() {
                return assoc_type.clone();
            }

            alias_list.reverse();

            let db = self.db;

            match self_ty.kind() {
                TyKind::Alias(self_ty_alias) => {
                    let AliasTyKind::Opaque { def_id } = self_ty_alias.kind else {
                        panic!(
                            "Tries to assoc but not assoc (self is not opaque): {assoc_ty}\nkind: {kind:?}",
                            assoc_ty =
                                assoc_ty.display(self.db, self.krate.to_display_target(self.db)),
                            kind = self_ty_alias.kind,
                        );
                        // return assoc_type.clone();
                    };
                    let bounds = def_id
                        .expect_opaque_ty()
                        .predicates(db)
                        .iter_instantiated_copied(self.interner, self_ty_alias.args.as_slice());
                    let resolver = match def_id.expect_opaque_ty().loc(db) {
                        ImplTraitId::ReturnTypeImplTrait(f, _) => f.resolver(db),
                        ImplTraitId::TypeAliasImplTrait(a, _) => a.resolver(db),
                    };

                    match parse_bounds_for(bounds, assoc_ty, &resolver, self.db) {
                        ParsedProjection::NoBounds => assoc_type.clone(),
                        ParsedProjection::Projection(ty) => T::from_type(ty),
                        ParsedProjection::Traits(traits) => {
                            eprintln!(
                                "type: {assoc_ty_rs}, traits: {traits:?}",
                                assoc_ty_rs = assoc_ty
                                    .display(self.db, self.krate.to_display_target(self.db)),
                                traits = traits
                                    .iter()
                                    .map(|x| x.0.debug_display(self.db).to_string())
                                    .collect::<Vec<_>>()
                            );
                            assoc_type.clone()
                        }
                    }
                }
                TyKind::Param(param) => {
                    let bounds =
                        GenericPredicates::query_explicit(db, param.id.parent()).iter_identity();
                    let resolver = param.id.parent().resolver(db);
                    match parse_bounds_for(bounds, assoc_ty, &resolver, self.db) {
                        ParsedProjection::NoBounds => assoc_type.clone(),
                        ParsedProjection::Projection(ty) => T::from_type(ty),
                        ParsedProjection::Traits(_) => {
                            /*
                            unimplemented!(
                                "{assoc_ty:?}: {assoc_ty_rs}",
                                assoc_ty_rs = assoc_ty.display(self.db, self.display_target()),
                            )
                             */
                            assoc_type.clone()
                        }
                    }
                }
                TyKind::Adt(_adt, _args) => {
                    let alias_id = TypeAliasId::from(alias_list[0]);
                    let rest_alias = &alias_list[1..];

                    let mut resolved_impl_assoc = None;

                    let trait_ = match alias_id.lookup(db).container {
                        ItemContainerId::TraitId(t) => t,
                        container => {
                            panic!("Projection type alias is defined in non-trait: {container:?}");
                        }
                    };

                    self.interner
                        .for_each_relevant_impl(trait_.into(), self_ty, |impl_def_id| {
                            let AnyImplId::ImplId(impl_id) = impl_def_id else {
                                // Builtin derive traits don't have type/consts assoc items.
                                return;
                            };

                            let trait_assoc_data = TypeAliasSignature::of(db, alias_id);
                            let Some(alias) = impl_id
                                .impl_items(db)
                                .items
                                .iter()
                                .find_map(|(impl_assoc_name, impl_assoc_id)| {
                                    if let AssocItemId::TypeAliasId(impl_assoc_id) = *impl_assoc_id
                                        && *impl_assoc_name == trait_assoc_data.name
                                    {
                                        Some(impl_assoc_id)
                                    } else {
                                        None
                                    }
                                })
                                .or_else(|| {
                                    if trait_assoc_data.ty.is_some() {
                                        Some(alias_id)
                                    } else {
                                        None
                                    }
                                })
                            else {
                                return;
                            };

                            assert!(self.interner.has_item_definition(alias.into()));

                            let ty = db.ty(alias.into());
                            let args = self.translate_args(impl_id, self_ty);
                            assert!(
                                self.interner
                                    .check_args_compatible(AnyImplId::ImplId(impl_id).into(), args)
                            );
                            resolved_impl_assoc = Some(ty.skip_binder());
                        });

                    trace!(
                        "resolved?: {assoc_ty} => {resolved}\n{resolved1:?}",
                        assoc_ty = self.new_type(assoc_ty).debug_display(self.db),
                        resolved = self
                            .new_type(resolved_impl_assoc.unwrap())
                            .debug_display(self.db),
                        resolved1 = self.ty_to_str(resolved_impl_assoc.unwrap()),
                    );

                    // internerが大事な役割果たしてそう。for_each_relevant_implでtraitと型がらimpl一覧取れたりする
                    // fetch_eligible_assoc_item of SolverContextがimpl => typeの解決につかえる
                    // Intener::has_item_definition が true なとき、ちゃんと定義されてて、
                    // EvalCtxt::translate_args Intener::check_args_compatible や　Intener::type_of (db::ty) で最終的に解決する

                    let inner = resolved_impl_assoc.unwrap();

                    if rest_alias.is_empty() {
                        T::from_type(self.new_type(inner))
                    } else {
                        self.resolve_assoc_of_impl_impl(&T::from_type(
                            self.new_alias_ty(&self.new_type(inner), rest_alias),
                        ))
                    }
                }
                TyKind::Error(_) => assoc_type.clone(),
                _ => {
                    eprintln!(
                        "Unsupported assoc type resolution but not assoc (self is not alias): {assoc_ty:?}"
                    );
                    T::from_type(
                        self.new_type(Ty::new(self.interner, TyKind::Error(ErrorGuaranteed))),
                    )
                }
            }
        }

        #[tracing::instrument(skip(self))]
        fn translate_args(&self, target: ImplId, self_ty: Ty<'db>) -> GenericArgs<'db> {
            let db = self.db;
            let impl_self_ty = db.impl_self_ty(target).instantiate_identity();
            let (self_adt, self_args) = self_ty.as_adt().unwrap();
            match impl_self_ty.kind() {
                TyKind::Param(param_ty) => {
                    debug!("translate_args: param: {param_ty:?}");

                    GenericArgs::for_item(self.interner, target.into(), |_i, arg, _| match arg {
                        GenericParamId::TypeParamId(ty_arg) if ty_arg == param_ty.id => {
                            Term::from(self_ty).into()
                        }
                        _ => GenericArg::error_from_id(self.interner, arg),
                    })
                }
                TyKind::Adt(impl_adt_def, impl_adt_args) => {
                    assert_eq!(impl_adt_def.def_id(), self_adt);
                    trace!("translate_args: adt: {impl_adt_def:?} {impl_adt_args:?} {self_args:?}");

                    let mut type_map = std::collections::HashMap::new();

                    for (impl_arg, self_arg) in
                        impl_adt_args.as_slice().iter().zip(self_args.as_slice())
                    {
                        if let Some(impl_arg) = impl_arg.ty() {
                            let self_arg = self_arg.expect_ty();

                            match impl_arg.kind() {
                                TyKind::Param(param)
                                    if param.id.parent() == GenericDefId::ImplId(target) =>
                                {
                                    type_map
                                        .insert(GenericParamId::TypeParamId(param.id), self_arg);
                                }
                                _ => {
                                    panic!("unsupported type");
                                }
                            }
                        }
                    }

                    GenericArgs::for_item(self.interner, target.into(), |_i, arg, _| {
                        if let Some(ty) = type_map.get(&arg) {
                            Term::from(*ty).into()
                        } else {
                            GenericArg::error_from_id(self.interner, arg)
                        }
                    })
                }
                impl_self_ty => panic!("Unexpected impl_self_ty: {impl_self_ty:?}"),
            }
        }

        pub fn new_type(&self, ty: Ty<'db>) -> Type<'db> {
            Type::from_ty(ty, self.db, self.krate.base())
        }

        #[allow(dead_code)] // may be used in the future
        pub fn adt_with_generic(&self, adt: Adt, args: GenericArgs<'db>) -> Type<'db> {
            let id = AdtId::from(adt);
            let interner = DbInterner::new_no_crate(self.db);
            let ty = Ty::new_adt(interner, id, args);
            self.new_type(ty)
        }
    }

    impl<'db> CodeGenerator<'db> {
        #[allow(dead_code)]
        pub(super) fn ty_to_str<'a>(&'a self, ty: Ty<'a>) -> impl std::fmt::Debug + 'a {
            std::fmt::from_fn(move |f| {
                match ty.kind() {
                    TyKind::Array(inner, count) => f
                        .debug_tuple("Array")
                        .field(&self.ty_to_str(inner))
                        .field(&count)
                        .finish(),
                    TyKind::Slice(inner) => f
                        .debug_tuple("Slice")
                        .field(&self.ty_to_str(inner))
                        .finish(),
                    TyKind::RawPtr(inner, mutability) => f
                        .debug_tuple("RawPtr")
                        .field(&self.ty_to_str(inner))
                        .field(&mutability)
                        .finish(),
                    TyKind::Ref(reg, inner, mutability) => f
                        .debug_tuple("Ref")
                        .field(&reg)
                        .field(&self.ty_to_str(inner))
                        .field(&mutability)
                        .finish(),
                    TyKind::Alias(alias) => f
                        .debug_tuple("Alias")
                        .field(&std::fmt::from_fn(move |f| match alias.kind {
                            AliasTyKind::Projection { def_id } => f
                                .debug_struct("Projection")
                                .field("def_id", &self.def_id_to_str(def_id))
                                .finish(),
                            AliasTyKind::Inherent { def_id } => f
                                .debug_struct("Inherent")
                                .field("def_id", &self.def_id_to_str(def_id))
                                .finish(),
                            AliasTyKind::Opaque { def_id } => f
                                .debug_struct("Opaque")
                                .field("def_id", &self.def_id_to_str(def_id))
                                .field("interned", &self.interner.type_of(def_id))
                                .finish(),
                            AliasTyKind::Free { def_id } => f
                                .debug_struct("Free")
                                .field("def_id", &self.def_id_to_str(def_id))
                                .finish(),
                        }))
                        .field(&self.args_to_str(alias.args))
                        .finish(),
                    TyKind::Param(param) => f
                        .debug_struct("Param")
                        .field("id", &param.id)
                        .field("index", &param.index)
                        .finish(),

                    //TyKind::FnDef(_, _) => {}
                    //TyKind::FnPtr(_, _) => {}
                    //TyKind::UnsafeBinder(_) => {}
                    //TyKind::Dynamic(_, _) => {}
                    //TyKind::Closure(_, _) => {}
                    //TyKind::CoroutineClosure(_, _) => {}
                    //TyKind::Coroutine(_, _) => {}
                    //TyKind::CoroutineWitness(_, _) => {}
                    //TyKind::Never => {}
                    //TyKind::Tuple(_) => {}
                    //TyKind::Bound(_, _) => {}
                    //TyKind::Placeholder(_) => {}
                    //TyKind::Infer(_) => {}
                    //TyKind::Error(_) => {}
                    m => std::fmt::Debug::fmt(&m, f),
                }
            })
        }

        pub(super) fn args_to_str<'a>(
            &'a self,
            args: GenericArgs<'a>,
        ) -> impl std::fmt::Debug + 'a {
            std::fmt::from_fn(move |f| {
                let mut list = f.debug_list();
                for arg in args.as_slice() {
                    list.entry(&std::fmt::from_fn(move |f| match arg.kind() {
                        GenericArgKind::Type(ty) => {
                            f.debug_tuple("Type").field(&self.ty_to_str(ty)).finish()
                        }
                        _ => std::fmt::Debug::fmt(&arg, f),
                    }));
                }
                list.finish()
            })
        }

        pub(super) fn def_id_to_str<'a>(
            &'a self,
            def_id: SolverDefId,
        ) -> impl std::fmt::Debug + 'a {
            std::fmt::from_fn(move |f| match def_id {
                SolverDefId::InternedOpaqueTyId(id) => f
                    .debug_tuple("InternedOpaqueTyId")
                    .field(&id)
                    .field(&id.loc(self.db))
                    .field(&id.loc(self.db).predicates(self.db))
                    .finish(),

                _ => std::fmt::Debug::fmt(&def_id, f),
            })
        }
    }

    /// Returns Some if the type is `<SomeT as SomeTrait>::AssociatedType`
    pub fn associated_type_of_some<'db>(ty: &Type<'db>) -> Option<(Type<'db>, hir::TypeAlias)> {
        if let TyKind::Alias(alias) = ty.ns_ty().kind()
            && let AliasTyKind::Projection { def_id } = alias.kind
            && let SolverDefId::TypeAliasId(alias_id) = def_id
        {
            Some((
                Type::from_ty_env(alias.self_ty(), ty.env()),
                hir::TypeAlias::from(alias_id),
            ))
        } else {
            None
        }
    }

    /// Returns Some if the type is `<Param as SomeTrait>::AssociatedType`
    pub fn assoc_of_type_param(ty: &Type) -> Option<(hir::TypeParam, Vec<hir::TypeAlias>)> {
        let mut cur = ty.ns_ty();
        let mut aliases = vec![];

        while let TyKind::Alias(alias) = cur.kind()
            && let AliasTyKind::Projection { def_id } = alias.kind
            && let SolverDefId::TypeAliasId(alias_id) = def_id
        {
            aliases.push(hir::TypeAlias::from(alias_id));
            cur = alias.self_ty();
        }

        aliases.reverse();

        if let TyKind::Param(param) = cur.kind()
            && !aliases.is_empty()
        {
            let param = hir::TypeParam::from(param.id);

            Some((param, aliases))
        } else {
            None
        }
    }
}

pub trait TyFromType<'db> {
    fn ns_ty(&self) -> hir::next_solver::Ty<'db>;
    fn env(&self) -> hir_ty::ParamEnvAndCrate<'db>;
    fn from_ty(
        ty: hir::next_solver::Ty<'db>,
        db: &'db dyn HirDatabase,
        krate: base_db::Crate,
    ) -> Self;
    fn from_ty_env(ty: hir::next_solver::Ty<'db>, env: hir_ty::ParamEnvAndCrate<'db>) -> Self;
    fn from_ty_resolver(
        ty: hir::next_solver::Ty<'db>,
        db: &'db dyn HirDatabase,
        resolver: &hir_def::resolver::Resolver<'_>,
    ) -> Self;
    fn error(db: &'db dyn HirDatabase, krate: base_db::Crate) -> Self;
}

mod ty_and_type {
    use crate::codegen::ty::TyFromType;
    use hir_def::CallableDefId;
    use hir_def::resolver::{HasResolver, Resolver};
    use hir_ty::ParamEnvAndCrate;
    use hir_ty::db::HirDatabase;
    use hir_ty::next_solver::{DbInterner, ErrorGuaranteed, ParamEnv, Ty};
    use ide_db::base_db;
    use ide_db::base_db::{CrateOrigin, LangCrateOrigin, all_crates};
    use rustc_type_ir::TyKind;

    #[allow(dead_code)]
    struct TypeMap<'db> {
        env: ParamEnvAndCrate<'db>,
        ty: Ty<'db>,
    }

    impl<'db> TyFromType<'db> for hir::Type<'db> {
        fn ns_ty(&self) -> hir::next_solver::Ty<'db> {
            // SAFETY:  This is NOT safe in rust guaranteed behavior,
            //          but known implementation allows us to do so.
            unsafe { std::mem::transmute::<&Self, &TypeMap<'db>>(self).ty }
        }

        fn env(&self) -> ParamEnvAndCrate<'db> {
            unsafe { std::mem::transmute::<&Self, &TypeMap<'db>>(self).env }
        }

        fn from_ty_env(ty: Ty<'db>, env: ParamEnvAndCrate<'db>) -> Self {
            unsafe { std::mem::transmute::<TypeMap<'db>, Self>(TypeMap { env, ty }) }
        }

        fn from_ty(ty: Ty<'db>, db: &'db dyn HirDatabase, krate: base_db::Crate) -> Self {
            unsafe {
                std::mem::transmute::<TypeMap<'db>, Self>(TypeMap {
                    env: ty_env(db, krate, ty),
                    ty,
                })
            }
        }

        fn from_ty_resolver(
            ty: hir::next_solver::Ty<'db>,
            db: &'db dyn HirDatabase,
            resolver: &hir_def::resolver::Resolver<'_>,
        ) -> Self {
            unsafe {
                std::mem::transmute::<TypeMap<'db>, Self>(TypeMap {
                    env: param_env_from_resolver(db, resolver),
                    ty,
                })
            }
        }

        fn error(db: &'db dyn HirDatabase, krate: base_db::Crate) -> Self {
            Self::from_ty(
                Ty::new(DbInterner::new_no_crate(db), TyKind::Error(ErrorGuaranteed)),
                db,
                krate,
            )
        }
    }

    fn ty_env<'db>(
        db: &'db dyn HirDatabase,
        krate: base_db::Crate,
        ty: hir::next_solver::Ty,
    ) -> ParamEnvAndCrate<'db> {
        use hir_ty::next_solver::TyKind;
        match ty.inner().internee {
            // builtin types
            TyKind::Bool
            | TyKind::Char
            | TyKind::Int(_)
            | TyKind::Uint(_)
            | TyKind::Float(_)
            | TyKind::Str => empty_param_env(core_crate(db)),

            TyKind::Adt(a, _) => param_env_from_resolver(db, &a.def_id().resolver(db)),
            TyKind::Foreign(f) => param_env_from_resolver(db, &f.0.resolver(db)),
            TyKind::Slice(e) => ty_env(db, krate, e),
            TyKind::Array(e, _) => ty_env(db, krate, e),
            TyKind::Pat(t, _) => ty_env(db, krate, t),
            TyKind::RawPtr(e, _) => ty_env(db, krate, e),
            TyKind::Ref(_, e, _) => ty_env(db, krate, e),
            TyKind::FnDef(f, _) => match f.0 {
                CallableDefId::FunctionId(f) => param_env_from_resolver(db, &f.resolver(db)),
                CallableDefId::StructId(s) => param_env_from_resolver(db, &s.resolver(db)),
                CallableDefId::EnumVariantId(e) => param_env_from_resolver(db, &e.resolver(db)),
            },

            // unknown types are fell backed to current drate with emtpy
            _ => empty_param_env(krate),
            /*
            TyKind::FnPtr(_, _) => empty_param_env(krate),
            TyKind::UnsafeBinder(_) => empty_param_env(krate),
            TyKind::Dynamic(_, _) => empty_param_env(krate),
            TyKind::Closure(_, _) => {}
            TyKind::CoroutineClosure(_, _) => {}
            TyKind::Coroutine(_, _) => {}
            TyKind::CoroutineWitness(_, _) => {}
            TyKind::Never => {}
            TyKind::Tuple(_) => {}
            TyKind::Alias(_) => {}
            TyKind::Param(_) => {}
            TyKind::Bound(_, _) => {}
            TyKind::Placeholder(_) => {}
            TyKind::Infer(_) => {}
            TyKind::Error(_) => {}
            // */
        }
    }

    fn core_crate(db: &dyn HirDatabase) -> base_db::Crate {
        all_crates(db)
            .iter()
            .copied()
            .find(|&krate| {
                matches!(
                    krate.data(db).origin,
                    CrateOrigin::Lang(LangCrateOrigin::Core)
                )
            })
            .unwrap_or_else(|| all_crates(db)[0])
    }

    fn param_env_from_resolver<'db>(
        db: &'db dyn HirDatabase,
        resolver: &Resolver<'_>,
    ) -> ParamEnvAndCrate<'db> {
        ParamEnvAndCrate {
            param_env: resolver
                .generic_def()
                .map_or_else(ParamEnv::empty, |generic_def| {
                    db.trait_environment(generic_def.into())
                }),
            krate: resolver.krate(),
        }
    }

    fn empty_param_env<'db>(krate: base_db::Crate) -> ParamEnvAndCrate<'db> {
        ParamEnvAndCrate {
            param_env: ParamEnv::empty(),
            krate,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ConstructableDef {
    Struct(hir::Struct),
    EnumVariant(hir::EnumVariant),
}

impl From<hir::Struct> for ConstructableDef {
    fn from(value: hir::Struct) -> Self {
        ConstructableDef::Struct(value)
    }
}

impl From<hir::EnumVariant> for ConstructableDef {
    fn from(value: hir::EnumVariant) -> Self {
        ConstructableDef::EnumVariant(value)
    }
}

impl ConstructableDef {
    pub fn from_variant(variant: hir::Variant) -> Option<Self> {
        match variant {
            hir::Variant::Struct(s) => Some(Self::Struct(s)),
            hir::Variant::EnumVariant(e) => Some(Self::EnumVariant(e)),
            _ => None,
        }
    }

    pub fn from_module_def(resolution: hir::ModuleDef) -> Option<Self> {
        match resolution {
            hir::ModuleDef::EnumVariant(variant) => Some(variant.into()),
            hir::ModuleDef::Adt(hir::Adt::Struct(variant)) => Some(variant.into()),
            _ => None,
        }
    }

    pub fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
        match self {
            Self::Struct(it) => it.fields(db),
            Self::EnumVariant(it) => it.fields(db),
        }
    }

    #[allow(dead_code)]
    pub fn module(self, db: &dyn HirDatabase) -> Module {
        match self {
            Self::Struct(it) => it.module(db),
            Self::EnumVariant(it) => it.module(db),
        }
    }

    #[allow(dead_code)]
    pub fn name(&self, db: &dyn HirDatabase) -> Name {
        match self {
            Self::Struct(s) => (*s).name(db),
            Self::EnumVariant(e) => (*e).name(db),
        }
    }

    pub fn adt(&self, db: &dyn HirDatabase) -> Adt {
        match *self {
            Self::Struct(it) => it.into(),
            Self::EnumVariant(it) => it.parent_enum(db).into(),
        }
    }

    pub fn kind(&self, db: &dyn HirDatabase) -> hir::StructKind {
        match *self {
            Self::Struct(it) => it.kind(db),
            Self::EnumVariant(it) => it.kind(db),
        }
    }
}

pub struct Constructable<'db> {
    pub def: ConstructableDef,
    pub args: Vec<Type<'db>>,
}

impl<'db> Constructable<'db> {
    pub fn new(def: ConstructableDef, args: Vec<Type<'db>>) -> Self {
        Self { def, args }
    }

    pub fn fields(&self, db: &dyn HirDatabase) -> Vec<hir::Field> {
        self.def.fields(db)
    }
}

pub trait TypeParamExt {
    fn trait_bounds_with_args(self, db: &'_ dyn HirDatabase) -> Vec<(Trait, Vec<Type<'_>>)>;
    fn trait_bounds_of_nested_type_with_args<'db>(
        self,
        t: &Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(Trait, Vec<Type<'db>>)>, Type<'db>>;
}

mod ty_param_ext {
    use crate::codegen::ty::{TyFromType, TypeParamExt};

    use hir::{Trait, Type, TypeParam};
    use hir_def::TypeParamId;
    use hir_def::lang_item::lang_items;
    use hir_def::resolver::{HasResolver, Resolver};
    use hir_ty::GenericPredicates;
    use hir_ty::db::HirDatabase;
    use hir_ty::next_solver::{AliasTy, Clause, ClauseKind, SolverDefId, TermKind, TraitRef, Ty};
    use itertools::{Either, Itertools};
    use rustc_type_ir::inherent::{GenericArg, IntoKind};
    use rustc_type_ir::{AliasTyKind, PredicatePolarity, TyKind};

    impl TypeParamExt for TypeParam {
        fn trait_bounds_with_args(self, db: &'_ dyn HirDatabase) -> Vec<(Trait, Vec<Type<'_>>)> {
            match self.trait_bounds_of_nested_type_with_args(&self.ty(db), db) {
                Either::Left(traits) => traits,
                Either::Right(_) => unreachable!(),
            }
        }

        fn trait_bounds_of_nested_type_with_args<'db>(
            self,
            t: &Type<'db>,
            db: &'db dyn HirDatabase,
        ) -> Either<Vec<(Trait, Vec<Type<'db>>)>, Type<'db>> {
            let self_ty = t.ns_ty();
            let resolver = TypeParamId::from(self).parent().resolver(db);

            match parse_bounds_for(
                GenericPredicates::query_explicit(db, TypeParamId::from(self).parent())
                    .iter_identity()
                    .collect::<Vec<_>>(),
                self_ty,
                &resolver,
                db,
            ) {
                ParsedProjection::Projection(t) => Either::Right(t),
                ParsedProjection::NoBounds => Either::Left(vec![]),
                ParsedProjection::Traits(traits) => Either::Left(traits),
            }
        }
    }

    pub enum ParsedProjection<'db> {
        NoBounds,
        Projection(Type<'db>),
        Traits(Vec<(Trait, Vec<Type<'db>>)>),
    }

    pub fn parse_bounds_for<'db>(
        bounds: impl IntoIterator<Item = Clause<'db>>,
        target_ty: Ty<'db>,
        resolver: &Resolver,
        db: &'db dyn HirDatabase,
    ) -> ParsedProjection<'db> {
        let mut clauses = vec![];

        {
            let mut target_ty = target_ty;
            while let TyKind::Alias(
                alias @ AliasTy {
                    kind:
                        AliasTyKind::Projection {
                            def_id: SolverDefId::TypeAliasId(alias_id),
                        },
                    ..
                },
            ) = target_ty.kind()
            {
                let interner = hir_ty::next_solver::interner::DbInterner::new_with(
                    db,
                    hir::TypeAlias::from(alias_id).module(db).krate(db).into(),
                );
                use rustc_type_ir::Interner;
                let x = interner
                    .item_self_bounds(alias_id.into())
                    .iter_instantiated(interner, alias.args)
                    .collect::<Vec<_>>();

                clauses.push(x);

                target_ty = alias.self_ty();
            }
        }

        let projection = 'resolve_projection: {
            let TyKind::Alias(alias @ AliasTy { .. }) = target_ty.kind() else {
                // likely to be already resolved
                break 'resolve_projection None;
            };
            let AliasTyKind::Projection {
                def_id: SolverDefId::TypeAliasId(alias_id),
            } = alias.kind
            else {
                panic!("Tries to assoc but not assoc: {target_ty:?}")
            };
            let self_ty = alias.self_ty();

            let interner = hir_ty::next_solver::interner::DbInterner::new_with(
                db,
                hir::TypeAlias::from(alias_id).module(db).krate(db).into(),
            );
            use rustc_type_ir::Interner;
            let x = interner
                .item_self_bounds(alias_id.into())
                .iter_instantiated(interner, alias.args)
                .collect::<Vec<_>>();

            clauses.push(x);

            Some((self_ty, SolverDefId::TypeAliasId(alias_id)))
        };

        let lang_items = lang_items(db, resolver.krate());

        #[derive(Debug)]
        enum Pred<'db> {
            Ty(Ty<'db>),
            Trait(TraitRef<'db>),
        }
        let conds = (bounds.into_iter().chain(clauses.into_iter().flatten())).collect::<Vec<_>>();
        let preds = (conds.into_iter())
            .filter_map(|clause| match clause.kind().skip_binder() {
                ClauseKind::Projection(proj)
                    if Some((proj.projection_term.self_ty(), proj.def_id())) == projection =>
                {
                    match proj.term.kind() {
                        TermKind::Ty(ty) => Some(Pred::Ty(ty)),
                        TermKind::Const(_) => {
                            unreachable!("Associated type is not type")
                        }
                    }
                }
                ClauseKind::Trait(trait_)
                    if trait_.self_ty() == target_ty
                        && trait_.polarity == PredicatePolarity::Positive =>
                {
                    Some(Pred::Trait(trait_.trait_ref))
                }
                _ => None,
            })
            .filter(|x| !matches!(x, Pred::Trait(trait_ref) if Some(trait_ref.def_id.into()) == lang_items.Sized))
            .collect::<Vec<_>>();

        if preds.is_empty() {
            return ParsedProjection::NoBounds;
        }

        // If there is <Assoc = SomeType> part, we pick the type
        if let Some(pred) = preds.iter().find_map(|x| match x {
            Pred::Ty(ty) => Some(ty),
            _ => None,
        }) {
            return ParsedProjection::Projection(Type::from_ty_resolver(*pred, db, resolver));
        }

        ParsedProjection::Traits(
            preds
                .into_iter()
                .map(|x| match x {
                    Pred::Trait(trait_ref) => {
                        let types = trait_ref
                            .args
                            .as_slice()
                            .iter()
                            .flat_map(|arg| arg.as_type())
                            .map(|ty| Type::from_ty_resolver(ty, db, resolver))
                            .collect();
                        (Trait::from(trait_ref.def_id.0), types)
                    }
                    _ => unreachable!(),
                })
                //.filter(|(t, _)| Some((*t).into()) != lang_items.Sized)
                .unique()
                .collect(),
        )
    }
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
            DynCompatibilityViolation::GAT(_) => false,
            DynCompatibilityViolation::HasNonCompatibleSuperTrait(_) => false,
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
            .any(|&x| Some(x.into()) == self.lang_items.Sized)
        {
            cb(DynCompatibilityViolation::SizedSelf);
        }

        if predicates_reference_self(self, self.db, trait_) {
            cb(DynCompatibilityViolation::SelfReferential);
        }

        // TODO: check for SelfReferential

        for assoc_item in trait_.items_with_supertraits(db) {
            // remove associated items with Self: Sized, but does not exclude Trait : Sized
            if generics_require_sized_self(trait_, assoc_item, db) {
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
                        cb(DynCompatibilityViolation::Method(
                            hir_def::FunctionId::try_from(it).unwrap(),
                            mvc,
                        ))
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
            hir_ty::GenericPredicates::query_explicit(
                db,
                GenericDef::from(trait_).try_into().unwrap(),
            )
            .iter_identity()
            .any(|predicate| match predicate.kind().skip_binder() {
                rustc_type_ir::ClauseKind::Trait(trait_pred) => {
                    trait_pred.trait_ref.args.iter().skip(1).any(|arg| {
                        match arg.kind() {
                            hir_ty::next_solver::GenericArgKind::Type(type_) => {
                                this.includes_type_in_type(&this.new_type(type_), &|ty| {
                                    if let Some(param) = ty.as_type_param(this.db) {
                                        // Generic parameter Self
                                        *param.name(this.db).symbol() == sym::Self_
                                    } else {
                                        false
                                    }
                                })
                            }
                            _ => false,
                        }
                    })
                }
                rustc_type_ir::ClauseKind::Projection(proj_pred) => {
                    proj_pred.projection_term.args.iter().skip(1).any(|arg| {
                        match arg.kind() {
                            hir_ty::next_solver::GenericArgKind::Type(type_) => {
                                this.includes_type_in_type(&this.new_type(type_), &|ty| {
                                    if let Some(param) = ty.as_type_param(this.db) {
                                        // Generic parameter Self
                                        *param.name(this.db).symbol() == sym::Self_
                                    } else {
                                        false
                                    }
                                })
                            }
                            _ => false,
                        }
                    })
                }
                _ => false,
            })
        }

        fn generics_require_sized_self(
            trait_: Trait,
            assoc_item: AssocItem,
            db: &dyn HirDatabase,
        ) -> bool {
            let krate = assoc_item.module(db).krate(db);
            let interner = hir_ty::next_solver::DbInterner::new_with(db, krate.into());
            let Some(sized) = interner.lang_items().Sized else {
                return false;
            };

            let predicates = hir_ty::GenericPredicates::query_own_explicit(
                db,
                match assoc_item {
                    AssocItem::Function(f) => GenericDef::from(f).try_into().unwrap(),
                    AssocItem::Const(c) => GenericDef::from(c).try_into().unwrap(),
                    AssocItem::TypeAlias(t) => GenericDef::from(t).try_into().unwrap(),
                },
            );

            let includes_sized =
                rustc_type_ir::elaborate::elaborate(interner, predicates.iter_identity()).any(
                    |pred| match pred.kind().skip_binder() {
                        rustc_type_ir::ClauseKind::Trait(trait_pred) => {
                            if sized == trait_pred.def_id().0
                                && let rustc_type_ir::TyKind::Param(param_ty) =
                                    trait_pred.trait_ref.self_ty().kind()
                                && param_ty.index == 0
                            {
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    },
                );
            if includes_sized {
                trace!(
                "ignored item {assoc_item} of {trait} since guarded by Sized",
                    assoc_item = assoc_item.name(db).unwrap().as_str(),
                    trait = trait_.debug_display(db),
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
            .any(|x| self.includes_type_in_type(x.ty(), &is_self_ty))
        {
            cb(MethodViolationCode::ReferencesSelfInput);
        }

        if self.includes_type_in_type(&func.ret_type(db), &is_self_ty) {
            cb(MethodViolationCode::ReferencesSelfOutput);
        }

        let params = GenericDef::from(func).params(db);

        if self.includes_type_in_generic_params_cs_constraints(&params, &is_self_ty) {
            cb(MethodViolationCode::WhereClauseReferencesSelf);
        }
    }

    fn includes_type_in_type(&self, ty: &Type<'db>, cond: &impl Fn(&Type<'db>) -> bool) -> bool {
        let db = self.db;
        let ty = &self.resolve_assoc_of_impl(ty);

        if cond(ty) {
            return true;
        }

        if let Some((inner, _mutability)) = ty.as_reference() {
            self.includes_type_in_type(&inner, cond)
        } else if let Some((inner, _)) = ty.as_raw_ptr() {
            self.includes_type_in_type(&inner, cond)
        } else if let Some(inner) = ty.as_slice() {
            self.includes_type_in_type(&inner, cond)
        } else if let rustc_type_ir::TyKind::Array(inner, _size) = ty.ns_ty().kind() {
            self.includes_type_in_type(&self.new_type(inner), cond)
        } else if ty.is_tuple() {
            let fields = ty.tuple_fields(db);
            fields.iter().any(|t| self.includes_type_in_type(t, cond))
        } else if let Some((_, args)) = ty.as_adt_with_args() {
            args.iter()
                .flatten()
                .any(|t| self.includes_type_in_type(t, cond))
        } else if ty.as_impl_traits(db).is_some() {
            use hir_ty::next_solver::ClauseKind;
            // Unfortunately hir crate does not provide us generic parameters of trait so we access
            // new solver's ty
            let traits = ty
                .ns_ty()
                .impl_trait_bounds(db)
                .unwrap()
                .into_iter()
                .filter_map(|pred| match pred.kind().skip_binder() {
                    ClauseKind::Trait(trait_ref) => {
                        Some((Trait::from(trait_ref.def_id().0), trait_ref.trait_ref.args))
                    }
                    _ => None,
                })
                // remove marker traits including lang items like Send, Sized
                .filter(|&(t, _)| !t.items_with_supertraits(db).is_empty())
                .collect::<Vec<_>>();
            if traits.len() > 1 {
                eprintln!("Multiple traits are used: {}", ty.debug_display(self.db));
            }
            if traits.is_empty() {
                false
            } else {
                let (_, args) = traits[0];
                let rs_generic_args = self.generic_args_to_types(args).collect::<Vec<_>>();

                rs_generic_args
                    .iter()
                    .skip(1)
                    .any(|x| self.includes_type_in_type(x, cond))
            }
        } else {
            false
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
            self.includes_type_in_type(&param_type, &cond)
                || match param.trait_bounds_of_nested_type_with_args(&param_type, db) {
                    Either::Right(projection) => self.includes_type_in_type(&projection, &cond),
                    Either::Left(traits) => traits.iter().any(|&(trait_, ref args)| {
                        self.trait_type_args(trait_, &args[0], &args[1..])
                            .any(|t| self.includes_type_in_type(&t, &cond))
                    }),
                }
        })
    }
}
