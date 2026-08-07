use hir::{HasContainer, HasCrate};
use hir_def::lang_item::lang_items;
use hir_def::{AdtId, BuiltinDeriveImplId, GenericDefId, TypeAliasId};
use hir_ty::ParamEnvAndCrate;
use hir_ty::db::{AnonConstId, HirDatabase};
use hir_ty::next_solver::{
    AnyImplId, Clause, ClauseKind, DbInterner, EarlyBinder, GenericArgs, ParamEnv, TermId,
    TermKind, TraitAssocTermId, TraitAssocTyId, TraitRef, Ty, TyKind, Unnormalized,
};
use itertools::Itertools;
use rustc_type_ir::inherent::{GenericArg, IntoKind};
use rustc_type_ir::{AliasTyKind, Interner, PredicatePolarity};

pub(crate) trait TyFromType<'db> {
    fn ns_ty(&self) -> EarlyBinder<'db, Ty<'db>>;
    fn owner_id(&self) -> TypeOwnerId<'db>;
    fn from_ty_owner(ty: EarlyBinder<'db, Ty<'db>>, owner: impl Into<TypeOwnerId<'db>>) -> Self;

    fn env(&self, db: &'db dyn HirDatabase) -> ParamEnvAndCrate<'db>;
    fn derived(&self, ty: Ty<'db>) -> Self;
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub enum TypeOwnerId<'db> {
    GenericDefId(GenericDefId),
    BuiltinDeriveImplId(BuiltinDeriveImplId),
    AnonConstId(AnonConstId<'db>),
    // FIXME: What do when we unify two different crates? Currently we just randomly keep one.
    NoParams(base_db::Crate),
}

#[allow(dead_code)]
struct TypeRepr<'db> {
    owner: TypeOwnerId<'db>,
    ty: EarlyBinder<'db, Ty<'db>>,
}

impl<'db> TyFromType<'db> for hir::Type<'db> {
    fn ns_ty(&self) -> EarlyBinder<'db, Ty<'db>> {
        // SAFETY:  This is NOT safe in rust guaranteed behavior,
        //          but known implementation allows us to do so.
        unsafe { std::mem::transmute::<&Self, &TypeRepr<'db>>(self).ty }
    }

    fn owner_id(&self) -> TypeOwnerId<'db> {
        unsafe { std::mem::transmute::<&Self, &TypeRepr<'db>>(self).owner }
    }

    fn from_ty_owner(ty: EarlyBinder<'db, Ty<'db>>, owner: impl Into<TypeOwnerId<'db>>) -> Self {
        unsafe {
            std::mem::transmute::<TypeRepr<'db>, Self>(TypeRepr {
                ty,
                owner: owner.into(),
            })
        }
    }

    fn env(&self, db: &'db dyn HirDatabase) -> ParamEnvAndCrate<'db> {
        let interner = DbInterner::new_no_crate(db);
        let krate = self.krate(db).into();
        match self.owner_id() {
            TypeOwnerId::GenericDefId(def) => ParamEnvAndCrate {
                param_env: db.trait_environment(def),
                krate,
            },
            TypeOwnerId::BuiltinDeriveImplId(def) => ParamEnvAndCrate {
                param_env: hir_ty::builtin_derive::param_env(interner, def),
                krate,
            },
            TypeOwnerId::AnonConstId(def) => ParamEnvAndCrate {
                param_env: db.trait_environment(def.loc(db).owner.generic_def(db)),
                krate,
            },
            TypeOwnerId::NoParams(_) => ParamEnvAndCrate {
                param_env: ParamEnv::empty(interner),
                krate,
            },
        }
    }

    fn derived(&self, ty: Ty<'db>) -> Self {
        Self::from_ty_owner(EarlyBinder::bind(ty), self.owner_id())
    }
}

impl<'db> From<hir::Crate> for TypeOwnerId<'db> {
    fn from(value: hir::Crate) -> Self {
        TypeOwnerId::NoParams(value.base())
    }
}
impl<'db> From<hir::GenericDef> for TypeOwnerId<'db> {
    fn from(value: hir::GenericDef) -> Self {
        match value {
            hir::GenericDef::Adt(hir::Adt::Struct(hir)) => {
                TypeOwnerId::GenericDefId(GenericDefId::AdtId(AdtId::StructId(hir.into())))
            }
            hir::GenericDef::Adt(hir::Adt::Enum(hir)) => {
                TypeOwnerId::GenericDefId(GenericDefId::AdtId(AdtId::EnumId(hir.into())))
            }
            hir::GenericDef::Adt(hir::Adt::Union(hir)) => {
                TypeOwnerId::GenericDefId(GenericDefId::AdtId(AdtId::UnionId(hir.into())))
            }
            hir::GenericDef::Const(hir) => {
                TypeOwnerId::GenericDefId(GenericDefId::ConstId(hir.into()))
            }
            hir::GenericDef::Function(hir) if let Ok(id) = hir.try_into() => {
                TypeOwnerId::GenericDefId(GenericDefId::FunctionId(id))
            }
            hir::GenericDef::Function(hir) => {
                // hir.try_into::<FunctionId>() = None => BuiltinDeriveImplMethod.
                // We use container (impl)'s id
                let hir::ItemContainer::Impl(impl_) =
                    hir_ty::with_attached_db(|db| hir.container(db))
                else {
                    panic!("Expected impl container for function {hir:?}");
                };
                let AnyImplId::BuiltinDeriveImplId(derive_id) = AnyImplId::from(impl_) else {
                    panic!("Expected derive impl container for function {hir:?}");
                };
                TypeOwnerId::BuiltinDeriveImplId(derive_id)
            }
            hir::GenericDef::Impl(hir) => match AnyImplId::from(hir) {
                AnyImplId::ImplId(i) => TypeOwnerId::GenericDefId(GenericDefId::ImplId(i)),
                AnyImplId::BuiltinDeriveImplId(i) => TypeOwnerId::BuiltinDeriveImplId(i),
            },
            hir::GenericDef::Static(hir) => {
                TypeOwnerId::GenericDefId(GenericDefId::StaticId(hir.into()))
            }
            hir::GenericDef::Trait(hir) => {
                TypeOwnerId::GenericDefId(GenericDefId::TraitId(hir.into()))
            }
            hir::GenericDef::TypeAlias(hir) => {
                TypeOwnerId::GenericDefId(GenericDefId::TypeAliasId(hir.into()))
            }
        }
    }
}

pub(crate) trait TyExt<'db> {
    /// Returns Some if the type is `<SomeT as SomeTrait>::AssociatedType`
    fn as_associated_type(&self) -> Option<(Ty<'db>, GenericArgs<'db>, TypeAliasId)>;
}

