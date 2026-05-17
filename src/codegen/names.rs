use crate::codegen::CodeGenerator;
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
pub fn mod_name(rust_name: &str) -> String {
    format!("mod_{}", pascal(rust_name))
}

/// Field name.
pub fn field_name(rust_name: &str) -> String {
    format!("f_{}", camel(rust_name))
}

/// Associated function / method name.
pub fn method_name(rust_name: &str) -> String {
    format!("m_{}", pascal(rust_name))
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

    pub fn function_name(&self, function: &hir::Function) -> String {
        method_name(function.name(self.db).as_str())
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
