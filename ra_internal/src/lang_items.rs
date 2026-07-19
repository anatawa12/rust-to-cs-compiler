#![allow(non_snake_case)]

use base_db::{Crate, CrateOrigin, LangCrateOrigin, all_crates};
use hir_def::lang_item::{LangItems as HirDefLangItems, lang_items};
use hir_ty::db::HirDatabase;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct LangItems {
    inner: HirDefLangItems,
    send: Option<hir::Trait>,
    into: Option<hir::Trait>,
    cow: Option<hir::Enum>,
    cow_owned: Option<hir::EnumVariant>,
    cow_borrowed: Option<hir::EnumVariant>,
}

impl LangItems {
    pub fn new(db: &dyn HirDatabase, krate: hir::Crate) -> &Self {
        #[salsa_macros::tracked(returns(ref))]
        fn new(db: &dyn HirDatabase, krate: Crate) -> LangItems {
            let send: Option<hir::Trait>;
            let into: Option<hir::Trait>;
            let cow: Option<hir::Enum>;
            let cow_owned: Option<hir::EnumVariant>;
            let cow_borrowed: Option<hir::EnumVariant>;

            {
                trait ItemInNsConvert: Sized {
                    fn into_ns(value: hir::ItemInNs) -> Option<Self>;
                }

                macro_rules! item_in_ns_convert_impl {
                    ($ty: path as ($($convert: path),+)) => {
                        impl ItemInNsConvert for $ty {
                            fn into_ns(value: hir::ItemInNs) -> Option<Self> {
                                $(let value = variant_or_none!(value, $convert)?;)+
                                Some(value)
                            }
                        }
                    };
                }

                item_in_ns_convert_impl!(
                    hir::Trait as (hir::ItemInNs::Types, hir::ModuleDef::Trait)
                );
                item_in_ns_convert_impl!(
                    hir::Enum as (hir::ItemInNs::Types, hir::ModuleDef::Adt, hir::Adt::Enum)
                );
                item_in_ns_convert_impl!(
                    hir::EnumVariant as (hir::ItemInNs::Types, hir::ModuleDef::EnumVariant)
                );

                macro_rules! resolve_item {
                    ($crate_:ident ::$($path:ident)::+ as $ty: ty) => {{
                        {
                            #![allow(unused)]
                            extern crate alloc;
                            use $crate_::$($path)::+;
                        }
                        $crate_.and_then(|crate_| crate_
                            .root_module(db)
                            .resolve_mod_path(
                                db,
                                [$( ::hir::Name::new_root(stringify!($path))),+ ],
                            )
                            .into_iter()
                            .flatten()
                            .flat_map(|x| <$ty as ItemInNsConvert>::into_ns(x))
                            .next()
                        )
                    }};
                }

                let all_crates = all_crates(db);
                macro_rules! find_crate {
                    (|$origin:ident| $expr: expr) => {
                        all_crates
                            .iter()
                            .copied()
                            .find(|&krate| {
                                let $origin = &krate.data(db).origin;
                                $expr
                            })
                            .map(hir::Crate::from)
                    };
                }

                let core = find_crate!(|origin| {
                    matches!(origin, CrateOrigin::Lang(LangCrateOrigin::Core))
                });
                let alloc = find_crate!(|origin| {
                    matches!(origin, CrateOrigin::Lang(LangCrateOrigin::Alloc))
                });

                send = resolve_item!(core::marker::Send as hir::Trait);
                into = resolve_item!(core::convert::Into as hir::Trait);
                cow = resolve_item!(alloc::borrow::Cow as hir::Enum);
                cow_borrowed = resolve_item!(alloc::borrow::Cow::Borrowed as hir::EnumVariant);
                cow_owned = resolve_item!(alloc::borrow::Cow::Owned as hir::EnumVariant);
            }

            LangItems {
                inner: lang_items(db, krate).clone(),
                send,
                into,
                cow,
                cow_owned,
                cow_borrowed,
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
        MetaSized: Trait,
        Copy: Trait,
        Clone: Trait,
        Sync: Trait,

        Iterator: Trait,

        OwnedBox: Struct, // alloc::boxed::Box

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
    pub fn Send(&self) -> Option<hir::Trait> {
        self.send
    }

    pub fn Into(&self) -> Option<hir::Trait> {
        self.into
    }

    pub fn Cow(&self) -> Option<hir::Enum> {
        self.cow
    }

    pub fn CowOwned(&self) -> Option<hir::EnumVariant> {
        self.cow_owned
    }

    pub fn CowBorrowed(&self) -> Option<hir::EnumVariant> {
        self.cow_borrowed
    }
}
