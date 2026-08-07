use hir_ty::db::HirDatabase;
use hir_ty::next_solver::Unnormalized;
use rustc_type_ir::inherent::IntoKind;

pub trait AssocItemExt {
    fn includes_self_sized_bounds(self, db: &dyn HirDatabase) -> bool;
}

impl AssocItemExt for hir::AssocItem {
    fn includes_self_sized_bounds(self, db: &dyn HirDatabase) -> bool {
        let krate = self.module(db).krate(db);
        let interner = hir_ty::next_solver::DbInterner::new_with(db, krate.into());
        let Some(sized) = interner.lang_items().Sized else {
            return false;
        };

        let predicates = hir_ty::GenericPredicates::query_own_explicit(
            db,
            match self {
                hir::AssocItem::Function(f) => hir::GenericDef::from(f).try_into().unwrap(),
                hir::AssocItem::Const(c) => hir::GenericDef::from(c).try_into().unwrap(),
                hir::AssocItem::TypeAlias(t) => hir::GenericDef::from(t).try_into().unwrap(),
            },
        );

        // copied from ra_ap_hir_ty-0.0.343/src/dyn_compatibility.rs
        // FIXME: We should use `explicit_predicates_of` here, which hasn't been implemented to
        // rust-analyzer yet
        // https://github.com/rust-lang/rust/blob/ddaf12390d3ffb7d5ba74491a48f3cd528e5d777/compiler/rustc_hir_analysis/src/collect/predicates_of.rs#L490
        rustc_type_ir::elaborate::elaborate(
            interner,
            predicates.iter_identity().map(Unnormalized::skip_norm_wip),
        )
        .any(|pred| match pred.kind().skip_binder() {
            rustc_type_ir::ClauseKind::Trait(trait_pred) => {
                if sized == trait_pred.def_id().0
                    && let rustc_type_ir::TyKind::Param(param_ty) =
                        trait_pred.trait_ref.self_ty().kind()
                    && param_ty.index == 0
                {
                    true
                } else {
                    false
                }
            }
            _ => false,
        })
    }
}
