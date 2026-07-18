use hir::HasCrate;
use hir_def::AdtId;
use hir_ty::db::HirDatabase;

pub trait AdtExt: Copy {
    fn impls(self, db: &dyn HirDatabase) -> &[hir::Impl];
}

impl AdtExt for hir::Adt {
    fn impls(self, db: &dyn HirDatabase) -> &[hir::Impl] {
        #[salsa_macros::tracked(returns(ref))]
        fn impls(db: &dyn HirDatabase, adt: AdtId) -> Vec<hir::Impl> {
            let adt = hir::Adt::from(adt);
            hir::Impl::all_in_crate(db, adt.krate(db))
                .into_iter()
                .filter(|impl_| {
                    if let Some((impl_adt, _)) = impl_.self_ty(db).as_adt_with_args() {
                        impl_adt == adt
                    } else {
                        false
                    }
                })
                .collect()
        }

        impls(db, self.into())
    }
}
