use hir::HasCrate;
use hir_ty::db::HirDatabase;
use hir_ty::display::HirDisplay;
use std::fmt::Display;

pub trait DebugDisplay<'db> {
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + 'a
    where
        'db: 'a;
}

trait SharedImplDebugDisplay {}

impl SharedImplDebugDisplay for hir::Type<'_> {}
impl SharedImplDebugDisplay for hir::Function {}
impl SharedImplDebugDisplay for hir::Trait {}
impl SharedImplDebugDisplay for hir::TypeAlias {}

impl<'db, T> DebugDisplay<'db> for T
where
    T: HasCrate + HirDisplay<'db> + SharedImplDebugDisplay,
{
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + 'a
    where
        'db: 'a,
    {
        self.display_test(db, self.krate(db).to_display_target(db))
    }
}

impl<'db> DebugDisplay<'db> for hir::TypeParam {
    fn debug_display<'a>(&'a self, db: &'db dyn HirDatabase) -> impl Display + 'a
    where
        'db: 'a,
    {
        self.display_test(db, self.module(db).krate(db).to_display_target(db))
    }
}
