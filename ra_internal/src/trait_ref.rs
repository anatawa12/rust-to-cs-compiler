use hir_ty::db::HirDatabase;

pub trait TraitRefExt<'db> {
    fn generic_types(
        &self,
        db: &'db dyn HirDatabase,
    ) -> impl Iterator<Item = Option<hir::Type<'db>>>;
}

impl<'db> TraitRefExt<'db> for hir::TraitRef<'db> {
    fn generic_types(
        &self,
        db: &'db dyn HirDatabase,
    ) -> impl Iterator<Item = Option<hir::Type<'db>>> {
        hir::GenericDef::Trait(self.trait_())
            .params(db)
            .into_iter()
            .enumerate()
            .map(|(i, _)| self.get_type_argument(i))
            .map(move |x| x.map(|x| x.to_type(db)))
    }
}
