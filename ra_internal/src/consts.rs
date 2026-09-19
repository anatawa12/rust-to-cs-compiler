use crate::internal::TyFromType;
use hir::HasCrate;
use hir_def::layout::TargetDataLayout;
use hir_def::{AdtId, ConstId, FieldId};
use hir_ty::db::HirDatabase;
use hir_ty::layout::{Layout, TagEncoding};
use hir_ty::mir::{IsSigned, VTableMap, pad16};
use hir_ty::next_solver::infer::DbInternerInferExt;
use hir_ty::next_solver::infer::traits::ObligationCause;
use hir_ty::next_solver::{
    Allocation, DbInterner, GenericArgs, ParamEnv, Ty, TyKind, TypingMode, Tys,
};
use hir_ty::primitive::FloatTy;
use hir_ty::{MemoryMap, ParamEnvAndCrate, consteval};
use rustc_type_ir::EarlyBinder;
use rustc_type_ir::inherent::IntoKind;
use std::range::Range;

pub trait ConstExt: Copy {
    fn eval_value<'db>(
        self,
        db: &'db dyn HirDatabase,
    ) -> Result<ConstValue<'db>, hir::ConstEvalError<'db>>;
}

impl ConstExt for hir::Const {
    fn eval_value<'db>(
        self,
        db: &'db dyn HirDatabase,
    ) -> Result<ConstValue<'db>, hir::ConstEvalError<'db>> {
        let interner = DbInterner::new_no_crate(db);
        let id = ConstId::from(self);
        let ty = db
            .value_ty(id.into())
            .unwrap()
            .instantiate_identity()
            .skip_norm_wip();
        db.const_eval(id, GenericArgs::empty(interner), None)
            .map(|it| ConstValue {
                db,
                source: self,
                range: (0..it.memory.len()).into(),
                alloc: it,
                addr: None,
                ty,
            })
    }
}

#[derive(Copy, Clone)]
pub struct ConstValue<'db> {
    db: &'db dyn HirDatabase,
    source: hir::Const,
    // fields (data) from root Allocation
    alloc: Allocation<'db>,
    addr: Option<usize>,

    // Per-value
    ty: Ty<'db>,
    range: Range<usize>,
}

