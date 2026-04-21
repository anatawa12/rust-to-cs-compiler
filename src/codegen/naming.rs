/// Naming conventions for the Rust → C# transpiler.
///
/// All identifiers generated from Rust names are prefixed to avoid clashes
/// with C# keywords and between different Rust namespaces.  The full table
/// is defined in `design/design-v1.md`.
///
/// | Rust concept                                   | C# prefix |
/// |------------------------------------------------|-----------|
/// | module                                         | `mod_`    |
/// | struct / enum                                  | `s_`      |
/// | empty struct for trait static members (vtable) | `S_`      |
/// | non-dyn trait interface                        | `t_`      |
/// | trait interface incl. dyn/static members       | `T_`      |
/// | generic param: associated type                 | `A_`      |
/// | generic param: Self                            | `Self`    |
/// | generic param: generic parameter               | `P_`      |
/// | field                                          | `f_`      |
/// | associated function / method                   | `m_`      |
/// | local variable                                 | `l_${name}_${index}` |

/// Convert a Rust module name to a C# partial-class name.
pub fn module_name(rust_name: &str) -> String {
    format!("mod_{rust_name}")
}

/// Convert a Rust struct or enum name to a C# struct name.
pub fn struct_name(rust_name: &str) -> String {
    format!("s_{rust_name}")
}

/// Name of the empty vtable struct for a specific trait impl.
/// e.g. `S_Display_for_s_Point` for `impl Display for Point`.
pub fn vtable_struct_name(trait_name: &str, impl_type_name: &str) -> String {
    format!("S_{trait_name}_for_{impl_type_name}")
}

/// Convert a Rust trait name to the C# non-dyn trait interface name.
/// Used for `t_Foo<TSelf>` interfaces.
pub fn trait_iface_name(rust_name: &str) -> String {
    format!("t_{rust_name}")
}

/// Convert a Rust trait name to the C# trait-static-member interface name.
/// Used for vtable dispatch (`T_Foo` interface implemented by `S_Foo_for_*`).
pub fn trait_static_iface_name(rust_name: &str) -> String {
    format!("T_{rust_name}")
}

/// Convert a Rust generic type parameter to a C# generic parameter name.
pub fn generic_param_name(rust_name: &str) -> String {
    format!("P_{rust_name}")
}

/// Convert a Rust associated type name to a C# generic parameter name.
pub fn assoc_type_param_name(rust_name: &str) -> String {
    format!("A_{rust_name}")
}

/// Convert a Rust field name to a C# field name.
pub fn field_name(rust_name: &str) -> String {
    format!("f_{rust_name}")
}

/// Convert a Rust method / associated function name to a C# method name.
pub fn method_name(rust_name: &str) -> String {
    format!("m_{rust_name}")
}

/// Generate a C# local variable name.
/// `index` disambiguates shadowed locals with the same Rust name.
pub fn local_name(rust_name: &str, index: usize) -> String {
    format!("l_{rust_name}_{index}")
}

/// The `TSelf` generic parameter used in all trait interfaces.
pub const SELF_PARAM: &str = "Self";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_name_adds_prefix() {
        assert_eq!(module_name("my_module"), "mod_my_module");
    }

    #[test]
    fn struct_name_adds_prefix() {
        assert_eq!(struct_name("Point"), "s_Point");
    }

    #[test]
    fn vtable_struct_name_correct() {
        assert_eq!(
            vtable_struct_name("Display", "s_Point"),
            "S_Display_for_s_Point"
        );
    }

    #[test]
    fn trait_iface_name_correct() {
        assert_eq!(trait_iface_name("Iterator"), "t_Iterator");
    }

    #[test]
    fn trait_static_iface_name_correct() {
        assert_eq!(trait_static_iface_name("Display"), "T_Display");
    }

    #[test]
    fn generic_param_name_correct() {
        assert_eq!(generic_param_name("T"), "P_T");
    }

    #[test]
    fn assoc_type_param_name_correct() {
        assert_eq!(assoc_type_param_name("Item"), "A_Item");
    }

    #[test]
    fn field_name_correct() {
        assert_eq!(field_name("value"), "f_value");
    }

    #[test]
    fn method_name_correct() {
        assert_eq!(method_name("new"), "m_new");
    }

    #[test]
    fn local_name_includes_index() {
        assert_eq!(local_name("x", 0), "l_x_0");
        assert_eq!(local_name("x", 1), "l_x_1");
    }

    #[test]
    fn self_param_is_constant() {
        assert_eq!(SELF_PARAM, "Self");
    }
}
