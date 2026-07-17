/// Filters a single expression to a single enum variant or returns None
#[macro_export]
macro_rules! variant_or_none {
    ($expr: expr, $variant: path) => {
        match $expr {
            $variant(v) => Some(v),
            _ => None,
        }
    };
}

mod internal;

mod assoc_item;
mod debug;
pub mod function;
mod impl_;
mod lang_items;
mod trait_;
mod trait_ref;
mod ty;
mod ty_map;
pub mod type_alias;
mod type_param;

pub use assoc_item::*;
pub use debug::*;
pub use impl_::*;
pub use lang_items::*;
pub use trait_::*;
pub use trait_ref::*;
pub use ty::*;
pub use ty_map::*;
pub use type_param::*;

pub use crate::ImplExt as _;
pub use crate::TraitExt as _;
pub use crate::TypeExt as _;
pub use crate::TypeParamExt as _;
