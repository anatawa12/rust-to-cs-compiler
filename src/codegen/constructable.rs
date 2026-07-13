use hir::db::HirDatabase;
use hir::{Adt, Crate, HasCrate, Module, Name, Type};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ConstructableDef {
    Struct(hir::Struct),
    EnumVariant(hir::EnumVariant),
}

impl From<hir::Struct> for ConstructableDef {
    fn from(value: hir::Struct) -> Self {
        ConstructableDef::Struct(value)
    }
}

impl From<hir::EnumVariant> for ConstructableDef {
    fn from(value: hir::EnumVariant) -> Self {
        ConstructableDef::EnumVariant(value)
    }
}

impl ConstructableDef {
    pub fn from_variant(variant: hir::Variant) -> Option<Self> {
        match variant {
            hir::Variant::Struct(s) => Some(Self::Struct(s)),
            hir::Variant::EnumVariant(e) => Some(Self::EnumVariant(e)),
            _ => None,
        }
    }

    pub fn from_module_def(resolution: hir::ModuleDef) -> Option<Self> {
        match resolution {
            hir::ModuleDef::EnumVariant(variant) => Some(variant.into()),
            hir::ModuleDef::Adt(hir::Adt::Struct(variant)) => Some(variant.into()),
            _ => None,
        }
    }

    pub fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
        match self {
            Self::Struct(it) => it.fields(db),
            Self::EnumVariant(it) => it.fields(db),
        }
    }

    #[allow(dead_code)]
    pub fn module(self, db: &dyn HirDatabase) -> Module {
        match self {
            Self::Struct(it) => it.module(db),
            Self::EnumVariant(it) => it.module(db),
        }
    }

    #[allow(dead_code)]
    pub fn name(&self, db: &dyn HirDatabase) -> Name {
        match self {
            Self::Struct(s) => (*s).name(db),
            Self::EnumVariant(e) => (*e).name(db),
        }
    }

    pub fn adt(&self, db: &dyn HirDatabase) -> Adt {
        match *self {
            Self::Struct(it) => it.into(),
            Self::EnumVariant(it) => it.parent_enum(db).into(),
        }
    }

    pub fn kind(&self, db: &dyn HirDatabase) -> hir::StructKind {
        match *self {
            Self::Struct(it) => it.kind(db),
            Self::EnumVariant(it) => it.kind(db),
        }
    }
}

impl HasCrate for ConstructableDef {
    fn krate(&self, db: &dyn HirDatabase) -> Crate {
        self.module(db).krate(db)
    }
}

pub struct Constructable<'db> {
    pub def: ConstructableDef,
    pub args: Vec<Type<'db>>,
}

impl<'db> Constructable<'db> {
    pub fn new(def: ConstructableDef, args: Vec<Type<'db>>) -> Self {
        Self { def, args }
    }

    pub fn fields(&self, db: &dyn HirDatabase) -> Vec<hir::Field> {
        self.def.fields(db)
    }
}