impl<'db> ConstValue<'db> {
    fn interner(&self) -> DbInterner<'db> {
        let db = self.db;
        DbInterner::new_with(db, self.source.module(db).krate(db).base())
    }

    fn param_env(&self, interner: DbInterner<'db>) -> ParamEnvAndCrate<'db> {
        let db = self.db;
        let param_env = ParamEnv::empty(interner);
        let krate = self.source.krate(db).base();
        ParamEnvAndCrate { param_env, krate }
    }

    fn new_sub_addr(
        &self,
        addr: Option<usize>,
        range: Range<usize>,
        ty: Ty<'db>,
    ) -> ConstValue<'db> {
        let Some(_) = self.get_memory(addr, range) else {
            panic!("Invalid address or range");
        };

        ConstValue {
            db: self.db,
            source: self.source,
            alloc: self.alloc,

            addr,
            ty,
            range,
        }
    }
    fn new_sub(&self, range: Range<usize>, ty: Ty<'db>) -> ConstValue<'db> {
        assert!(self.range.start <= range.start && range.start <= self.range.end);
        assert!(self.range.start <= range.end && range.end <= self.range.end);
        self.new_sub_addr(self.addr, range, ty)
    }

    fn get_memory(&self, addr: Option<usize>, range: Range<usize>) -> Option<&[u8]> {
        if let Some(addr) = addr {
            if range.is_empty() {
                Some(&[])
            } else {
                match &self.alloc.memory_map {
                    MemoryMap::Empty => Some(&[]),
                    MemoryMap::Simple(m) if addr == 0 => m.get(range),
                    MemoryMap::Simple(_) => None,
                    MemoryMap::Complex(cm) => {
                        #[allow(unused)]
                        pub struct ComplexMemoryMap<'db> {
                            memory: rustc_type_ir::data_structures::IndexMap<usize, Box<[u8]>>,
                            vtable: VTableMap<'db>,
                        }
                        // SAFETY: not safe
                        let cm = unsafe {
                            std::mem::transmute::<&hir_ty::ComplexMemoryMap, &ComplexMemoryMap>(&cm)
                        };
                        cm.memory.get(&addr)?.get(range)
                    }
                }
            }
        } else {
            self.alloc.memory.get(range)
        }
    }

    pub fn kind(&self) -> ConstValueKind<'db> {
        let db = self.db;
        let ty = self.ty;
        let interner = self.interner();
        let param_env = self.param_env(interner);
        let infcx = interner.infer_ctxt().build(TypingMode::PostAnalysis);
        let ty = infcx
            .at(&ObligationCause::dummy(), param_env.param_env)
            .deeply_normalize(ty)
            .unwrap_or(ty);
        let krate = self.source.krate(db).base();
        let Some(b) = self.get_memory(self.addr, self.range) else {
            return ConstValueKind::Error("bad address");
        };

        match ty.kind() {
            TyKind::Bool => ConstValueKind::Bool(b[0] != 0),
            TyKind::Char => {
                let it = as_u128_pad_le(b) as u32;
                let Ok(c) = char::try_from(it) else {
                    return ConstValueKind::Error("<unicode-error>");
                };
                ConstValueKind::Char(c)
            }
            TyKind::Int(_) => ConstValueKind::Int(as_i128_pad_le(b)),
            TyKind::Uint(_) => ConstValueKind::Uint(as_u128_pad_le(b)),
            TyKind::Float(fl) => match fl {
                FloatTy::F16 => ConstValueKind::Error("f16"),
                FloatTy::F32 => ConstValueKind::F32(f32::from_le_bytes(b.try_into().unwrap())),
                FloatTy::F64 => ConstValueKind::F64(f64::from_le_bytes(b.try_into().unwrap())),
                FloatTy::F128 => ConstValueKind::Error("f128"),
            },
            TyKind::Adt(def, args) => {
                let def = def.def_id();
                let Ok(layout) = db.layout_of_adt(def, args.store(), param_env.store()) else {
                    return ConstValueKind::Error("layout error");
                };
                match def {
                    AdtId::StructId(s) => {
                        let s = hir::Struct::from(s);
                        //s.fields(db)[0].ty()
                        ConstValueKind::Struct(ConstVariantValue {
                            def: s,
                            args,
                            layout: Layout::clone(&layout),
                            upstream: self.clone(),
                        })
                    }
                    AdtId::EnumId(e) => {
                        let e = hir::Enum::from(e);
                        let Ok(target_data_layout) = db.target_data_layout(krate) else {
                            return ConstValueKind::Error("target layout not available");
                        };
                        let Some((variant, var_layout)) =
                            detect_variant_from_bytes(&layout, db, target_data_layout, b, e)
                        else {
                            return ConstValueKind::Error("failed to resolve enum variant");
                        };
                        ConstValueKind::Enum(ConstVariantValue {
                            def: variant,
                            args,
                            layout: Layout::clone(var_layout),
                            upstream: self.clone(),
                        })
                    }
                    AdtId::UnionId(_) => ConstValueKind::Error("union"),
                }
            }
            TyKind::Foreign(_) => unreachable!("Foreign types are not available for const eval"),
            TyKind::Str => unreachable!("unsized value: str"),
            TyKind::Array(ty, len) => {
                let Some(len) = consteval::try_const_usize(db, len) else {
                    return ConstValueKind::Error("<unknown-array-len>");
                };
                let Ok(_layout) = db.layout_of_ty(ty.store(), param_env.store()) else {
                    return ConstValueKind::Error("<layout-error>");
                };
                ConstValueKind::Array(ConstArrayValue {
                    element: ty,
                    len: len as usize,
                    upstream: *self,
                })
            }
            TyKind::Pat(_, _) => ConstValueKind::Error("pattern type"),
            TyKind::Slice(_) => unreachable!("unsized value: slice"),
            TyKind::RawPtr(_, _) => ConstValueKind::RawPtr(as_u128_pad_le(b)),
            TyKind::Ref(_, t, _) => match t.kind() {
                TyKind::Str => {
                    let addr = usize::from_le_bytes(b[0..b.len() / 2].try_into().unwrap());
                    let size = usize::from_le_bytes(b[b.len() / 2..].try_into().unwrap());
                    let Some(bytes) = self.get_memory(Some(addr), (0..size).into()) else {
                        return ConstValueKind::Error("<ref-data-not-available>");
                    };

                    let s = std::str::from_utf8(bytes).unwrap_or("<utf8-error>");
                    ConstValueKind::Ref(ConstRefValue::Str(s.to_string()))
                }
                TyKind::Slice(ty) => {
                    let addr = usize::from_le_bytes(b[0..b.len() / 2].try_into().unwrap());
                    let count = usize::from_le_bytes(b[b.len() / 2..].try_into().unwrap());
                    let Ok(layout) = db.layout_of_ty(ty.store(), param_env.store()) else {
                        return ConstValueKind::Error("<layout-error>");
                    };
                    let size_one = layout.size.bytes_usize();
                    let Some(bytes) = self.get_memory(Some(addr), (0..size_one * count).into())
                    else {
                        return ConstValueKind::Error("<ref-data-not-available>");
                    };
                    let expected_len = count * size_one;
                    if bytes.len() < expected_len {
                        panic!(
                            "Memory map size is too small. Expected {expected_len}, got {}",
                            bytes.len(),
                        );
                    }

                    ConstValueKind::Ref(ConstRefValue::Slice(ConstArrayValue {
                        element: ty,
                        upstream: self.new_sub_addr(Some(addr), (0..expected_len).into(), t),
                        len: count,
                    }))
                }
                TyKind::Dynamic(_, _) => {
                    let addr = usize::from_le_bytes(b[0..b.len() / 2].try_into().unwrap());
                    let ty_id = usize::from_le_bytes(b[b.len() / 2..].try_into().unwrap());
                    let Ok(t) = self.alloc.memory_map.vtable_ty(ty_id) else {
                        return ConstValueKind::Error("<ty-missing-in-vtable-map>");
                    };
                    let Ok(layout) = db.layout_of_ty(t.store(), param_env.store()) else {
                        return ConstValueKind::Error("<layout-error>");
                    };
                    let size = layout.size.bytes_usize();
                    let Some(_data) = self.get_memory(Some(addr), (0..size).into()) else {
                        return ConstValueKind::Error("<ref-data-not-available>");
                    };
                    ConstValueKind::Ref(ConstRefValue::Dyn(self.new_sub_addr(
                        Some(addr),
                        (0..size).into(),
                        ty,
                    )))
                }
                TyKind::Adt(adt, _) if b.len() == 2 * size_of::<usize>() => match adt.def_id() {
                    AdtId::StructId(_) => ConstValueKind::Error("unsized struct"),
                    _ => ConstValueKind::Error("<unsized-enum-or-union>"),
                },
                _ => {
                    // sized values
                    let addr = usize::from_le_bytes(match b.try_into() {
                        Ok(b) => b,
                        Err(_) => {
                            return ConstValueKind::Error("<layout-error>");
                        }
                    });
                    let Ok(layout) = db.layout_of_ty(t.store(), param_env.store()) else {
                        return ConstValueKind::Error("<layout-error>");
                    };
                    let size = layout.size.bytes_usize();
                    let Some(_) = self.get_memory(Some(addr), (0..size).into()) else {
                        return ConstValueKind::Error("<ref-data-not-available>");
                    };

                    ConstValueKind::Ref(ConstRefValue::Ref(self.new_sub_addr(
                        Some(addr),
                        (0..size).into(),
                        t,
                    )))
                }
            },
            TyKind::FnDef(_, _) => ConstValueKind::Error("function type"),
            TyKind::FnPtr(_, _) => ConstValueKind::FnPtr(as_u128_pad_le(b)),
            TyKind::UnsafeBinder(_) => ConstValueKind::Error("unsafe binder"),
            TyKind::Dynamic(_, _) => ConstValueKind::Error("unsized type: dyn"),
            TyKind::Closure(_, _) => ConstValueKind::Error("closure"),
            TyKind::CoroutineClosure(_, _) => ConstValueKind::Error("coroutine closure"),
            TyKind::Coroutine(_, _) => ConstValueKind::Error("coroutine"),
            TyKind::CoroutineWitness(_, _) => ConstValueKind::Error("coroutine witness"),
            TyKind::Never => ConstValueKind::Error("never type"),
            TyKind::Tuple(tys) => {
                let Ok(layout) = db.layout_of_ty(ty.store(), param_env.store()) else {
                    return ConstValueKind::Error("<layout-error>");
                };
                ConstValueKind::Tuple(ConstTupleValue {
                    tys,
                    layout: Layout::clone(&layout),
                    upstream: *self,
                })
            }
            TyKind::Alias(_) => ConstValueKind::Error("placeholder or unknown type"),
            TyKind::Param(_) => ConstValueKind::Error("placeholder or unknown type"),
            TyKind::Bound(_, _) => ConstValueKind::Error("placeholder or unknown type"),
            TyKind::Placeholder(_) => ConstValueKind::Error("placeholder or unknown type"),
            TyKind::Infer(_) => ConstValueKind::Error("placeholder or unknown type"),
            TyKind::Error(_) => ConstValueKind::Error("placeholder or unknown type"),
        }
    }
}

