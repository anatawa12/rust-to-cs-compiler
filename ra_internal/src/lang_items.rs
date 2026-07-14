#![allow(non_snake_case)]

use base_db::{Crate, CrateOrigin, LangCrateOrigin, all_crates};
use hir::{Name, sym};
use hir_def::lang_item::{LangItems as HirDefLangItems, lang_items};
use hir_ty::db::HirDatabase;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct LangItems {
    inner: HirDefLangItems,
}

impl LangItems {
    pub fn new(db: &dyn HirDatabase, krate: hir::Crate) -> &Self {
        #[salsa_macros::tracked(returns(ref))]
        fn new(db: &dyn HirDatabase, krate: Crate) -> LangItems {
            LangItems {
                inner: lang_items(db, krate).clone(),
            }
        }

        new(db, krate.base())
    }
}

macro_rules! lang_item_wrapper {
    (
        struct $name:ident {
            $($field:ident: $ty:ident, )*
        }
    ) => {
        impl LangItems {
            $(pub fn $field(&self) -> Option<hir::$ty> {
                self.inner.$field.map(<hir::$ty>::from)
            })*
        }
    };
}

lang_item_wrapper! {
    struct LangItems {
        Sized: Trait,
        Copy: Trait,
        Sync: Trait,

        Fn: Trait,
        FnMut: Trait,
        FnOnce: Trait,
        FnOnceOutput: TypeAlias,

        Future: Trait,
        FutureOutput: TypeAlias,
        Unpin: Trait,

        PartialEq: Trait,
        PartialOrd: Trait,
        Ord: Trait,

        Debug: Trait,
        Hash: Trait,
    }
}

impl LangItems {
}
