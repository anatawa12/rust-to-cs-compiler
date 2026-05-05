use super::{CodeGenerator, names};
use crate::codegen::names::mod_name;
/// Converts Rust HIR types to C# type strings.
use hir::db::HirDatabase;
use hir::{Adt, BuiltinType, Module, Name, Trait, Type};
use hir_def::resolver::HasResolver;
use hir_ty::display::HirDisplay;
use syntax::SyntaxNodePtr;

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
        if let Some((inner, _size)) = ty.as_array(db) {
            return format!("{}[]", self.rust_type_to_cs_inner(&inner, false));
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
            } else {
                names::generic_param(&name)
            }
        } else if let Some(impl_traits) = ty.as_impl_traits(db) {
            let traits = impl_traits.collect::<Vec<_>>();
            if traits.len() > 1 {
                eprintln!("Multiple traits are used: {:?}", ty);
            }
            if traits.is_empty() {
                eprintln!("empty impl traits: {:?}", ty);
                "object".to_string()
            } else {
                self.trait_itf_cs(traits[0])
            }
        } else {
            // Closure types, fn pointers, etc. — use Action/Func
            if ty.is_fn() {
                "Action".to_string()
            } else if ty.is_closure() {
                "Action".to_string()
            } else {
                eprintln!("Unsupported type: {:?}", ty);
                format!("object /*Unsupported type:  {ty:?}*/").to_string()
            }
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
                hir::GenericParam::TypeParam(tp) => {
                    let name = names::generic_param(tp.name(db).as_str());
                    type_params.push(name.clone());
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

    pub fn trait_itf_cs(&self, t: Trait) -> String {
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

    pub fn fn_path_cs(&self, adt: hir::Function) -> String {
        let mut path = self.module_class_cs(adt.module(self.db));
        path.push('.');
        path.push_str(&names::method_name(adt.name(self.db).as_str()));
        path
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

    pub fn enum_variant_cs(&self, v: hir::EnumVariant) -> String {
        let mut path = self.rust_type_to_cs(&v.parent_enum(self.db).ty(self.db));
        path.push('.');
        path.push_str(&names::variant_name(v.name(self.db).as_str()));
        path
    }

    pub fn constructable_name_cs(&self, c: Constructable) -> String {
        match c {
            Constructable::Struct(s) => self.adt_name_cs(s.into()),
            Constructable::EnumVariant(v) => self.enum_variant_cs(v.into()),
        }
    }
}

// TODO: Generic Args
#[derive(Debug, Clone, Copy)]
pub enum Constructable {
    Struct(hir::Struct),
    EnumVariant(hir::EnumVariant),
}

impl Constructable {
    pub fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
        match self {
            Constructable::Struct(s) => s.fields(db),
            Constructable::EnumVariant(v) => v.fields(db),
        }
    }
}
