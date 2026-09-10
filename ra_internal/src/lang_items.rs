#![allow(non_snake_case)]

use base_db::{Crate, CrateOrigin, LangCrateOrigin, all_crates};
use hir_def::lang_item::{LangItems as HirDefLangItems, lang_items};
use hir_ty::db::HirDatabase;

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

trait ItemInNsConvert: Sized {
    fn from_item_in_ns(value: hir::ItemInNs) -> Option<Self>;
}

macro_rules! item_in_ns_convert_impl {
    ($ty: path as ($($convert: path),+)) => {
        impl ItemInNsConvert for $ty {
            fn from_item_in_ns(value: hir::ItemInNs) -> Option<Self> {
                $(let value = variant_or_none!(value, $convert)?;)+
                Some(value)
            }
        }
    };
}

item_in_ns_convert_impl!(hir::Trait as (hir::ItemInNs::Types, hir::ModuleDef::Trait));
item_in_ns_convert_impl!(hir::Enum as (hir::ItemInNs::Types, hir::ModuleDef::Adt, hir::Adt::Enum));
item_in_ns_convert_impl!(
    hir::Struct as (hir::ItemInNs::Types, hir::ModuleDef::Adt, hir::Adt::Struct)
);
item_in_ns_convert_impl!(hir::EnumVariant as (hir::ItemInNs::Types, hir::ModuleDef::EnumVariant));
item_in_ns_convert_impl!(hir::Function as (hir::ItemInNs::Values, hir::ModuleDef::Function));
item_in_ns_convert_impl!(hir::Macro as (hir::ItemInNs::Macros));

macro_rules! resolve_item {
    ($crate_:ident ::$($path:ident)::+, $db: ident) => {{
        $crate_.and_then(|crate_| crate_
            .root_module($db)
            .resolve_mod_path(
                $db,
                [$( ::hir::Name::new_root(stringify!($path))),+ ],
            )
            .into_iter()
            .flatten()
            .flat_map(ItemInNsConvert::from_item_in_ns)
            .next()
        )
    }};
}

macro_rules! find_crate {
    ($all_crates: ident, $db: ident, |$origin:ident| $expr: expr) => {
        $all_crates
            .iter()
            .copied()
            .find(|&krate| {
                let $origin = &krate.data($db).origin;
                $expr
            })
            .map(hir::Crate::from)
    };
}

macro_rules! our_lang_items {
    (
        {
            extern crate $core: ident;
            extern crate $alloc: ident;
            extern crate $std: ident;
            $(extern crate $krate: ident;)*
        }
        $($name: ident = $($path: ident)::* as $ty: ident;)*
    ) => {
        #[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
        pub struct LangItems {
            inner: HirDefLangItems,
            $($name: Option<hir::$ty>,)*
        }

        impl LangItems {

            pub fn new(db: &dyn HirDatabase, krate: hir::Crate) -> &Self {
                #[salsa_macros::tracked(returns(ref))]
                fn new(db: &dyn HirDatabase, krate: Crate) -> LangItems {
                   new_impl(db, krate)
                }
                fn new_impl(db: &dyn HirDatabase, krate: Crate) -> LangItems {
                    let all_crates = all_crates(db);

                    let $core = find_crate!(all_crates, db, |origin| matches!(origin, CrateOrigin::Lang(LangCrateOrigin::Core)));
                    let $alloc = find_crate!(all_crates, db, |origin| matches!(origin, CrateOrigin::Lang(LangCrateOrigin::Alloc)));
                    let $std = find_crate!(all_crates, db, |origin| matches!(origin, CrateOrigin::Lang(LangCrateOrigin::Std)));

                    $(let $krate = find_crate!(all_crates, db, |origin| {
                        matches!(origin, CrateOrigin::Library{ name: name, .. } if name.as_str() == stringify!($krate))
                    });)*

                    LangItems {
                        inner: lang_items(db, krate).clone(),

                        $($name: resolve_item!($($path)::*, db),)*
                    }
                }

                new(db, krate.base())
            }
        }

        impl LangItems {
            $(
                pub fn $name(&self) -> Option<hir::$ty> {
                    self.$name
                }
            )*
        }
    };
}

our_lang_items! {
    {
        extern crate core;
        extern crate alloc;
        extern crate std;
        extern crate log;
        extern crate lazy_static;
        extern crate futures;
    }

    // core
    ready = core::task::ready as Macro;
    pin = core::pin::pin as Macro;
    unreachable = core::unreachable as Macro;
    matches = core::matches as Macro;
    write = core::write as Macro;
    format_args = core::format_args as Macro;
    cfg = core::cfg as Macro;
    Send = core::marker::Send as Trait;
    Into = core::convert::Into as Trait;
    internal_debug_text = core::__r2cs_internal::debug_text as Function;
    internal_display_text = core::__r2cs_internal::display_text as Function;

    // alloc
    Vec = alloc::vec::Vec as Struct;
    vec = alloc::vec as Macro;
    format = alloc::format as Macro;
    Cow = alloc::borrow::Cow as Enum;
    CowBorrowed = alloc::borrow::Cow::Borrowed as EnumVariant;
    CowOwned = alloc::borrow::Cow::Owned as EnumVariant;

    // std
    panic = std::panic as Macro;
    assert = std::assert as Macro;
    OsStr = std::ffi::OsStr as Struct;
    OsString = std::ffi::OsString as Struct;
    IntoIterator = std::iter::IntoIterator as Trait;
    Result = std::result::Result as Enum;

    // log
    log_trace = log::trace as Macro;
    log_debug = log::debug as Macro;
    log_info = log::info as Macro;
    log_warn = log::warn as Macro;
    log_error = log::error as Macro;

    //lazy_static
    lazy_static = lazy_static::lazy_static as Macro;

    // futures
    try_join = futures::try_join as Macro;
}

lang_item_wrapper! {
    struct LangItems {
        Sized: Trait,
        MetaSized: Trait,
        Copy: Trait,
        Clone: Trait,
        Sync: Trait,
        Default: Trait,

        Iterator: Trait,

        OwnedBox: Struct, // alloc::boxed::Box
        String: Struct,

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

        Option: Enum,

        Debug: Trait,
        Hash: Trait,
    }
}