impl<'db> TyExt<'db> for Ty<'db> {
    fn as_associated_type(&self) -> Option<(Ty<'db>, GenericArgs<'db>, TypeAliasId)> {
        if let TyKind::Alias(alias) = self.kind()
            && let AliasTyKind::Projection { def_id } = alias.kind
            && let TraitAssocTyId(alias_id) = def_id
        {
            Some((alias.self_ty(), alias.args, alias_id))
        } else {
            None
        }
    }
}

pub(crate) enum ParsedProjection<'db> {
    NoBounds,
    Projection(hir::Type<'db>),
    Traits(Vec<(hir::Trait, Vec<hir::Type<'db>>)>),
}

/// Resolves type Clauses for the `target_ty` among the given `bounds`.
///
/// If there is a ` target_ty = some_ty ` clause, it returns [`ParsedProjection::Projection`].
/// Otherwise, if there are some `<target_ty>: Trait<args>` clauses, returns [`ParsedProjection::Traits`].
/// Otherwise, returns [`ParsedProjection::NoBounds`].
pub(crate) fn parse_bounds_for<'db>(
    bounds: impl IntoIterator<Item = Unnormalized<'db, Clause<'db>>>,
    target_type: &hir::Type<'db>,
    db: &'db dyn HirDatabase,
) -> ParsedProjection<'db> {
    let target_ty = target_type.ns_ty().skip_binder();
    let mut def_clauses = vec![];

    // collect clauses of associated type declaration at trait definition
    {
        let mut target_ty = target_ty;
        while let Some((self_ty, args, alias_id)) = target_ty.as_associated_type() {
            let interner = DbInterner::new_with(
                db,
                hir::TypeAlias::from(alias_id).module(db).krate(db).into(),
            );
            let x = interner
                .item_self_bounds(alias_id.into())
                .iter_instantiated(interner, args)
                .collect::<Vec<_>>();

            def_clauses.push(x);

            target_ty = self_ty;
        }
    }

    // gets the target_ty as projection form to be used to find projecton clause
    let projection = 'resolve_projection: {
        let TyKind::Alias(alias) = target_ty.kind() else {
            // likely to be already resolved
            break 'resolve_projection None;
        };
        let AliasTyKind::Projection {
            def_id: TraitAssocTyId(alias_id),
        } = alias.kind
        else {
            panic!("Tries to assoc but not assoc: {target_ty:?}")
        };

        /* // same code as above // TODO: remove
        let args = alias.args;
        let interner = DbInterner::new_with(
            db,
            hir::TypeAlias::from(alias_id).module(db).krate(db).into(),
        );
        let x = interner
            .item_self_bounds(alias_id.into())
            .iter_instantiated(interner, args)
            .collect::<Vec<_>>();

        clauses.push(x);
        */

        Some((
            alias.self_ty(),
            TraitAssocTermId(TermId::TypeAliasId(alias_id)),
        ))
    };

    let clauses = (bounds.into_iter().chain(def_clauses.into_iter().flatten())).collect::<Vec<_>>();
    parse_clauses(clauses, target_type, projection, db)
}

