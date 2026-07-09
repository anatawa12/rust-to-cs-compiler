use crate::codegen::CodeGenerator;
use hir::{HasContainer, HasName, ItemContainer, ModuleDef};
/// Naming convention helpers for Rust → C# name mangling.
use std::collections::HashMap;

/// Converts a Rust identifier to a C# identifier per the naming conventions.
/// All names are converted from snake_case to PascalCase within the naming prefix.
pub fn struct_name(rust_name: &str) -> String {
    format!("s_{}", pascal(rust_name))
}

/// Empty struct for trait static members (vtable).
pub fn static_struct_name(rust_name: &str) -> String {
    format!("S_{}", pascal(rust_name))
}

/// Non-dyn trait interface.
pub fn trait_name(rust_name: &str) -> String {
    format!("t_{}", pascal(rust_name))
}

/// Dyn-capable trait interface (with static member support).
pub fn dyn_trait_name(rust_name: &str) -> String {
    format!("T_{}", pascal(rust_name))
}

/// Enum variant name within an enum class.
pub fn variant_name(rust_name: &str) -> String {
    format!("v_{}", pascal(rust_name))
}

/// Module → partial class prefix.
fn mod_name(rust_name: &str) -> String {
    format!("mod_{}", pascal(rust_name))
}

/// Module → partial class prefix.
fn crate_name(rust_name: &str) -> String {
    format!("crt_{}", pascal(rust_name))
}

/// Field name.
pub fn field_name(rust_name: &str) -> String {
    format!("f_{}", camel(rust_name))
}

/// Associated function / method name.
fn method_name(rust_name: &str) -> String {
    format!("m_{}", pascal(rust_name))
}

/// Const
pub fn const_name(rust_name: &str) -> String {
    format!("c_{}", rust_name)
}

/// Local variable name with uniqueness index.
pub fn local_name(rust_name: &str, index: usize) -> String {
    format!("l_{}_{}", camel(rust_name), index)
}

/// Generic type parameter.
pub fn generic_param(rust_name: &str) -> String {
    format!("P_{}", rust_name)
}

/// Associated type generic parameter.
pub fn assoc_type_param(rust_name: &str) -> String {
    format!("A_{}", rust_name)
}

/// Convert snake_case → PascalCase.
pub fn pascal(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

/// Convert snake_case → camelCase.
pub fn camel(s: &str) -> String {
    let mut parts = s.split('_').filter(|p| !p.is_empty());
    match parts.next() {
        None => String::new(),
        Some(first) => {
            let rest: String = parts
                .map(|p| {
                    let mut c = p.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect();
            format!("{}{}", first, rest)
        }
    }
}

impl CodeGenerator<'_> {
    pub fn field_name(&self, field: &hir::Field) -> String {
        field_name(field.name(self.db).as_str())
    }

    pub fn function_name(&self, function: hir::Function) -> String {
        // We name special name for some special trait implementation
        let db = self.db;
        match function.container(db) {
            ItemContainer::Impl(impl_) => {
                if impl_.trait_(db) == self.lang_items.Debug.map(Into::into)
                    && function.name(db) == hir::sym::fmt
                {
                    return "DebugFmt".into();
                }

                //if impl_.trait_(db) == self.lang_items.Display.map(Into::into) {
                if impl_
                    .trait_(db)
                    .map(|x| x.name(db))
                    .as_ref()
                    .map(|x| x.as_str())
                    == Some("Display")
                    && function.name(db) == hir::sym::fmt
                {
                    return "DisplayFmt".into();
                }
            }
            ItemContainer::Trait(trait_) => {
                if Some(trait_) == self.lang_items.Debug.map(Into::into)
                    && function.name(db) == hir::sym::fmt
                {
                    return "DebugFmt".into();
                }

                //if impl_.trait_(db) == self.lang_items.Display.map(Into::into) {
                if trait_.name(db).as_str() == "Display" && function.name(db) == hir::sym::fmt {
                    return "DisplayFmt".into();
                }
            }
            ItemContainer::Module(_) => {}
            ItemContainer::ExternBlock(_) => {}
            ItemContainer::Crate(_) => {}
        }

        method_name(function.name(self.db).as_str())
    }

    pub fn mod_simple_name(&self, module: hir::Module) -> String {
        self.mod_simple_name_impl(module, None)
    }
    fn mod_simple_name_impl(&self, module: hir::Module, child: Option<hir::Module>) -> String {
        if module.is_crate_root(self.db) {
            let crate_name = module.krate(self.db).display_name(self.db);
            let crate_name = crate_name.as_ref().map(|x| x.as_str()).unwrap_or_else(|| {
                eprintln!("Unsupported: module (crate) does not have a name");
                return "unnamed_crate";
            });
            self::crate_name(&crate_name)
        } else {
            let Some(parent) = module.parent(self.db) else {
                panic!(
                    "Non-crate root module: at {}",
                    self.location_with_file(module.definition_source(self.db))
                );
                return "unexpected_root_module".to_owned();
            };
            if let Some(name) = module.name(self.db) {
                let mut base_name = mod_name(name.as_str());
                if parent.name(self.db) != module.name(self.db) {
                    base_name
                } else {
                    let parent_name = self.mod_simple_name_impl(parent, Some(module));
                    if parent_name == base_name {
                        base_name.push('_');
                    }
                    base_name
                }
            } else {
                eprintln!(
                    "Unsupported: module (crate) does not have a name at {:?} ({module:?})",
                    self.location_with_file(module.definition_source(self.db))
                );
                "unnamed_mod".to_owned()
            }
        }
    }
}

/// Tracks local variable name → counter for uniqueness.
#[derive(Default)]
pub struct LocalNameMap {
    counts: HashMap<String, usize>,
}

impl LocalNameMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a unique C# name for the given Rust binding name.
    pub fn alloc(&mut self, rust_name: &str) -> String {
        let count = self.counts.entry(rust_name.to_string()).or_insert(0);
        let result = local_name(rust_name, *count);
        *count += 1;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pascal() {
        assert_eq!(pascal("foo_bar"), "FooBar");
        assert_eq!(pascal("my_struct"), "MyStruct");
        assert_eq!(pascal("hello"), "Hello");
    }

    #[test]
    fn test_names() {
        assert_eq!(struct_name("my_struct"), "s_MyStruct");
        assert_eq!(trait_name("my_trait"), "t_MyTrait");
        assert_eq!(method_name("do_thing"), "m_DoThing");
        assert_eq!(field_name("my_field"), "f_myField");
    }
}
