use crate::LangItems;
use crate::internal::TyFromType;
use hir::{HasContainer, HasCrate};
use hir_def::hir::generics::LocalTypeOrConstParamId;
use hir_def::{FunctionId, TraitId, TypeOrConstParamId, TypeParamId};
use hir_ty::GenericPredicates;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{ClauseKind, DbInterner, Ty};
use rustc_type_ir::inherent::IntoKind;

pub trait FunctionExt: Copy {
    fn is_explicit_sized_self(self, db: &dyn HirDatabase) -> bool;
}

impl FunctionExt for hir::Function {
    fn is_explicit_sized_self(self, db: &dyn HirDatabase) -> bool {
        let Ok(function_id) = FunctionId::try_from(self) else {
            return false;
        };
        let self_ty = match self.container(db) {
            hir::ItemContainer::Impl(i) => i.self_ty(db).ns_ty(),
            hir::ItemContainer::Trait(t) => Ty::new_param(
                DbInterner::new_with(db, self.krate(db).base()),
                TypeParamId::from_unchecked(TypeOrConstParamId {
                    parent: TraitId::from(t).into(),
                    local_id: LocalTypeOrConstParamId::from_raw(0.into()),
                }),
                0,
            ),
            _ => return false,
        };
        let bounds = GenericPredicates::query_own_explicit(db, function_id.into())
            .iter_identity()
            .collect::<Vec<_>>();

        let lang_item = LangItems::new(db, self.krate(db));

        bounds
            .iter()
            .filter_map(|clause| variant_or_none!(clause.kind().skip_binder(), ClauseKind::Trait))
            .filter(|trait_pred| self_ty == trait_pred.trait_ref.self_ty())
            .any(|trait_pred| {
                Some(hir::Trait::from(trait_pred.trait_ref.def_id.0)) == lang_item.Sized()
            })
    }
}