pub(crate) fn parse_clauses<'db>(
    clauses: impl IntoIterator<Item = Unnormalized<'db, Clause<'db>>>,
    target_type: &hir::Type<'db>,
    projection: Option<(Ty<'db>, TraitAssocTermId)>,
    db: &'db dyn HirDatabase,
) -> ParsedProjection<'db> {
    let target_ty = target_type.ns_ty().skip_binder();

    let lang_items = lang_items(db, target_type.krate(db).base());

    #[derive(Debug)]
    enum Pred<'db> {
        Ty(Ty<'db>),
        Trait(TraitRef<'db>),
    }

    let preds = (clauses.into_iter())
        .filter_map(|clause| match clause.kind().skip_binder() {
            ClauseKind::Projection(proj)
            if Some((proj.projection_term.self_ty(), proj.def_id())) == projection =>
                Some(Pred::Ty(variant_or_none!(proj.term.kind(), TermKind::Ty)
                    .expect("Associated type is not type"))),
            ClauseKind::Trait(trait_)
            if trait_.self_ty() == target_ty
                && trait_.polarity == PredicatePolarity::Positive =>
                Some(Pred::Trait(trait_.trait_ref)),
            _ => None,
        })
        .filter(|x| !matches!(x, Pred::Trait(trait_ref) if Some(trait_ref.def_id.into()) == lang_items.Sized))
        .collect::<Vec<_>>();

    if preds.is_empty() {
        return ParsedProjection::NoBounds;
    }

    // If there is <Assoc = SomeType> part, we pick the type
    if let Some(&pred) = preds.iter().find_map(|x| variant_or_none!(x, Pred::Ty)) {
        return ParsedProjection::Projection(target_type.derived(pred));
    }

    ParsedProjection::Traits(
        preds
            .into_iter()
            .map(|x| match x {
                Pred::Trait(trait_ref) => {
                    let types = trait_ref
                        .args
                        .as_slice()
                        .iter()
                        .flat_map(|arg| Some(target_type.derived(arg.as_type()?)))
                        .collect();
                    (hir::Trait::from(trait_ref.def_id.0), types)
                }
                _ => unreachable!(),
            })
            //.filter(|(t, _)| Some((*t).into()) != lang_items.Sized)
            .unique()
            .collect(),
    )
}
