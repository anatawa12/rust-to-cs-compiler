use super::{CodeGenerator, names};
use crate::codegen::names::mod_name;
/// Converts Rust HIR types to C# type strings.
use hir::db::HirDatabase;
use hir::next_solver::GenericArgs;
use hir::{Adt, BuiltinType, HasContainer, ItemContainer, Module, Name, Trait, Type};
use hir_ty::display::HirDisplay;
use ide_db::base_db;
use rustc_type_ir::inherent::IntoKind;

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
            return format!("{}[]", self.rust_type_to_cs_inner(&inner, false));
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

            let type_args: Vec<String> = args
                .iter()
                .filter_map(|a| a.as_ref())
                .map(|a| self.rust_type_to_cs_inner(a, false))
                .filter(|s| s != "void")
                .collect();
            if type_args.is_empty() {
                cs_path
            } else {
                format!("{}<{}>", cs_path, type_args.join(", "))
            }
        } else if let Some(trait_) = ty.as_dyn_trait() {
            // dyn Trait → T_TraitName (dyn interface)
            names::dyn_trait_name(trait_.name(db).as_str())
        } else if let Some(param) = ty.as_type_param(db) {
            // Generic parameter
            let name = param.name(db).as_str().to_string();
            if name == "Self" {
                "Self".to_string()
            } else if !param.is_implicit(db) {
                names::generic_param(&name)
            } else {
                format!("/* implicit */ {}", self.impl_ty_param_id.id_name(&param))
            }
        } else if let Some((param, alias)) = rustc_ty::alias_of_type_params(ty) {
            format!(
                "{}_{} /* {} */",
                self.rust_type_to_cs_inner(&param.ty(db), false),
                alias.name(db).as_str(),
                ty.display(db, self.display_target())
            )
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
                let t = self.trait_itf_cs(trait_);
                // TODO: Associated types as generic parameters
                if Some(trait_.into()) == self.lang_items.Future {
                    let generic_args = self.generic_args_to_types(args).collect::<Vec<_>>();
                    ty.normalize_trait_assoc_type(
                        db,
                        &generic_args,
                        self.lang_items.FutureOutput.unwrap().into(),
                    )
                    .map(|x| self.resolve_assoc_of_impl(&x));
                }
                t
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
            "i128" => "global::System.Int128",
            "isize" => "nint",
            "u8" => "byte",
            "u16" => "ushort",
            "u32" => "uint",
            "u64" => "ulong",
            "u128" => "global::System.UInt128",
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
            "PathBuf" | "Path" | "OsString" | "OsStr" | "CString" | "CStr" => {
                Some("string".to_string())
            }
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
    pub fn generic_params_cs(
        &self,
        params: &[hir::GenericParam],
        db: &dyn HirDatabase,
    ) -> (Vec<String>, Vec<String>) {
        let mut type_params = Vec::new();
        let mut constraints = Vec::new();

        for param in params {
            match param {
                hir::GenericParam::TypeParam(param) => {
                    let name = names::generic_param(param.name(db).as_str());
                    type_params.push(if !param.name(db).is_missing() {
                        name
                    } else {
                        format!("/* implicit */ {}", self.impl_ty_param_id.id_name(param))
                    });
                    // TODO: add where clauses from bounds
                }
                hir::GenericParam::ConstParam(_cp) => {
                    // C# doesn't support const generics in the same way; skip for now
                }
                hir::GenericParam::LifetimeParam(_) => {
                    // Lifetimes don't translate to C#
                }
            }
        }

        (type_params, constraints)
    }

    pub fn trait_itf_cs1(
        &self,
        trait_: Trait,
        args: GenericArgs,
        self_ty: Option<&Type<'db>>,
    ) -> String {
        let db = self.db;

        // TODO: args
        if trait_.dyn_compatibility(db).is_none() {
            self.trait_itf_cs(trait_)
        } else {
            let mut path = self.trait_itf_cs(trait_);
            path.push('<');
            path.push_str(&self.rust_type_to_cs(self_ty.unwrap()));
            path.push('>');
            path
        }
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
            let module_name = module.name(self.db);
            let module_name = module_name.as_ref().map(|m| m.as_str()).unwrap_or_else(|| {
                let src = module.definition_source(self.db);

                eprintln!(
                    "Unsupported: module (crate) does not have a name in {path} at {:?}",
                    self.location_with_file(src)
                );
                return "unnamed_mod";
            });
            path.push('.');
            path.push_str(&mod_name(module_name));
            path
        } else {
            if !module.is_crate_root(self.db) {
                eprintln!("Unsupported: non-crate root module without parent module: {module:?}")
            }
            let crate_name = module.krate(self.db).display_name(self.db);
            let crate_name = crate_name.as_ref().map(|x| x.as_str()).unwrap_or_else(|| {
                eprintln!("Unsupported: module (crate) does not have a name");
                return "unnamed_crate";
            });
            let mut path = String::new();
            path.push_str("global::");
            path.push_str(&self.root_namespace);
            path.push_str(".");
            path.push_str(mod_name(&crate_name).as_str());
            path
        }
    }

    pub fn fn_path_cs(&self, f: hir::Function) -> String {
        match f.container(self.db) {
            ItemContainer::Impl(impl_) if let Some(adt) = impl_.self_ty(self.db).as_adt() => {
                let mut path = self.module_class_cs(adt.module(self.db));
                path.push('.');
                path.push_str(&names::struct_name(adt.name(self.db).as_str()));
                path.push('.');
                path.push_str(&names::method_name(f.name(self.db).as_str()));
                path
            }
            ItemContainer::Impl(impl_)
                if let Some(primitive) = impl_.self_ty(self.db).as_builtin() =>
            {
                let mut path = String::from(primitive.name().as_str());
                path.push('.');
                path.push_str(&names::method_name(f.name(self.db).as_str()));
                path
            }
            ItemContainer::Module(module) => {
                let mut path = self.module_class_cs(module);
                path.push('.');
                path.push_str(&names::method_name(f.name(self.db).as_str()));
                path
            }
            //ItemContainer::Trait(_) => {}
            //ItemContainer::ExternBlock(_) => {}
            //ItemContainer::Crate(_) => {}
            ItemContainer::Impl(impl_) => {
                eprintln!(
                    "Unsupported function type with self: {:?}",
                    impl_.self_ty(self.db)
                );
                let mut path = self.module_class_cs(f.module(self.db));
                path.push('.');
                path.push_str(&names::method_name(f.name(self.db).as_str()));
                path
            }
            unsupported => {
                eprintln!("Unsupported function type: {:?}", unsupported);
                let mut path = self.module_class_cs(f.module(self.db));
                path.push('.');
                path.push_str(&names::method_name(f.name(self.db).as_str()));
                path
            }
        }
    }

    pub fn const_path_cs(&self, adt: hir::Const) -> String {
        let mut path = self.module_class_cs(adt.module(self.db));
        path.push('.');
        path.push_str(&names::method_name(
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
    use crate::codegen::ty::TyFromType;
    use hir::{Adt, Type};
    use hir_def::signatures::TypeAliasSignature;
    use hir_def::{
        AdtId, AssocItemId, GenericDefId, GenericParamId, HasModule, ImplId, ItemContainerId,
        Lookup,
    };
    use hir_ty::display::HirDisplay;
    use hir_ty::next_solver::{
        AnyImplId, Binder, ClauseKind, Const, ConstKind, DbInterner, ErrorGuaranteed, GenericArg,
        GenericArgKind, GenericArgs, ParamEnv, PredicateKind, SolverDefId, Term, TermKind,
        TraitRef, Ty,
    };
    use rustc_type_ir::inherent::{GenericsOf as _, IntoKind, SliceLike, Term as _};
    use rustc_type_ir::solve::{Goal, GoalSource, NoSolution};
    use rustc_type_ir::{AliasTyKind, Interner, PredicatePolarity, TyKind};
    use tracing::debug;

    impl<'db> CodeGenerator<'db> {
        // This tries to resolve `<impl SomeTrait<Assoc = SomeType> as SomeTrait>::Assoc`
        pub(super) fn resolve_assoc_of_impl(&self, assoc_ty: &Type<'db>) -> Type<'db> {
            debug!(
                "resolve_assoc_of_impl: {}",
                assoc_ty.display(self.db, self.display_target())
            );
            self.new_type(self.resolve_assoc_of_impl_impl(assoc_ty.ns_ty()))
        }

        #[tracing::instrument(skip(self))]
        pub(super) fn resolve_assoc_of_impl_impl(&self, assoc_ty: Ty<'db>) -> Ty<'db> {
            let db = self.db;

            let TyKind::Alias(alias) = assoc_ty.kind() else {
                // likely to be already resolved
                return assoc_ty;
            };
            let AliasTyKind::Projection {
                def_id: SolverDefId::TypeAliasId(alias_id),
            } = alias.kind
            else {
                panic!("Tries to assoc but not assoc: {assoc_ty:?}")
            };

            let trait_ = match alias_id.lookup(db).container {
                ItemContainerId::TraitId(t) => t,
                container => {
                    panic!("Projection type alias is defined in non-trait: {container:?}");
                }
            };

            let Some(self_ty) = alias.args.as_slice().first().and_then(|a| a.ty()) else {
                panic!("Tries to assoc but not assoc (no self): {assoc_ty:?}")
            };

            //TypeAlias::from(alias_id).;

            match self_ty.kind() {
                TyKind::Alias(self_ty_alias) => {
                    let AliasTyKind::Opaque { def_id } = self_ty_alias.kind else {
                        return self_ty; // can be projection
                        //panic!("Tries to assoc but not assoc (self is not opaque): {assoc_ty:?}")
                    };
                    #[derive(Debug)]
                    enum Pred<'db> {
                        Ty(Ty<'db>),
                        #[allow(dead_code)]
                        Trait(PredicatePolarity, TraitRef<'db>),
                    }
                    let preds = def_id
                        .expect_opaque_ty()
                        .predicates(db)
                        .iter_instantiated_copied(self.interner, self_ty_alias.args.as_slice())
                        .filter_map(|pred| match pred.kind().skip_binder() {
                            ClauseKind::Projection(proj)
                                if proj
                                    .projection_term
                                    .args
                                    .as_slice()
                                    .first()
                                    .and_then(|x| x.ty())
                                    == Some(self_ty)
                                    && proj.def_id() == SolverDefId::TypeAliasId(alias_id) =>
                            {
                                match proj.term.kind() {
                                    TermKind::Ty(ty) => Some(Pred::Ty(ty)),
                                    TermKind::Const(_) => {
                                        unreachable!("Associated type is not type")
                                    }
                                }
                            }
                            ClauseKind::Trait(trait_)
                                if (trait_.trait_ref.args.as_slice())
                                    .first()
                                    .and_then(|x| x.ty())
                                    == Some(assoc_ty) =>
                            {
                                Some(Pred::Trait(trait_.polarity, trait_.trait_ref))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();

                    // If there is <Assoc = SomeType> part, we pick the type
                    if let Some(pred) = preds.iter().find_map(|x| match x {
                        Pred::Ty(ty) => Some(ty),
                        _ => None,
                    }) {
                        return *pred;
                    }

                    // If there is no such, we need create impl SpecifiedTraits
                    if cfg!(false) {
                        /*
                        ImplTraitId::TypeAliasImplTrait(type_alias_id, )
                        Ty::new(
                            self.interner,
                            TyKind::Alias(AliasTy::new(self.interner, AliasTyKind::Opaque { def_id })),
                        )
                        // */
                    }
                    unimplemented!(
                        "{assoc_ty_rs}",
                        assoc_ty_rs = assoc_ty.display(self.db, self.display_target()),
                    )
                }
                TyKind::Adt(adt, args) => {
                    let mut resolved_impl_assoc = None;

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

                    resolved_impl_assoc.unwrap()
                }
                TyKind::Error(_) => assoc_ty,
                _ => {
                    eprintln!(
                        "Unsupported assoc type resolution but not assoc (self is not alias): {assoc_ty:?}"
                    );
                    Ty::new(self.interner, TyKind::Error(ErrorGuaranteed))
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
    pub fn alias_of_type_params(ty: &Type) -> Option<(hir::TypeParam, hir::TypeAlias)> {
        if let TyKind::Alias(alias) = ty.ns_ty().kind()
            && let AliasTyKind::Projection { def_id } = alias.kind
            && let SolverDefId::TypeAliasId(alias_id) = def_id
            && let TyKind::Param(param) = alias.args.as_slice()[0].expect_ty().kind()
        {
            let param = hir::TypeParam::from(param.id);
            let alias = hir::TypeAlias::from(alias_id);

            Some((param, alias))
        } else {
            None
        }
    }
}

pub trait TyFromType<'db> {
    fn ns_ty(&self) -> hir::next_solver::Ty<'db>;
    fn from_ty(
        ty: hir::next_solver::Ty<'db>,
        db: &'db dyn HirDatabase,
        krate: base_db::Crate,
    ) -> Self;
}

mod ty_and_type {
    use crate::codegen::ty::TyFromType;
    use hir_def::CallableDefId;
    use hir_def::resolver::{HasResolver, Resolver};
    use hir_ty::ParamEnvAndCrate;
    use hir_ty::db::HirDatabase;
    use hir_ty::next_solver::{ParamEnv, Ty};
    use ide_db::base_db;
    use ide_db::base_db::{CrateOrigin, LangCrateOrigin, all_crates};

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

        fn from_ty(ty: Ty<'db>, db: &'db dyn HirDatabase, krate: base_db::Crate) -> Self {
            unsafe {
                std::mem::transmute::<TypeMap<'db>, Self>(TypeMap {
                    env: ty_env(db, krate, ty),
                    ty,
                })
            }
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
