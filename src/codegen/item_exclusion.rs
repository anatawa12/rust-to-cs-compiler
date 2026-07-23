//! This module handles removing specific portion of crate from output.

use cfg::CfgExpr;
use hir::db::HirDatabase;
use hir::{HasAttrs, HasContainer, HasCrate, ModuleSource};

pub trait RustPath: Copy {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String>;
}

impl RustPath for hir::Adt {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        Some(self.module(db).rust_path(db)? + "::" + self.name(db).as_str())
    }
}

impl RustPath for hir::Trait {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        Some(self.module(db).rust_path(db)? + "::" + self.name(db).as_str())
    }
}

impl RustPath for hir::ExternBlock {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        self.module(db).rust_path(db)
    }
}

impl RustPath for hir::Crate {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        Some(self.display_name(db)?.as_str().into())
    }
}

impl RustPath for hir::Impl {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        let self_ty = self.self_ty(db);
        if let Some(adt) = self_ty.as_adt() {
            if let Some(trait_) = self.trait_(db) {
                Some(
                    String::from("<") + &adt.rust_path(db)? + " as " + &trait_.rust_path(db)? + ">",
                )
            } else {
                adt.rust_path(db)
            }
        } else {
            Some(self.module(db).rust_path(db)? + "::<impl>")
        }
    }
}

impl RustPath for hir::ItemContainer {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        match self {
            hir::ItemContainer::Trait(t) => t.rust_path(db),
            hir::ItemContainer::Impl(a) => a.rust_path(db),
            hir::ItemContainer::Module(m) => m.rust_path(db),
            hir::ItemContainer::ExternBlock(e) => e.rust_path(db),
            hir::ItemContainer::Crate(e) => e.rust_path(db),
        }
    }
}

impl RustPath for hir::Function {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        Some(self.container(db).rust_path(db)? + "::" + self.name(db).as_str())
    }
}

impl RustPath for hir::Module {
    fn rust_path(self, db: &dyn HirDatabase) -> Option<String> {
        if self.is_crate_root(db) {
            self.krate(db).rust_path(db)
        } else if let Some(parent) = self.parent(db) {
            Some(parent.rust_path(db)? + "::" + self.name(db)?.as_str())
        } else {
            Some(self.name(db)?.as_str().into())
        }
    }
}

pub fn should_emit(
    adt: impl RustPath + HasAttrs + HasCrate + std::fmt::Debug,
    db: &dyn HirDatabase,
) -> bool {
    let path = adt.rust_path(db);
    eprintln!("path: {path:?} for {adt:?}");
    !matches!(
        path.as_deref(),
        Some(
            "_____dummy_for_format"
                | "serde_core::de::value::SeqAccessDeserializer"
                | "serde_core::de::value::MapAccessDeserializer"
                | "serde_core::de::value::EnumAccessDeserializer"
                | "serde_core::de::value::PairDeserializer"
                | "serde_core::de::value::PairVisitor"
                | "<serde_core::de::value::BorrowedStrDeserializer as serde_core::de::EnumAccess>"
                | "<serde_core::de::value::StringDeserializer as serde_core::de::EnumAccess>"
                | "<serde_core::de::value::CowStrDeserializer as serde_core::de::EnumAccess>"
                | "<serde_core::de::value::StrDeserializer as serde_core::de::EnumAccess>"
                | "<serde_core::de::value::U32Deserializer as serde_core::de::EnumAccess>"
                | "<serde_core::de::value::Error as serde_core::ser::Error>"
                | "<serde_core::de::value::MapDeserializer as serde_core::de::SeqAccess>"
                | "serde_core::de::value::private::unit_only"
                | "serde_core::de::value::private::UnitOnly"
                | "serde_core::de::value::private::map_as_enum"
                | "serde_core::de::value::private::MapAsEnum"
                | "serde_core::de::impls::ArrayVisitor"
                | "serde_core::de::impls::ArrayInPlaceVisitor"
                | "serde_core::ser::impossible"
        )
    ) && !cfg_disabled(adt.attrs(db), adt.krate(db), db)
}

fn cfg_disabled(attrs: hir::AttrsWithOwner, krate: hir::Crate, db: &dyn HirDatabase) -> bool {
    match attrs.cfgs(db) {
        Some(cfg_expr) => krate.cfg(db).check(cfg_expr) == Some(false),
        None => false,
    }
}

pub trait R2csHasAttr: hir::HasCrate {
    fn attrs(self, db: &dyn HirDatabase) -> Option<impl Iterator<Item = syntax::ast::Attr>>;
}
trait R2csHasAttrBlanket {}

impl R2csHasAttrBlanket for hir::Function {}
impl R2csHasAttrBlanket for hir::Adt {}

impl<T> R2csHasAttr for T
where
    T: hir::HasSource + hir::HasCrate + R2csHasAttrBlanket + Copy,
    <T as hir::HasSource>::Ast: syntax::ast::HasAttrs,
{
    fn attrs(self, db: &dyn HirDatabase) -> Option<impl Iterator<Item = syntax::ast::Attr>> {
        use syntax::ast::HasAttrs;
        Some(self.source(db)?.value.attrs())
    }
}

impl R2csHasAttr for hir::Module {
    fn attrs(self, db: &dyn HirDatabase) -> Option<impl Iterator<Item = syntax::ast::Attr>> {
        use syntax::ast::HasAttrs;

        let definition = match self.definition_source(db).value {
            ModuleSource::SourceFile(s) => s.attrs(),
            ModuleSource::Module(m) => m.attrs(),
            ModuleSource::BlockExpr(b) => b.attrs(),
        };
        let declaration = self.declaration_source(db).map(|x| x.value.attrs());

        Some(definition.chain(declaration.into_iter().flatten()))
    }
}

pub fn is_r2cs_native<T>(f: T, db: &dyn HirDatabase) -> bool
where
    T: R2csHasAttr,
{
    use syntax::ast::Meta;
    let krate = f.krate(db);
    let Some(mut attrs) = f.attrs(db) else {
        return false;
    };

    attrs.any(|attr| {
        let attrs = match attr.meta() {
            Some(Meta::CfgAttrMeta(cfg))
                if let Some(cfg_predicate) = cfg.cfg_predicate()
                    && krate.cfg(db).check(&CfgExpr::parse_from_ast(cfg_predicate))
                        == Some(true) =>
            {
                cfg.metas().collect::<Vec<_>>()
            }
            Some(meta) => vec![meta],
            None => vec![],
        };

        attrs.iter().any(|attr| {
            attr.path().is_some_and(|path| {
                let segs: Vec<_> = path.segments().collect();
                match segs.len() {
                    1 => segs[0]
                        .name_ref()
                        .is_some_and(|n| n.text() == "r2cs_native"),
                    2 => {
                        segs[0].name_ref().is_some_and(|n| n.text() == "r2cs")
                            && segs[1].name_ref().is_some_and(|n| n.text() == "native")
                    }
                    _ => false,
                }
            })
        })
    })
}
