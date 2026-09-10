use crate::codegen::body::eir::EirNode;

pub trait EirNodeConstructHelper {
    type RawStruct;
    fn construct_from_raw(raw: Self::RawStruct) -> Self;
}

/// The Helper to construct the RawStruct, but current rust disallows
/// <T as EirNodeConstructHelper>::RawStruct so we use this type alias to mention
/// RawStruct without qualified path name.
#[doc(hidden)]
pub type EirNodeConstructHelperAlias<T> = <T as EirNodeConstructHelper>::RawStruct;

#[macro_export]
macro_rules! new_eir_node {
    ($ty: ty { $($body:tt)* }) => {
        <$ty as $crate::codegen::body::eir::eir_macros::EirNodeConstructHelper>::construct_from_raw(
            $crate::codegen::body::eir::eir_macros::EirNodeConstructHelperAlias::<$ty> {
                $($body)*
            }
        )
    };
}

#[macro_export]
macro_rules! eir_children {
    ($($tt: tt)*) => {
        $crate::codegen::body::eir::ChildrenContainer::from(::std::vec![$($tt)*])
    };
}

/// The helper trait for def_eir to cast T => T or Option<T> to T depending on necessary type.
/// In AST world, everything is optional so we need to 'lower' the types
pub(super) trait LowerCast<Eir> {
    fn lower_cast(self, field: &'static str) -> Eir;
}

impl<Eir> LowerCast<Eir> for Eir {
    fn lower_cast(self, _: &'static str) -> Eir {
        self
    }
}

impl<Eir> LowerCast<Eir> for Option<Eir> {
    fn lower_cast(self, field: &'static str) -> Eir {
        self.unwrap_or_else(|| panic!("{field} not exists"))
    }
}

/// The helper trait for determining the type of eir child access method
pub trait EirAccessType<'a> {
    type Result;
    fn cast(&'a self) -> Self::Result;
}

macro_rules! ref_eir_access {
    ($ty: ty) => {
        impl<'a> EirAccessType<'a> for $ty {
            type Result = &'a $ty;
            fn cast(&'a self) -> &'a $ty {
                self
            }
        }
    };
}
macro_rules! clone_eir_access {
    ($ty: ty) => {
        impl<'a> EirAccessType<'a> for $ty {
            type Result = $ty;
            fn cast(&self) -> $ty {
                self.clone()
            }
        }
    };
}

ref_eir_access!(crate::codegen::output::Code);
clone_eir_access!(bool);
clone_eir_access!(super::BinaryOp);
clone_eir_access!(super::UnaryOp);
clone_eir_access!(super::RangeOp);
clone_eir_access!(super::BlockModifier);
clone_eir_access!(super::LiteralKind);

impl<'a, T: EirAccessType<'a> + 'a> EirAccessType<'a> for Option<T> {
    type Result = Option<T::Result>;
    fn cast(&'a self) -> Self::Result {
        self.as_ref().map(EirAccessType::cast)
    }
}

impl<'a, T: 'a> EirAccessType<'a> for Vec<T> {
    type Result = &'a [T];
    fn cast(&'a self) -> Self::Result {
        self
    }
}

impl<'a, T: EirNode + 'a> EirAccessType<'a> for T {
    type Result = &'a T;

    fn cast(&'a self) -> &'a T {
        self
    }
}

impl<'a, T: 'a> EirAccessType<'a> for super::children::ChildrenContainer<T> {
    type Result = super::children::Children<'a, T>;

    fn cast(&'a self) -> Self::Result {
        self.iterator()
    }
}

