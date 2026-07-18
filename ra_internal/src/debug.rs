use hir::db::HirDatabase;
use hir::{HasCrate, HirDisplay};
use std::fmt::{Debug, Display, Formatter};

pub trait DebugDisplay<'db> {
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + Debug + 'a
    where
        'db: 'a;
}

trait SharedImplDebugDisplay {}

impl SharedImplDebugDisplay for hir::Type<'_> {}
impl SharedImplDebugDisplay for hir::Function {}
impl SharedImplDebugDisplay for hir::Trait {}
impl SharedImplDebugDisplay for hir::Impl {}
impl SharedImplDebugDisplay for hir::Adt {}
impl SharedImplDebugDisplay for hir::TypeAlias {}

impl<'db, T> DebugDisplay<'db> for T
where
    T: HasCrate + HirDisplay<'db> + SharedImplDebugDisplay,
{
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + Debug + 'a
    where
        'db: 'a,
    {
        DisplayAsDebug(self.display_test(db, self.krate(db).to_display_target(db)))
    }
}

impl<'db> DebugDisplay<'db> for hir::TypeParam {
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + Debug + 'a
    where
        'db: 'a,
    {
        DisplayAsDebug(self.display_test(db, self.module(db).krate(db).to_display_target(db)))
    }
}

impl<'db> DebugDisplay<'db> for hir::AssocItemContainer {
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + Debug + 'a
    where
        'db: 'a,
    {
        std::fmt::from_fn(|f| match self {
            hir::AssocItemContainer::Trait(t) => Display::fmt(&t.debug_display(db), f),
            hir::AssocItemContainer::Impl(_) => f.write_str("some impl /*TODO*/"),
        })
    }
}

struct DisplayAsDebug<T>(T);
impl<T: Display> Debug for DisplayAsDebug<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<T: Display> Display for DisplayAsDebug<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
