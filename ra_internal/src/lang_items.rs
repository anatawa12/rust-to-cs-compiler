#![allow(non_snake_case)]

use base_db::{Crate, CrateOrigin, LangCrateOrigin, all_crates};
use hir::{Name, sym};
use hir_def::lang_item::{LangItems as HirDefLangItems, lang_items};
use hir_ty::db::HirDatabase;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct LangItems {
    inner: HirDefLangItems,
    send: Option<hir::Trait>,
}

impl LangItems {
    pub fn new(db: &dyn HirDatabase, krate: hir::Crate) -> &Self {
        #[salsa_macros::tracked(returns(ref))]
        fn new(db: &dyn HirDatabase, krate: Crate) -> LangItems {
            let mut send: Option<hir::Trait> = None;

            if let Some(core) = all_crates(db).iter().copied().find(|&krate| {
                matches!(
                    krate.data(db).origin,
                    CrateOrigin::Lang(LangCrateOrigin::Core)
                )
            }) {
                let core = hir::Crate::from(core);
                send = core
                    .root_module(db)
                    .resolve_mod_path(
                        db,
                        [Name::new_symbol_root(sym::marker), Name::new_root("Send")],
                    )
                    .into_iter()
                    .flatten()
                    .flat_map(|x| variant_or_none!(x, hir::ItemInNs::Types))
                    .flat_map(|x| variant_or_none!(x, hir::ModuleDef::Trait))
                    .next();
            }

            LangItems {
                inner: lang_items(db, krate).clone(),
                send,
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
    pub fn Send(&self) -> Option<hir::Trait> {
        self.send
    }
}