#[macro_export]
macro_rules! def_eir {
    (
        @main [$doller: tt]

        #[common_info]
        $vis: vis enum $enum_name: ident {
            $(
            // this attribute notes that
            $(#[manual_construct$($manual_construct_mark:tt)?])?
            $(#[common_info_ast_ty = $common_info_ast_ty: ty])?
            $variant:ident {
                $($variant_body: tt)*
            }
            ),* $(,)?
        }

        fn lower_to_eir(self, $ctx: ident: &LowerToEirCtx) -> Eir {
            $(match self {
                $(
                $(#[pre_match$($pre_match:tt)?])? $lower_pat: pat $(
                    if $($(let $lower_guard_let: pat =)? ($lower_guard: expr))&&*
                )? => $lower_expr: expr $(,)?
                )*
            })?
        }
    ) => {
        def_eir!(
            #[common_info]
            $vis enum $enum_name {
                $(
                $(#[manual_construct$($manual_construct_mark:tt)?])?
                $variant(_),
                )*
            }

            fn lower_to_eir(self, $ctx: &LowerToEirCtx) -> Eir {
                $(match self {
                    $(
                    $(#[pre_match$($pre_match)?])? $lower_pat $(
                        if $($(let $lower_guard_let =)? ($lower_guard))&&*
                    )? => $lower_expr,
                    )*
                })?
            }
        );

        $(
        def_eir!(
            #[common_info]
            $(#[common_info_ast_ty = $common_info_ast_ty])?
            $(#[manual_construct$($manual_construct_mark:tt)?])?
            $vis struct $variant {
                $($variant_body)*
            }
        );
        )*
    };

    (
        @main [$doller: tt]

        $(#[common_info $($common_info_marker:tt)?])?
        $vis: vis enum $enum_name: ident {
            $(
            $(#[manual_construct$($manual_construct_mark:tt)?])?
            $variant:ident (_)
            ),* $(,)?
        }

        fn lower_to_eir(self, $ctx: ident: &LowerToEirCtx) -> Eir {
            $(match self {
                $(
                $(#[pre_match$($pre_match:tt)?])? $lower_pat: pat $(
                    if $($(let $lower_guard_let: pat =)? ($lower_guard: expr))&&*
                )? => $lower_expr: expr $(,)?
                )*
            })?
        }
    ) => {
        #[allow(dead_code)]
        #[allow(clippy::enum_variant_names)]
        #[derive(Clone)]
        $vis enum $enum_name {
            $($variant($variant),)*
        }

        def_eir!(@impl_eir_node [$(common_info $($common_info_marker:)?)?] [$enum_name] [$($variant)*]);

        $(
        impl From<$variant> for $enum_name {
            fn from(x: $variant) -> Self {
                $enum_name::$variant(x)
            }
        }
        )*

        impl LowerToEir for ast::$enum_name {
            type Eir = $enum_name;

            fn lower_to_eir(self, $ctx: &LowerToEirCtx) -> $enum_name {
                match self {
                    //$($($pre_lower_pat $(if $pre_lower_guard)? => $pre_lower_expr,)*)?
                    $($( #[cfg(any($(true$($pre_match)?)?))] $lower_pat $(if $($(let $lower_guard_let =)? ($lower_guard))&&*)? => $lower_expr,)*)?
                    $(
                    $(#[cfg(false)] $($manual_construct_mark)?)?
                    Self::$variant(expr)
                        => $enum_name::from($ctx.lower(expr)),
                    )*
                    $($( $(#[cfg(false)] $($pre_match)?)? $lower_pat $(if $($(let $lower_guard_let =)? ($lower_guard))&&*)? => $lower_expr,)*)?
                }
            }
        }
    };

    (
        @main [$doller: tt]

        $(
        #[common_info $($common_info:tt)?]
        $(#[common_info_ast_ty = $common_info_ast_ty: ty])?
        )?
        $(#[manual_construct$($manual_construct_mark:tt)?])?
        $vis: vis struct $variant: ident {
            $($variant_child_name: ident: $variant_child_ty: ty),* $(,)?
        }
    ) => {
        #[allow(dead_code)]
        #[derive(Clone)]
        $vis struct $variant(Box<<$variant as $crate::codegen::body::eir::eir_macros::EirNodeConstructHelper>::RawStruct>);

        impl $variant {
            $(
            #[allow(dead_code)]
            pub fn $variant_child_name(&self) -> <$variant_child_ty as $crate::codegen::body::eir::eir_macros::EirAccessType<'_>>::Result {
                $crate::codegen::body::eir::eir_macros::EirAccessType::cast(&self.0.$variant_child_name)
            }
            )*
        }

        $(
        impl EirNode for $variant {
            type AstNode = def_eir!(@common_info_type [$($common_info_ast_ty)?] [ast::$variant]);

            fn node_info(&self) -> NodeInfo<Self::AstNode> {
                self.0.node_info.clone()
            }
        }
        )?

        const _: () = {
            mod raw_struct {
                #[allow(unused_imports)]
                use super::*;
                #[allow(dead_code)]
                #[derive(Clone)]
                pub struct $variant {
                    $(pub node_info: NodeInfo<def_eir!(@common_info_type [$($common_info_ast_ty)?] [ast::$variant])>,)?
                    $(pub $variant_child_name: $variant_child_ty,)*
                }
            }

            impl $crate::codegen::body::eir::eir_macros::EirNodeConstructHelper for $variant {
                type RawStruct = raw_struct::$variant;

                fn construct_from_raw(raw: Self::RawStruct) -> Self {
                    Self(Box::new(raw))
                }
            }
        };

        def_eir!(@lower_to_eir_variant [$(manual_construct $($manual_construct_mark:tt)?)?] [$variant] [$($variant_child_name)*] [$(node_info $($common_info)?)?]);
    };

    (@impl_eir_node [] [$enum_name: ident] [$($variant: ident)*]) => {};
    (@impl_eir_node [common_info] [$enum_name: ident] [$($variant: ident)*]) => {
        impl EirNode for $enum_name {
            type AstNode = ast::$enum_name;

            #[allow(dead_code)]
            fn node_info(&self) -> NodeInfo<ast::$enum_name> {
                match self {
                    $(Self::$variant(expr) => ExprInfoCast::cast(&expr.node_info()),)*
                }
            }
        }
    };

    (@lower_to_eir_variant [manual_construct] $($tt: tt)*) => {};
    (@lower_to_eir_variant [] [$variant: ident] [$($variant_child_name: ident)*] [$($common_info_name: ident)?]) => {
        impl LowerToEir for ast::$variant {
            type Eir = $variant;

            fn lower_to_eir(self, #[allow(unused)] ctx: &LowerToEirCtx) -> $variant {
                new_eir_node!($variant {
                    $($variant_child_name: $crate::codegen::body::eir::eir_macros::LowerCast::lower_cast(ctx.lower(self.$variant_child_name()), stringify!($variant_child_name)),)*
                    $($common_info_name: CommonInfoFrom::common_info_from(self),)?
                })
            }
        }
    };

    (@common_info_type [] [$ty: ty]) => { $ty };
    (@common_info_type [$ty: ty] [$($tt:tt)*]) => { $ty };

    (@ $($tt:tt)* ) => {
        compile_error!(concat!("Bad macro invocation: ", stringify!($($tt)*)));
    };
    ($($tt:tt)*) => {
        def_eir! {
            @main [$] $($tt)*
        }
    }
}