#[derive(Clone)]
pub enum ConstValueKind<'db> {
    Bool(bool),
    Char(char),
    Int(i128),
    Uint(u128),

    // F16(f16),
    F32(f32),
    F64(f64),
    // F128(f128),
    Struct(ConstVariantValue<'db, hir::Struct>),
    Enum(ConstVariantValue<'db, hir::EnumVariant>),
    // str
    Array(ConstArrayValue<'db>),
    RawPtr(u128),
    Ref(ConstRefValue<'db>),
    FnPtr(u128),
    Tuple(ConstTupleValue<'db>),

    Error(&'static str),
}

#[derive(Clone)]
pub struct ConstVariantValue<'db, T: hir_variant::HirVariant> {
    pub def: T,
    pub args: GenericArgs<'db>,
    layout: Layout,
    upstream: ConstValue<'db>,
}

mod hir_variant {
    use crate::ConstVariantValue;
    use hir_ty::db::HirDatabase;

    pub trait HirVariant: Copy + Into<hir::Variant> {
        fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field>;
    }

    impl HirVariant for hir::Struct {
        fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
            self.fields(db)
        }
    }

    impl HirVariant for hir::EnumVariant {
        fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
            self.fields(db)
        }
    }

    impl HirVariant for hir::Union {
        fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
            self.fields(db)
        }
    }

    impl HirVariant for hir::Variant {
        fn fields(self, db: &dyn HirDatabase) -> Vec<hir::Field> {
            self.fields(db)
        }
    }

    impl<'db> From<ConstVariantValue<'db, hir::Struct>> for ConstVariantValue<'db, hir::Variant> {
        fn from(value: ConstVariantValue<'db, hir::Struct>) -> Self {
            value.cast()
        }
    }

    impl<'db> From<ConstVariantValue<'db, hir::EnumVariant>> for ConstVariantValue<'db, hir::Variant> {
        fn from(value: ConstVariantValue<'db, hir::EnumVariant>) -> Self {
            value.cast()
        }
    }

    impl<'db> From<ConstVariantValue<'db, hir::Union>> for ConstVariantValue<'db, hir::Variant> {
        fn from(value: ConstVariantValue<'db, hir::Union>) -> Self {
            value.cast()
        }
    }
}

