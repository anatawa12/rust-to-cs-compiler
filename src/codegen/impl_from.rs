macro_rules! impl_from {
    ($($($variant: ident)::+ $(($($($sub_variant: ident)::+),*))?),* for $enum:ident) => {
        $(
            impl From<$($variant)::+> for $enum {
                fn from(it: $($variant)::+) -> $enum {
                    impl_from!(@variant $enum $($variant)::+)(it)
                }
            }
            impl_from!(@sub_variant_impl $enum $($variant)::+ $($($($sub_variant)::+)*)?);
        )*
    };

    (@sub_variant_impl $enum:ident $($variant: ident)::+) => {
    };
    (@sub_variant_impl $enum:ident $($variant: ident)::+ $($sub_variant: ident)::+ $($($sub_variant_rest: ident)::+)*) => {
        impl From<$($sub_variant)::+> for $enum {
            fn from(it: $($sub_variant)::+) -> $enum {
                impl_from!(@variant $enum $($variant)::+)(impl_from!(@variant $($variant)::+ $($sub_variant)::+)(it))
            }
        }
        impl_from!(@sub_variant_impl $enum $($variant)::+ $($($sub_variant_rest)::+)*);
    };

    (@variant $($enum:ident)::+ $variant: ident) => {
        $($enum)::+::$variant
    };
    (@variant $($enum:ident)::+ $remove: ident :: $($variant: ident)::+) => {
        impl_from!(@variant $($enum)::+ $($variant)::+)
    }
}
