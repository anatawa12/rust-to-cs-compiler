use hir_def::TypeAliasId;
use hir_def::lang_item::lang_items;
use hir_def::resolver::Resolver;
use hir_ty::ParamEnvAndCrate;
use hir_ty::db::HirDatabase;
use hir_ty::next_solver::{
    Clause, ClauseKind, DbInterner, GenericArgs, ParamEnv, SolverDefId, TermKind, TraitRef, Ty,
    TyKind,
};
use itertools::Itertools;
use rustc_type_ir::inherent::{GenericArg, IntoKind};
use rustc_type_ir::{AliasTyKind, Interner, PredicatePolarity};

pub(crate) trait TyFromType<'db> {
    fn ns_ty(&self) -> Ty<'db>;
    fn env(&self) -> ParamEnvAndCrate<'db>;
    fn from_ty_env(ty: Ty<'db>, env: ParamEnvAndCrate<'db>) -> Self;
    #[allow(dead_code)]
    fn from_ty_resolver(ty: Ty<'db>, db: &'db dyn HirDatabase, resolver: &Resolver<'_>) -> Self;
    fn derived(&self, ty: Ty<'db>) -> Self;
}

#[allow(dead_code)]
struct TypeRepr<'db> {
    env: ParamEnvAndCrate<'db>,
    ty: Ty<'db>,
}

impl<'db> TyFromType<'db> for hir::Type<'db> {
    fn ns_ty(&self) -> Ty<'db> {
        // SAFETY:  This is NOT safe in rust guaranteed behavior,
        //          but known implementation allows us to do so.
        unsafe { std::mem::transmute::<&Self, &TypeRepr<'db>>(self).ty }
    }

    fn env(&self) -> ParamEnvAndCrate<'db> {
        unsafe { std::mem::transmute::<&Self, &TypeRepr<'db>>(self).env }
    }

    fn from_ty_env(ty: Ty<'db>, env: ParamEnvAndCrate<'db>) -> Self {
        unsafe { std::mem::transmute::<TypeRepr<'db>, Self>(TypeRepr { env, ty }) }
    }

    fn from_ty_resolver(ty: Ty<'db>, db: &'db dyn HirDatabase, resolver: &Resolver<'_>) -> Self {
        Self::from_ty_env(ty, param_env_from_resolver(db, resolver))
    }

    fn derived(&self, ty: Ty<'db>) -> Self {
        Self::from_ty_env(ty, self.env())
    }
}

pub(crate) fn param_env_from_resolver<'db>(
    db: &'db dyn HirDatabase,
    resolver: &Resolver<'_>,
) -> ParamEnvAndCrate<'db> {
    ParamEnvAndCrate {
        param_env: resolver
            .generic_def()
            .map_or_else(ParamEnv::empty, |generic_def| {
                db.trait_environment(generic_def.into())
            }),
        krate: resolver.krate(),
    }
}

pub(crate) fn empty_param_env<'db>(krate: base_db::Crate) -> ParamEnvAndCrate<'db> {
    ParamEnvAndCrate {
        param_env: ParamEnv::empty(),
        krate,
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
            && let SolverDefId::TypeAliasId(alias_id) = def_id
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
    bounds: impl IntoIterator<Item = Clause<'db>>,
    target_type: &hir::Type<'db>,
    resolver: &Resolver,
    db: &'db dyn HirDatabase,
) -> ParsedProjection<'db> {
    let target_ty = target_type.ns_ty();
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
            def_id: SolverDefId::TypeAliasId(alias_id),
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

        Some((alias.self_ty(), SolverDefId::TypeAliasId(alias_id)))
    };

    let lang_items = lang_items(db, resolver.krate());

    #[derive(Debug)]
    enum Pred<'db> {
        Ty(Ty<'db>),
        Trait(TraitRef<'db>),
    }
    let clauses = (bounds.into_iter().chain(def_clauses.into_iter().flatten())).collect::<Vec<_>>();
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