impl<'db, T: hir_variant::HirVariant> ConstVariantValue<'db, T> {
    pub fn fields(&self, db: &dyn HirDatabase) -> Vec<hir::Field> {
        self.def.fields(db)
    }

    pub fn get_field(&self, field: hir::Field) -> ConstValue<'db> {
        let interner = self.upstream.interner();
        let v = self.def.into();

        let field_types = self.upstream.db.field_types(v.into());
        let field_id = FieldId::from(field);
        let id = field_id.local_id;

        assert_eq!(field_id.parent, v.into(), "the field is not of the variant");

        let offset = (self.layout.fields)
            .offset(u32::from(id.into_raw()) as usize)
            .bytes_usize();
        let ty = (field_types[id].ty())
            .instantiate(interner, self.args)
            .skip_norm_wip();
        let param_env = ParamEnv::empty(interner);
        let krate = self.upstream.source.krate(self.upstream.db).base();
        let param_env = ParamEnvAndCrate { param_env, krate };
        let Ok(layout) = self.upstream.db.layout_of_ty(ty.store(), param_env.store()) else {
            panic!("failed getting layout for field type")
        };
        let size = layout.size.bytes_usize();
        let current_range = self.upstream.range;

        let start = current_range.start + offset;
        let end = current_range.start + offset + size;

        self.upstream.new_sub((start..end).into(), ty)
    }

    fn cast(&self) -> ConstVariantValue<'db, hir::Variant> {
        ConstVariantValue {
            upstream: self.upstream,
            def: self.def.into(),
            args: self.args,
            layout: self.layout.clone(),
        }
    }
}

#[derive(Copy, Clone)]
pub struct ConstArrayValue<'db> {
    element: Ty<'db>,
    len: usize,
    upstream: ConstValue<'db>,
}

impl<'db> ConstArrayValue<'db> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, index: usize) -> Option<ConstValue<'db>> {
        let Ok(layout) = (self.upstream.db).layout_of_ty(
            self.element.store(),
            self.upstream.param_env(self.upstream.interner()).store(),
        ) else {
            unreachable!("already validated before construct");
        };
        let size_one = layout.size.bytes_usize();
        let offset = size_one * index;
        Some(
            self.upstream
                .new_sub((offset..offset + size_one).into(), self.element),
        )
    }
}

impl<'db> IntoIterator for ConstArrayValue<'db> {
    type Item = ConstValue<'db>;
    type IntoIter = ConstArrayValueIterator<'db>;

    fn into_iter(self) -> Self::IntoIter {
        ConstArrayValueIterator {
            components: self,
            index: 0,
        }
    }
}

