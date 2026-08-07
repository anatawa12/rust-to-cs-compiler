use crate::codegen::CodeGenerator;
use crate::codegen::output::Code;
use crate::codegen::simple_extensions::*;
use crate::codegen::ty::CsTypeOption;
use crate::codegen::ty::generic_params::CsTypeParamSource;
use hir::db::HirDatabase;
use hir::{HasContainer, HasCrate, sym};
use ra_internal::LangItems;

type TraitRef<'db> = (hir::Trait, Vec<hir::Type<'db>>);

#[allow(unused)]
pub enum ResolvedFunction<'db> {
    Static {
        self_ty: hir::Type<'db>,
        trait_: Option<TraitRef<'db>>,
        function_name: String,
        generic_sources: Vec<CsTypeParamSource<'db>>,
        generic_args: Vec<hir::Type<'db>>,
        args_map: Option<Vec<ArgSource>>,
    },
    Method {
        self_ty: hir::Type<'db>,
        trait_: Option<TraitRef<'db>>,
        function_name: String,
        generic_sources: Vec<CsTypeParamSource<'db>>,
        generic_args: Vec<hir::Type<'db>>,
        args_map: Option<(ArgSource, Vec<ArgSource>)>,
    },
    ModuleFunction {
        function_path: String,
        generic_sources: Vec<CsTypeParamSource<'db>>,
        generic_args: Vec<hir::Type<'db>>,
        args_map: Option<Vec<ArgSource>>,
    },
    OmitCall {
        comment: String,
    },
}

pub enum ArgSource {
    Source(usize),
    CustomExpr(Code),
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
        let lang_items = LangItems::new(db, f.krate(db));

        // T::into<U> => replace with U::from()
        if *f.name(db).symbol() == sym::into
            && let Some(into_trait) = lang_items.Into()
            && let Some(trait_params) =
                resolve_trait_impl(f, into_trait, parent_args.as_deref(), db)
        {
            let [self_ty, target_ty] = trait_params.as_slice() else {
                panic!("Into trait type params (self)");
            };

            return if target_ty == self_ty
                || self.rust_type_to_cs(target_ty) == self.rust_type_to_cs(self_ty)
            {
                ResolvedFunction::OmitCall {
                    comment: "/*omit Into::into method*/".into(),
                }
            } else {
                ResolvedFunction::Static {
                    self_ty: target_ty.clone(),
                    trait_: None,
                    function_name: "m_From/*convert from Into::into method*/".into(),
                    generic_sources: Vec::new(),
                    generic_args: Vec::new(),
                    args_map: None,
                }
            };
        }

        // str::parse<U> => replace with U::from_str()
        if *f.name(db).symbol() == sym::parse
            && let hir::ItemContainer::Impl(impl_) = f.container(self.db)
            && let ref self_ty = impl_.self_ty(self.db)
            && let Some(type_) = self_ty.as_builtin()
            && type_.is_str()
        {
            let [target_ty] = f_args.as_slice() else {
                panic!("Into trait type params (self)");
            };

            return if target_ty == self_ty
                || self.rust_type_to_cs(target_ty) == self.rust_type_to_cs(self_ty)
            {
                ResolvedFunction::OmitCall {
                    comment: "/*omit str::parse method*/".into(),
                }
            } else {
                ResolvedFunction::Static {
                    self_ty: target_ty.clone(),
                    trait_: None,
                    function_name: "m_FromStr/*convert from str::parse method*/".into(),
                    generic_sources: Vec::new(),
                    generic_args: Vec::new(),
                    args_map: None,
                }
            };
        }

        // T::collect<B>() => B::from_iter
        if f.name(db).as_str() == "collect"
            && let Some(iterator_trait) = lang_items.Iterator()
            && let Some(_trait_params) =
                resolve_trait_impl(f, iterator_trait, parent_args.as_deref(), db)
        {
            return ResolvedFunction::Static {
                self_ty: f_args[0].clone(),
                trait_: None,
                function_name: "m_FromIter/*collect*/".into(),
                generic_sources: Vec::new(),
                generic_args: Vec::new(),
                args_map: None,
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
                args_map: Some((ArgSource::Source(0), vec![])),
            };
        }

