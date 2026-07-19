#![allow(non_snake_case)]

use base_db::{Crate, CrateOrigin, LangCrateOrigin, all_crates};
use hir_def::lang_item::{LangItems as HirDefLangItems, lang_items};
use hir_ty::db::HirDatabase;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct LangItems {
    inner: HirDefLangItems,
    send: Option<hir::Trait>,
    into: Option<hir::Trait>,
}

impl LangItems {
    pub fn new(db: &dyn HirDatabase, krate: hir::Crate) -> &Self {
        #[salsa_macros::tracked(returns(ref))]
        fn new(db: &dyn HirDatabase, krate: Crate) -> LangItems {
            let mut send: Option<hir::Trait> = None;
            let mut into: Option<hir::Trait> = None;

            if let Some(core) = all_crates(db).iter().copied().find(|&krate| {
                matches!(
                    krate.data(db).origin,
                    CrateOrigin::Lang(LangCrateOrigin::Core)
                )
            }) {
                let core = hir::Crate::from(core);

                trait ItemInNsConvert: Sized {
                    fn into_ns(value: hir::ItemInNs) -> Option<Self>;
                }

                impl ItemInNsConvert for hir::Trait {
                    fn into_ns(value: hir::ItemInNs) -> Option<Self> {
                        variant_or_none!(
                            variant_or_none!(value, hir::ItemInNs::Types)?,
                            hir::ModuleDef::Trait
                        )
                    }
                }

                macro_rules! resolve_item {
                    ($crate_:ident ::$($path:ident)::+ as $ty: ty) => {
                        $crate_
                            .root_module(db)
                            .resolve_mod_path(
                                db,
                                [$( ::hir::Name::new_root(stringify!($path))),+ ],
                            )
                            .into_iter()
                            .flatten()
                            .flat_map(|x| <$ty as ItemInNsConvert>::into_ns(x))
                            .next()
                    };
                }

                send = resolve_item!(core::marker::Send as hir::Trait);
                into = resolve_item!(core::convert::Into as hir::Trait);
            }

            LangItems {
                inner: lang_items(db, krate).clone(),
                send,
                into,
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
}
