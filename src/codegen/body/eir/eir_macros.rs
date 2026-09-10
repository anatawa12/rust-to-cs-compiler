use crate::codegen::body::eir::{EirNode, MutatingEirVisitor};
use std::ops::ControlFlow;

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
    fn accept_mut<V: ?Sized + MutatingEirVisitor>(&mut self, _: &mut V) -> ControlFlow<V::Break>;
}

macro_rules! ref_eir_access {
    ($ty: ty) => {
        impl<'a> EirAccessType<'a> for $ty {
            type Result = &'a $ty;
            fn cast(&'a self) -> &'a $ty {
                self
            }

            fn accept_mut<V: ?Sized + MutatingEirVisitor>(
                &mut self,
                _: &mut V,
            ) -> ControlFlow<V::Break> {
                ControlFlow::Continue(())
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

            fn accept_mut<V: ?Sized + MutatingEirVisitor>(
                &mut self,
                _: &mut V,
            ) -> ControlFlow<V::Break> {
                ControlFlow::Continue(())
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

    fn accept_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> ControlFlow<V::Break> {
        self.as_mut()
            .map(|x| x.accept_mut(visitor))
            .unwrap_or(ControlFlow::Continue(()))
    }
}

impl<'a, T: 'a + for<'b> EirAccessType<'b>> EirAccessType<'a> for Vec<T> {
    type Result = &'a [T];
    fn cast(&'a self) -> Self::Result {
        self
    }

    fn accept_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> ControlFlow<V::Break> {
        for child in self {
            child.accept_mut(visitor)?;
        }
        ControlFlow::Continue(())
    }
}

impl<'a, T: EirNode + 'a> EirAccessType<'a> for T {
    type Result = &'a T;

    fn cast(&'a self) -> &'a T {
        self
    }

    fn accept_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> ControlFlow<V::Break> {
        EirNode::accept_mut(self, visitor)
    }
}

impl<'a, T: 'a + for<'b> EirAccessType<'b>> EirAccessType<'a>
    for super::children::ChildrenContainer<T>
{
    type Result = super::children::Children<'a, T>;

    fn cast(&'a self) -> Self::Result {
        self.iterator()
    }

    fn accept_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> ControlFlow<V::Break> {
        for child in self.iter_mut() {
            child.accept_mut(visitor)?;
        }
        ControlFlow::Continue(())
    }
}

#[macro_export]
macro_rules! def_eir {
    (
        @main [$doller: tt]

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
            $vis enum $enum_name {
                $(
                $(#[manual_construct$($manual_construct_mark:tt)?])?
                $variant,
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

        $vis: vis enum $enum_name: ident {
            $(
            $(#[manual_construct$($manual_construct_mark:tt)?])?
            $variant:ident$(($variant_type: ty))?
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
            $($variant(def_eir!(@fallback [$($variant_type)?] [$variant])),)*
        }

        impl EirNode for $enum_name {
            type AstNode = ast::$enum_name;

            #[allow(dead_code)]
            fn node_info(&self) -> NodeInfo<ast::$enum_name> {
                match self {
                    $(Self::$variant(expr) => ExprInfoCast::cast(&expr.node_info()),)*
                }
            }

            def_eir!(@accept_mut [$enum_name]);

            fn accept_children_mut<V: ?Sized + MutatingEirVisitor>(
                &mut self,
                visitor: &mut V,
            ) -> ControlFlow<V::Break> {
                match self {
                    $(Self::$variant(expr) => expr.accept_children_mut(visitor),)*
                }
            }
        }

        $(
        impl From<def_eir!(@fallback [$($variant_type)?] [$variant])> for $enum_name {
            fn from(x: def_eir!(@fallback [$($variant_type)?] [$variant])) -> Self {
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

    (@accept_mut [Expr]) => {
        #[allow(dead_code)]
        fn accept_mut<V: ?Sized + MutatingEirVisitor>(
            &mut self,
            visitor: &mut V,
        ) -> ControlFlow<V::Break> {
            visitor.visit_expr(self)
        }
    };
    (@accept_mut [Pat]) => {
        #[allow(dead_code)]
        fn accept_mut<V: ?Sized + MutatingEirVisitor>(
            &mut self,
            visitor: &mut V,
        ) -> ControlFlow<V::Break> {
            visitor.visit_pat(self)
        }
    };
    (@accept_mut [$variant: ident] $($rest:tt)*) => {};

    (
        @main [$doller: tt]

        $(#[common_info_ast_ty = $common_info_ast_ty: ty])?
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

            #[allow(dead_code)]
            pub fn into_inner(self) -> <Self as $crate::codegen::body::eir::eir_macros::EirNodeConstructHelper>::RawStruct {
                *self.0
            }
        }

        impl EirNode for $variant {
            type AstNode = def_eir!(@fallback [$($common_info_ast_ty)?] [ast::$variant]);

            fn node_info(&self) -> NodeInfo<Self::AstNode> {
                self.0.node_info.clone()
            }

            #[allow(unused_variables)]
            fn accept_children_mut<V: ?Sized + MutatingEirVisitor>(
                &mut self,
                visitor: &mut V,
            ) -> ControlFlow<V::Break> {
                $($crate::codegen::body::eir::eir_macros::EirAccessType::accept_mut(&mut self.0.$variant_child_name, visitor)?;)*
                ControlFlow::Continue(())
            }
        }

        const _: () = {
            mod raw_struct {
                #[allow(unused_imports)]
                use super::*;
                #[allow(dead_code)]
                #[derive(Clone)]
                pub struct $variant {
                    pub node_info: NodeInfo<<super::$variant as EirNode>::AstNode>,
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

        $(#[cfg(false)] $($manual_construct_mark)?)?
        impl LowerToEir for ast::$variant {
            type Eir = $variant;

            fn lower_to_eir(self, #[allow(unused)] ctx: &LowerToEirCtx) -> $variant {
                new_eir_node!($variant {
                    $($variant_child_name: $crate::codegen::body::eir::eir_macros::LowerCast::lower_cast(ctx.lower(self.$variant_child_name()), stringify!($variant_child_name)),)*
                    node_info: CommonInfoFrom::common_info_from(self),
                })
            }
        }
    };

    (@fallback $([])* [$($tt:tt)+] $($rest:tt)*) => { $($tt)+ };

    (@ $($tt:tt)* ) => {
        compile_error!(concat!("Bad macro invocation: ", stringify!($($tt)*)));
    };
    ($($tt:tt)*) => {
        def_eir! {
            @main [$] $($tt)*
        }
    }
}