        if f.name(self.db).as_str() == "new"
            && let hir::ItemContainer::Impl(impl_) = f.container(self.db)
            && let self_ty = impl_.self_ty(self.db)
            && let Some(adt) = self_ty.as_adt()
            && let hir::Adt::Struct(struct_) = adt
            && Some(struct_) == lang_items.OwnedBox()
        {
            return ResolvedFunction::OmitCall {
                comment: "/*omit Box::new method*/".into(),
            };
        }

        if f.name(self.db).as_str() == "as_ref"
            && let hir::ItemContainer::Impl(impl_) = f.container(self.db)
            && let self_ty = impl_.self_ty(self.db)
            && let Some(adt) = self_ty.as_adt()
            && let hir::Adt::Struct(struct_) = adt
            && Some(struct_) == lang_items.OwnedBox()
        {
            return ResolvedFunction::OmitCall {
                comment: "/*omit Box::as_ref method*/".into(),
            };
        }

        if f.name(self.db).as_str() == "new"
            && let hir::ItemContainer::Impl(impl_) = f.container(self.db)
            && let self_ty = impl_.self_ty(self.db)
            && let Some(adt) = self_ty.as_adt()
            && let hir::Adt::Struct(struct_) = adt
            && Some(struct_) == lang_items.OsStr()
        {
            return ResolvedFunction::OmitCall {
                comment: "/*omit OsStr::new method*/".into(),
            };
        }

        // Option<T>::unwrap_or_default() => Option<T>::unwrap_or_else(T::default)
        if f.name(self.db).as_str() == "unwrap_or_default"
            && let hir::ItemContainer::Impl(impl_) = f.container(self.db)
            && let self_ty = impl_.self_ty(self.db)
            && let Some(adt) = self_ty.as_adt()
            && let hir::Adt::Enum(enum_) = adt
            && (Some(enum_) == lang_items.Option() || Some(enum_) == lang_items.Result())
        {
            let Some([type_] | [type_, _]) = parent_args.as_deref() else {
                panic!("Into enum type params (self)");
            };
            return ResolvedFunction::Method {
                self_ty,
                trait_: None,
                function_name: "m_UnwrapOrElse".into(),
                generic_sources: vec![],
                generic_args: vec![],
                args_map: Some((
                    ArgSource::Source(0),
                    vec![ArgSource::CustomExpr(
                        format!(
                            "{}.m_Default",
                            self.rust_type_to_cs_options(type_, CsTypeOption::static_access())
                        )
                        .into(),
                    )],
                )),
            };
        }

        // generic way
        match f.container(db) {
            hir::ItemContainer::Impl(impl_) => {
                let self_ty = (impl_.self_ty(db)).instantiate(parent_args.as_ref().unwrap());

                if f.self_param(db).is_some() {
                    ResolvedFunction::Method {
                        self_ty,
                        trait_: None,
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into(), &f_args),
                        generic_args: f_args,
                        args_map: None,
                    }
                } else {
                    ResolvedFunction::Static {
                        self_ty,
                        trait_: None,
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into(), &f_args),
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
                        generic_sources: self.generic_params_cs_sources(f.into(), &f_args),
                        generic_args: f_args,
                        args_map: None,
                    }
                } else {
                    ResolvedFunction::Static {
                        self_ty: parent_args.as_ref().unwrap()[0].clone(),
                        trait_: Some((trait_, parent_args.unwrap())),
                        function_name: self.function_name(f),
                        generic_sources: self.generic_params_cs_sources(f.into(), &f_args),
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
                    generic_sources: self.generic_params_cs_sources(f.into(), &f_args),
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

fn resolve_trait_impl<'db>(
    f: hir::Function,
    trait_: hir::Trait,
    parent_args: Option<&[hir::Type<'db>]>,
    db: &'db dyn HirDatabase,
) -> Option<Vec<hir::Type<'db>>> {
    if f.container(db) == hir::ItemContainer::Trait(trait_) {
        Some(parent_args.unwrap().to_vec())
    } else if let hir::ItemContainer::Impl(impl_) = f.container(db)
        && let Some(trait_ref) = impl_.trait_ref(db)
        && trait_ref.trait_() == trait_
    {
        let parent_args = parent_args.unwrap();
        Some(
            (hir::GenericDef::Trait(trait_).params0(db).into_iter())
                .enumerate()
                .filter(|(_, x)| matches!(x, hir::GenericParam::TypeParam(_)))
                .flat_map(|(i, _)| trait_ref.get_type_argument(i))
                .map(move |x| x.instantiate(parent_args))
                .collect(),
        )
    } else {
        None
    }
}
