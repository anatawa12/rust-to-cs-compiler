use crate::codegen::CodeGenerator;
use crate::codegen::ty::generic_params::CsTypeParamSource;
use hir::HasContainer;
use hir::db::HirDatabase;
use ra_internal::TypeExt;

type TraitRef<'db> = (hir::Trait, Vec<hir::Type<'db>>);

#[allow(unused)]
pub enum ResolvedFunction<'db> {
    Static {
        self_ty: hir::Type<'db>,
        trait_: Option<TraitRef<'db>>,
        function_name: String,
        generic_sources: Vec<CsTypeParamSource>,
        generic_args: Vec<hir::Type<'db>>,
        args_map: Option<Vec<usize>>,
    },
    Method {
        self_ty: hir::Type<'db>,
        trait_: Option<TraitRef<'db>>,
        function_name: String,
        generic_sources: Vec<CsTypeParamSource>,
        generic_args: Vec<hir::Type<'db>>,
        args_map: Option<(usize, Vec<usize>)>,
    },
    ModuleFunction {
        function_path: String,
        generic_sources: Vec<CsTypeParamSource>,
        generic_args: Vec<hir::Type<'db>>,
        args_map: Option<Vec<usize>>,
    },
    OmitCall {
        comment: String,
    },
}

impl<'db> CodeGenerator<'db> {
    /// Resolves the function call to the actual function type
    ///
    /// This may return a different function than the original function call.
    pub fn resolve_function(
        &self,
        f: hir::Function,
        args: Vec<(hir::Symbol, hir::Type<'db>)>,
    ) -> ResolvedFunction<'db> {
        let (parent_args, f_args) = self.extract_generic_args(f, args);
        let db = self.db;

        // T::into<U> => replace with U::from()
        if let Some(resolved_info) = resolve_into(f, parent_args.as_deref(), &f_args, self.db) {
            return if Some(&resolved_info.target_ty) == resolved_info.self_ty.as_ref()
                || self.rust_type_to_cs(&resolved_info.target_ty)
                    == self.rust_type_to_cs(resolved_info.self_ty.as_ref().unwrap())
            {
                ResolvedFunction::OmitCall {
                    comment: "/*omit Into::into method*/".into(),
                }
            } else {
                ResolvedFunction::Static {
                    self_ty: resolved_info.target_ty,
                    trait_: None,
                    function_name: "m_From/*convert from Into::into method*/".into(),
                    generic_sources: Vec::new(),
                    generic_args: Vec::new(),
                    args_map: None,
                }
            };
        }

        if f.name(self.db).as_str() == "encode_utf8"
            && let hir::ItemContainer::Impl(impl_) = f.container(self.db)
            && let self_ty = impl_.self_ty(self.db)
            && let Some(type_) = self_ty.as_builtin()
            && type_.is_char()
        {
            return ResolvedFunction::Method {
                self_ty,
                trait_: None,
                function_name: "ToString/*converted from encode_utf8*/".into(),
                generic_sources: vec![],
                generic_args: vec![],
                args_map: Some((0, vec![])),
            };
        }

        // generic way
        match f.container(db) {
            hir::ItemContainer::Impl(impl_) => {
                let self_ty =
                    (impl_.self_ty(db)).instantiate(impl_.into(), &parent_args.unwrap(), db);

                if f.self_param(db).is_some() {
                    ResolvedFunction::Method {
                        self_ty,
                        trait_: None,
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into()),
                        generic_args: f_args,
                        args_map: None,
                    }
                } else {
                    ResolvedFunction::Static {
                        self_ty,
                        trait_: None,
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into()),
                        generic_args: f_args,
                        args_map: None,
                    }
                }
            }
            hir::ItemContainer::Trait(trait_) => {
                if f.self_param(db).is_some() {
                    ResolvedFunction::Method {
                        self_ty: parent_args.as_ref().unwrap()[0].clone(),
                        trait_: Some((trait_, parent_args.unwrap())),
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into()),
                        generic_args: f_args,
                        args_map: None,
                    }
                } else {
                    ResolvedFunction::Static {
                        self_ty: parent_args.as_ref().unwrap()[0].clone(),
                        trait_: Some((trait_, parent_args.unwrap())),
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into()),
                        generic_args: f_args,
                        args_map: None,
                    }
                }
            }
            hir::ItemContainer::Module(module) => {
                let mut path = self.module_class_cs(module);
                path.push('.');
                path.push_str(&self.function_name(f));
                ResolvedFunction::ModuleFunction {
                    function_path: path,
                    generic_sources: self.generic_params_cs_sources(f.into()),
                    generic_args: f_args,
                    args_map: None,
                }
            }
            unsupported => {
                panic!("Unsupported function container type: {:?}", unsupported);
            }
        }
    }
}

struct ResolvedInto<'db> {
    pub self_ty: Option<hir::Type<'db>>,
    pub target_ty: hir::Type<'db>,
}

fn resolve_into<'db>(
    f: hir::Function,
    parent_args: Option<&[hir::Type<'db>]>,
    f_args: &[hir::Type<'db>],
    db: &'db dyn HirDatabase,
) -> Option<ResolvedInto<'db>> {
    if f.name(db).symbol() == &hir::sym::into
        && let hir::ItemContainer::Trait(trait_) = f.container(db)
        && trait_.name(db).symbol() == &hir::sym::Into
    {
        assert_eq!(
            parent_args.as_ref().unwrap().len(),
            2,
            "Into trait type params (self)"
        );
        assert_eq!(f_args.len(), 0, "into function type params");
        Some(ResolvedInto {
            self_ty: Some(parent_args.unwrap()[0].clone()),
            target_ty: parent_args.unwrap()[1].clone(),
        })
    } else if f.name(db).symbol() == &hir::sym::into
        && let hir::ItemContainer::Impl(impl_) = f.container(db)
        && let Some(trait_ref) = impl_.trait_ref(db)
        && trait_ref.trait_().name(db).symbol() == &hir::sym::Into
    {
        assert_eq!(
            parent_args.as_ref().unwrap().len(),
            2,
            "Into trait type params (self)"
        );
        assert_eq!(f_args.len(), 0, "into function type params");
        Some(ResolvedInto {
            self_ty: Some(parent_args.unwrap()[0].clone()),
            target_ty: parent_args.unwrap()[1].clone(),
        })
    } else {
        None
    }
}