pub struct ConstArrayValueIterator<'db> {
    components: ConstArrayValue<'db>,
    index: usize,
}

impl<'db> Iterator for ConstArrayValueIterator<'db> {
    type Item = ConstValue<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.components.len() {
            return None;
        }
        let value = self.components.get(self.index);
        self.index += 1;
        value
    }
}

#[derive(Clone)]
pub enum ConstRefValue<'db> {
    Str(String),
    Slice(ConstArrayValue<'db>),
    Dyn(ConstValue<'db>),
    Ref(ConstValue<'db>),
}

#[derive(Clone)]
pub struct ConstTupleValue<'db> {
    layout: Layout,
    tys: Tys<'db>,
    upstream: ConstValue<'db>,
}

impl<'db> ConstTupleValue<'db> {
    pub fn len(&self) -> usize {
        self.tys.len()
    }

    pub fn get_type(&self, index: usize) -> hir::Type<'db> {
        hir::Type::from_ty_owner(
            EarlyBinder::bind(self.tys[index]),
            hir::GenericDef::from(self.upstream.source),
        )
    }

    pub fn get_value(&self, index: usize) -> Option<ConstValue<'db>> {
        let offset = self.layout.fields.offset(index).bytes_usize();
        let ty = self.tys[index];
        let Ok(layout) = self.upstream.db.layout_of_ty(
            ty.store(),
            self.upstream.param_env(self.upstream.interner()).store(),
        ) else {
            return None;
        };
        let size = layout.size.bytes_usize();
        Some(self.upstream.new_sub((offset..offset + size).into(), ty))
    }
}

impl<'db> IntoIterator for ConstTupleValue<'db> {
    type Item = (hir::Type<'db>, ConstValue<'db>);
    type IntoIter = ConstTupleIterator<'db>;

    fn into_iter(self) -> Self::IntoIter {
        ConstTupleIterator {
            tuple: self,
            index: 0,
        }
    }
}

pub struct ConstTupleIterator<'db> {
    tuple: ConstTupleValue<'db>,
    index: usize,
}

impl<'db> Iterator for ConstTupleIterator<'db> {
    type Item = (hir::Type<'db>, ConstValue<'db>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.tuple.tys.len() {
            return None;
        }
        let ty = self.tuple.get_type(self.index);
        let value = self.tuple.get_value(self.index)?;
        self.index += 1;
        Some((ty, value))
    }
}

fn as_u128_pad_le(b: &[u8]) -> u128 {
    let mut buf = [0u8; 16];
    buf[..b.len()].copy_from_slice(b);
    u128::from_le_bytes(buf)
}

fn as_i128_pad_le(b: &[u8]) -> i128 {
    let negative = b.last().unwrap_or(&0) > &127;
    let mut buf = [if negative { 255 } else { 0 }; 16];
    buf[..b.len()].copy_from_slice(b);
    i128::from_le_bytes(buf)
}

fn detect_variant_from_bytes<'a>(
    layout: &'a Layout,
    db: &dyn HirDatabase,
    target_data_layout: &TargetDataLayout,
    b: &[u8],
    e: hir::Enum,
) -> Option<(hir::EnumVariant, &'a Layout)> {
    let (var_id, var_layout) = match &layout.variants {
        hir_def::layout::Variants::Empty => panic!("uninhabited type had value"),
        hir_def::layout::Variants::Single { index } => (e.variants(db)[index.0], layout),
        hir_def::layout::Variants::Multiple {
            tag,
            tag_encoding,
            variants,
            ..
        } => {
            let size = tag.size(target_data_layout).bytes_usize();
            let offset = layout.fields.offset(0).bytes_usize(); // The only field on enum variants is the tag field
            let tag = i128::from_le_bytes(pad16(&b[offset..offset + size], IsSigned::No));
            match tag_encoding {
                TagEncoding::Direct => {
                    let (variant, layout) =
                        variants.iter_enumerated().find_map(|(var_idx, v)| {
                            let def = e.variants(db)[var_idx.0];
                            (db.const_eval_discriminant(def.into()) == Ok(tag)).then_some((def, v))
                        })?;
                    (variant, layout)
                }
                TagEncoding::Niche {
                    untagged_variant,
                    niche_start,
                    ..
                } => {
                    let candidate_tag = tag.wrapping_sub(*niche_start as i128) as usize;
                    let variant = variants
                        .iter_enumerated()
                        .map(|(x, _)| x)
                        .filter(|x| x != untagged_variant)
                        .nth(candidate_tag)
                        .unwrap_or(*untagged_variant);
                    (e.variants(db)[variant.0], &variants[variant])
                }
            }
        }
    };
    Some((var_id, var_layout))
}
