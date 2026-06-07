use super::{CodeGenerator, generic_args, names};
/// Converts Rust HIR types to C# type strings.
use hir::db::HirDatabase;
use hir::next_solver::GenericArgs;
use hir::{
    Adt, AssocItem, BuiltinType, GenericDef, GenericParam, HasContainer, HasCrate, ItemContainer,
    Module, Name, Symbol, Trait, Type, sym,
};
use hir_ty::display::HirDisplay;
use ide_db::base_db;
use itertools::{Either, Itertools};
use rustc_type_ir::inherent::{IntoKind, SliceLike};
use std::collections::HashMap;

impl<'db> CodeGenerator<'db> {
    pub fn rust_type_to_cs(&self, ty: &Type<'db>) -> String {
        self.rust_type_to_cs_inner(ty, false)
    }

    fn rust_type_to_cs_inner(&self, ty: &Type<'db>, in_slot: bool) -> String {
        let db = self.db;
        if ty.is_unit() {
            return "global::System.ValueTuple".to_string();
        }
        if ty.is_never() {
            return "void".to_string();
        }

        let ty = &self.resolve_assoc_of_impl(ty);

        // Primitives
        if let Some(builtin) = ty.as_builtin() {
            return self.builtin_to_cs(builtin);
        }

        // Reference → just the inner type (C# is reference semantics; we use Slot<T> for mutability)
        if let Some((inner, _mutability)) = ty.as_reference() {
            return self.rust_type_to_cs_inner(&inner, in_slot);
        }

        // Raw pointer → Ref<T>
        if let Some((inner, _)) = ty.as_raw_ptr() {
            return format!("Ref<{}>", self.rust_type_to_cs_inner(&inner, false));
        }

        // Slice
        if let Some(inner) = ty.as_slice() {
            return format!(
                "System.Memory<{}>",
                self.rust_type_to_cs_inner(&inner, false)
            );
        }

        // Array
        //if let Some((inner, _size)) = ty.as_array(db) {
        if let rustc_type_ir::TyKind::Array(inner, _size) = ty.ns_ty().kind() {
            return format!(
                "{}[]",
                self.rust_type_to_cs_inner(&self.new_type(inner), false)
            );
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
                    self.rust_type_to_cs_inner(&fields[0], false)
                );
            }
            let parts: Vec<String> = fields
                .iter()
                .map(|t| self.rust_type_to_cs_inner(t, false))
                .collect();
            return format!("({})", parts.join(", "));
        }

        // ADT (struct/enum/union)
        if let Some((adt, args)) = ty.as_adt_with_args() {
            let rust_adt_name = self.adt_rust_name(&adt, db);
            // Check std library mappings first
            if let Some(mapped) = self.map_std_type(&rust_adt_name, &args, db) {
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
            if let Some(cs) = self.special_types.borrow().get(&param) {
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
        } else if let Some((param, aliases)) = rustc_ty::alias_of_type_params(ty) {
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
        } else if let Some(_) = ty.as_impl_traits(db) {
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
                eprintln!(
                    "Multiple traits are used: {}",
                    ty.display(db, self.display_target())
                );
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
                    match ty
                        .normalize_trait_assoc_type(db, &rs_generic_args, alias)
                        .map(|x| self.resolve_assoc_of_impl(&x))
                    {
                        None => {
                            eprintln!(
                                "Failed to resolve type {alias} of {ty}",
                                alias = alias.display(db, self.display_target()),
                                ty = ty.display(db, self.display_target())
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
        } else if ty.is_fn() {
            // Closure types, fn pointers, etc. — use Action/Func
            "Action".to_string()
        } else if ty.is_closure() {
            // Closure types, fn pointers, etc. — use Action/Func
            "Action".to_string()
        } else if let normalized = self.resolve_assoc_of_impl(ty)
            && &normalized != ty
        {
            tracing::debug!(
                "Normalized! {} => {} ({normalized:?})\n",
                ty.display(self.db, self.display_target()),
                normalized.display(self.db, self.display_target())
            );
            self.rust_type_to_cs_inner(&normalized, in_slot)
        } else {
            eprintln!(
                "Unsupported type: {}: {ty:?}\n",
                ty.display(self.db, self.display_target()),
                ty = self.ty_to_str(ty.ns_ty()),
            );
            format!(
                "object /*Unsupported type: {} */",
                ty.display(self.db, self.display_target())
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

    fn adt_rust_name(&self, adt: &Adt, db: &dyn HirDatabase) -> String {
        match adt {
            Adt::Struct(s) => s.name(db).as_str().to_string(),
            Adt::Enum(e) => e.name(db).as_str().to_string(),
            Adt::Union(u) => u.name(db).as_str().to_string(),
        }
    }

    /// Maps well-known std types to C# equivalents.
    fn map_std_type(
        &self,
        rust_name: &str,
        args: &[Option<Type<'db>>],
        db: &'db dyn HirDatabase,
    ) -> Option<String> {
        match rust_name {
            "String" | "str" => Some("string".to_string()),
            "Vec" => {
                let inner = args.first()?.as_ref()?;
                Some(format!(
                    "System.Collections.Generic.List<{}>",
                    self.rust_type_to_cs_inner(inner, false)
                ))
            }
            "Box" => {
                let inner = args.first()?.as_ref()?;
                Some(self.rust_type_to_cs_inner(inner, false))
            }
            "Arc" | "Rc" | "Mutex" | "RwLock" => {
                let inner = args.first()?.as_ref()?;
                Some(self.rust_type_to_cs_inner(inner, false))
            }
            "HashMap" | "BTreeMap" | "IndexMap" | "AHashMap" => {
                let k = args.first()?.as_ref()?;
                let v = args.get(1)?.as_ref()?;
                Some(format!(
                    "System.Collections.Generic.Dictionary<{}, {}>",
                    self.rust_type_to_cs_inner(k, false),
                    self.rust_type_to_cs_inner(v, false)
                ))
            }
            "HashSet" | "BTreeSet" | "IndexSet" => {
                let inner = args.first()?.as_ref()?;
                Some(format!(
                    "System.Collections.Generic.HashSet<{}>",
                    self.rust_type_to_cs_inner(inner, false)
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
                let t = self.rust_type_to_cs_inner(inner, false);
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

            match self.func_impl_type_param(param) {
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

            if self.func_impl_type_param(param).is_special_impl() {
                continue;
            }

            type_params.push(CsTypeParamSource::TypeParam(index));

            let traits = param.trait_bounds_with_args(db);

            self.collect_assoc_type_params(
                &mut type_params,
                param,
                index,
                &traits,
                &param.ty(db),
                &[],
            );
        }

        type_params
    }

    fn collect_assoc_type_params(
        &self,
        type_params: &mut Vec<CsTypeParamSource>,
        param: hir::TypeParam,
        index: usize,
        traits: &[(hir::Trait, Vec<hir::Type>)],
        outer_instance: &hir::Type<'db>,
        outer_aliases: &[hir::TypeAlias],
    ) {
        if traits.is_empty() {
            return;
        }
        let db = self.db;

        for &(trait_, ref args) in traits {
            for alias in trait_.assoc_types(db) {
                let aliases = (outer_aliases.iter().copied().chain([alias])).collect::<Vec<_>>();
                let instance = self.new_alias_ty(outer_instance, &[alias])
                //let Some(instance) = outer_instance.normalize_trait_assoc_type(db, &[], alias)
                else {
                    panic!("unable to resolve {alias:?} of {type}",
                           alias = alias.name(db).as_str(),
                           type = outer_instance.display(db, self.display_target()),
                    );
                    continue;
                };
                if let Some((param_instance, alias_instance)) =
                    rustc_ty::alias_of_type_params(&instance)
                    && param_instance == param
                    && alias_instance == aliases
                {
                    let alias = self.new_alias_ty(&param.ty(db), &aliases);

                    if self.rust_type_to_cs(&alias) == "P_S_SerializeSeq_Ok" {
                        print!("");
                    }

                    match param.trait_bounds_of_nested_type_with_args(&alias, db) {
                        Either::Right(projected) => {
                            // projection. nothing to do
                            // eprintln!("projection: {projected:?}");
                        }
                        Either::Left(traits) => {
                            type_params
                                .push(CsTypeParamSource::AliasOfParam(index, aliases.clone()));

                            self.collect_assoc_type_params(
                                type_params,
                                param,
                                index,
                                &traits,
                                &instance,
                                &aliases,
                            );
                        }
                    }
                }
            }
        }
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
                            let generic_args = args
                                .iter()
                                .skip((!trait_.with_self_in_cs(db)) as usize)
                                .cloned();
                            let assoc_types = trait_.assoc_types(db).into_iter().map(|alias| {
                                self.resolve_assoc_of_impl(
                                    &param_type
                                        .normalize_trait_assoc_type(db, &[], alias)
                                        .unwrap(),
                                )
                            });

                            let args = (generic_args.chain(assoc_types))
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
}

enum SpecialImplBounds<'db> {
    None,
    Func(Type<'db>, Vec<Type<'db>>),
    Future(Type<'db>),
}

impl<'db> SpecialImplBounds<'db> {
    fn is_special_impl(&self) -> bool {
        !matches!(self, SpecialImplBounds::None)
    }
}

impl<'db> CodeGenerator<'db> {
    fn func_impl_type_param(&self, param: hir::TypeParam) -> SpecialImplBounds<'db> {
        let db = self.db;

        if let bounds = param
            .trait_bounds_with_args(db)
            .into_iter()
            .filter(|&(t, _)| Some(t.into()) != self.lang_items.Sized)
            .filter(|&(t, _)| Some(t.into()) != self.lang_items.Sync)
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

                SpecialImplBounds::Func(output, parameters)
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

                SpecialImplBounds::Future(output)
            } else {
                SpecialImplBounds::None
            }
        } else {
            SpecialImplBounds::None
        }
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

        if !trait_.with_self_in_cs(db) && !cs_type_params.is_empty() {
            cs_type_params.remove(0);
        }

        let self_ty = &args[0];
        for alias in trait_.assoc_types(db) {
            match self_ty
                .normalize_trait_assoc_type(db, &args, alias)
                .map(|x| self.resolve_assoc_of_impl(&x))
            {
                None => {
                    eprintln!(
                        "Failed to resolve type {alias} of {ty}",
                        alias = alias.display(db, self.display_target()),
                        ty = self_ty.display(db, self.display_target())
                    );
                    cs_type_params.push(format!(
                        "void /* {alias} of {ty} */",
                        alias = alias.display(db, self.display_target()),
                        ty = self_ty.display(db, self.display_target()),
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

        match f.container(self.db) {
            ItemContainer::Impl(impl_) if let Some(adt) = impl_.self_ty(self.db).as_adt() => {
                let mut path = self.adt_name_cs(adt);
                path = generic_args(path, self.params(&args_by_symbol, adt.into()));
                path.push('.');
                path.push_str(&self.function_name(f));
                path = generic_args(path, self.params(&args_by_symbol, f.into()));
                path
            }
            ItemContainer::Impl(impl_)
                if let Some(primitive) = impl_.self_ty(self.db).as_builtin() =>
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

    pub fn enum_variant_cs1(&self, v: hir::EnumVariant, generic: GenericArgs<'db>) -> String {
        let mut path =
            self.rust_type_to_cs(&self.adt_with_generic(v.parent_enum(self.db).into(), generic));
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

mod rustc_ty {
    //! Next solver ty related methods
    //!
    //! since rustc_type is NOT part of rustc_hir crate which is API boundary,
    //! we don't want to use those type generally

    use crate::codegen::CodeGenerator;
    use crate::codegen::ty::ty_param_ext::{ParsedProjection, parse_bounds_for};
    use crate::codegen::ty::{TyFromType, TypeParamExt};
    use hir::{Adt, Trait, Type};
    use hir_def::resolver::{HasResolver, Resolver};
    use hir_def::signatures::TypeAliasSignature;
    use hir_def::{
        AdtId, AssocItemId, GenericDefId, GenericParamId, HasModule, ImplId, ItemContainerId,
        Lookup, TypeAliasId, TypeParamId,
    };
    use hir_ty::display::HirDisplay;
    use hir_ty::next_solver::{
        AnyImplId, Binder, Clause, ClauseKind, Const, ConstKind, DbInterner, ErrorGuaranteed,
        GenericArg, GenericArgKind, GenericArgs, ParamEnv, PredicateKind, SolverDefId, Term,
        TermKind, TraitRef, Ty,
    };
    use hir_ty::{GenericPredicates, ImplTraitId, TyDefId};
    use rustc_type_ir::inherent::{GenericsOf as _, IntoKind, SliceLike, Term as _};
    use rustc_type_ir::solve::{Goal, GoalSource, NoSolution};
    use rustc_type_ir::{AliasTy, AliasTyKind, Interner, PredicatePolarity, TyKind};
    use std::fmt::Debug;
    use tracing::debug;

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
            debug!(
                "resolve_assoc_of_impl: {}",
                assoc_ty.display(self.db, self.display_target())
            );
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
                            assoc_ty = assoc_ty.display(self.db, self.display_target()),
                            kind = self_ty_alias.kind,
                        );
                        return assoc_type.clone();
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
                        ParsedProjection::Traits(_) => {
                            unimplemented!(
                                "{assoc_ty_rs}",
                                assoc_ty_rs = assoc_ty.display(self.db, self.display_target()),
                            )
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
                TyKind::Adt(adt, args) => {
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

                    debug!(
                        "resolved?: {assoc_ty} => {resolved}\n{resolved1:?}",
                        assoc_ty = self.new_type(assoc_ty).display(db, self.display_target()),
                        resolved = self
                            .new_type(resolved_impl_assoc.unwrap())
                            .display(db, self.display_target()),
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

                    GenericArgs::for_item(self.interner, target.into(), |i, arg, _| match arg {
                        GenericParamId::TypeParamId(ty_arg) if ty_arg == param_ty.id => {
                            Term::from(self_ty).into()
                        }
                        _ => GenericArg::error_from_id(self.interner, arg),
                    })
                }
                TyKind::Adt(impl_adt_def, impl_adt_args) => {
                    assert_eq!(impl_adt_def.def_id(), self_adt);
                    debug!("translate_args: adt: {impl_adt_def:?} {impl_adt_args:?} {self_args:?}");

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

                    GenericArgs::for_item(self.interner, target.into(), |i, arg, _| {
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

    /// Returns Some if the type is `<Param as SomeTrait>::AssociatedType`
    pub fn alias_of_type_params(ty: &Type) -> Option<(hir::TypeParam, Vec<hir::TypeAlias>)> {
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
            .map(base_db::Crate::from)
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

    pub fn module(self, db: &dyn HirDatabase) -> Module {
        match self {
            Self::Struct(it) => it.module(db),
            Self::EnumVariant(it) => it.module(db),
        }
    }

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

pub fn variant_from_module_def(resolution: hir::ModuleDef) -> Option<hir::Variant> {
    match resolution {
        hir::ModuleDef::EnumVariant(variant) => Some(variant.into()),
        hir::ModuleDef::Adt(hir::Adt::Struct(variant)) => Some(variant.into()),
        hir::ModuleDef::Adt(hir::Adt::Union(variant)) => Some(variant.into()),
        _ => None,
    }
}

pub trait TypeExt<'db> {
    fn expect_adt_with_args(&self) -> (Adt, Vec<Option<Type<'db>>>);
    fn expect_adt_of(&self, adt: Adt) -> Vec<Type<'db>>;
}

impl<'db> TypeExt<'db> for Type<'db> {
    fn expect_adt_with_args(&self) -> (Adt, Vec<Option<Type<'db>>>) {
        self.as_adt_with_args()
            .unwrap_or_else(|| panic!("expected adt but was {self:?}"))
    }

    fn expect_adt_of(&self, adt: Adt) -> Vec<Type<'db>> {
        let (adt_of_ty, types) = self
            .as_adt_with_args()
            .unwrap_or_else(|| panic!("expected adt of {adt:?} but was {self:?}"));
        assert_eq!(adt_of_ty, adt, "expected adt of {adt:?} but was {self:?}");

        types.into_iter().flatten().collect()
    }
}

pub trait TypeParamExt {
    fn trait_bounds_with_args(self, db: &'_ dyn HirDatabase) -> Vec<(Trait, Vec<Type>)>;
    fn trait_bounds_of_nested_type_with_args<'db>(
        self,
        t: &Type<'db>,
        db: &'db dyn HirDatabase,
    ) -> Either<Vec<(Trait, Vec<Type<'db>>)>, Type<'db>>;
}

mod ty_param_ext {
    use crate::codegen::ty::{TyFromType, TypeParamExt};
    use hir::sym::unreachable;
    use hir::{HasContainer, ItemContainer, Trait, Type, TypeParam};
    use hir_def::lang_item::lang_items;
    use hir_def::resolver::{HasResolver, Resolver};
    use hir_def::{TypeAliasId, TypeParamId};
    use hir_ty::GenericPredicates;
    use hir_ty::db::HirDatabase;
    use hir_ty::next_solver::{
        AliasTy, Clause, ClauseKind, ErrorGuaranteed, GenericArgKind, SolverDefId, TermKind,
        TraitRef, Ty,
    };
    use itertools::{Either, Itertools};
    use rustc_type_ir::inherent::{GenericArg, IntoKind};
    use rustc_type_ir::{AliasTyKind, Interner, PredicatePolarity, TyKind};

    impl TypeParamExt for TypeParam {
        fn trait_bounds_with_args(self, db: &'_ dyn HirDatabase) -> Vec<(Trait, Vec<Type>)> {
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
        Traits((Vec<(Trait, Vec<Type<'db>>)>)),
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
        let lang_items = lang_items(db, resolver.krate());

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
                .filter(|(t, _)| Some((*t).into()) != lang_items.Sized)
                .unique()
                .collect(),
        )
    }
}

pub trait TraitExt {
    fn with_self_in_cs(&self, db: &dyn HirDatabase) -> bool;
    fn assoc_types(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias>;
}

impl TraitExt for Trait {
    fn with_self_in_cs(&self, db: &dyn HirDatabase) -> bool {
        if self.dyn_compatibility(db).is_some() {
            return true;
        }

        false
    }

    fn assoc_types(&self, db: &dyn HirDatabase) -> Vec<hir::TypeAlias> {
        self.items(db)
            .into_iter()
            .flat_map(|x| match x {
                hir::AssocItem::TypeAlias(t) => Some(t),
                _ => None,
            })
            .collect()
    }
}
