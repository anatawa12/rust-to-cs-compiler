use crate::codegen::body::eir;
use crate::codegen::body::eir::EirNode;
use hir::HasSource;
use hir::db::HirDatabase;
use ide_db::line_index;
use itertools::Either;
use ra_internal::LangItems;
use std::marker::PhantomData;
use syntax::ast;
use vfs::Vfs;

/// The wrapper struct of hir::Semantics, that handles eir nodes rather than ast nodes.
///
/// There are few other functions that are not avaiable in hir::Semantics, but we do use for
/// semantics analysis.
pub struct EirSemantics<'db> {
    db: &'db dyn HirDatabase,
    hir: hir::Semantics<'db, dyn HirDatabase>,
    vfs: &'db Vfs,
    lang_items: &'db LangItems,
}

impl<'db> EirSemantics<'db> {
    pub fn new(db: &'db dyn HirDatabase, vfs: &'db Vfs, lang_items: &'db LangItems) -> Self {
        Self {
            db,
            hir: hir::Semantics::new_dyn(db),
            vfs,
            lang_items,
        }
    }
}

/// EirSemantics Specific: We provide a way to show location of several nodes, including ast nodes.
impl<'db> EirSemantics<'db> {
    pub fn location<M>(&self, node: &impl NodeWithLocation<M>) -> String {
        if let Some(loc) = node.text_range(self) {
            let loc = self.hir.diagnostics_display_range_for_range(loc);

            let path = self.vfs.file_path(loc.file_id);
            let line_index = line_index(self.db, loc.file_id);
            let line_col = line_index.line_col(loc.range.start());

            format!("{}:{}:{}", path, line_col.line + 1, line_col.col + 1)
        } else {
            "unknown location".to_string()
        }
    }
}

pub trait NodeWithLocation<M> {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>>;
}

// blanket impl for ref wrapping
pub struct NodeWithLocationRef<T>(PhantomData<T>);
impl<T: NodeWithLocation<M>, M> NodeWithLocation<NodeWithLocationRef<M>> for &T {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        (**self).text_range(sem)
    }
}

pub struct NodeWithLocationAstNode;
impl<T: syntax::AstNode> NodeWithLocation<NodeWithLocationAstNode> for T {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        Some(hir::InFile::new(
            sem.hir.hir_file_for(self.syntax()),
            self.syntax().text_range(),
        ))
    }
}

impl NodeWithLocation<syntax::SyntaxNode> for syntax::SyntaxNode {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        Some(hir::InFile::new(
            sem.hir.hir_file_for(self),
            self.text_range(),
        ))
    }
}

impl NodeWithLocation<syntax::SyntaxToken> for syntax::SyntaxToken {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        Some(hir::InFile::new(
            sem.hir.hir_file_for(&self.parent()?),
            self.text_range(),
        ))
    }
}

impl NodeWithLocation<hir::InFile<syntax::TextRange>> for hir::InFile<syntax::TextRange> {
    fn text_range(&self, _: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        Some(*self)
    }
}

pub struct NodeWithLocationNodeOrToken<T>(PhantomData<T>);
impl<Ast: syntax::AstNode> NodeWithLocation<NodeWithLocationNodeOrToken<Ast>>
    for syntax::NodeOrToken<Ast, syntax::SyntaxToken>
{
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        match self {
            syntax::NodeOrToken::Node(node) => node.text_range(sem),
            syntax::NodeOrToken::Token(token) => NodeWithLocation::text_range(token, sem),
        }
    }
}

pub struct NodeWithLocationEirExpr;
impl<T: Clone + Into<eir::Expr>> NodeWithLocation<NodeWithLocationEirExpr> for T {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        let eir::NodeInfo::Ast(ast) = self.clone().into().node_info() else {
            return None;
        };
        ast.text_range(sem)
    }
}

pub struct NodeWithLocationEirPat;
impl<T: Clone + Into<eir::Pat>> NodeWithLocation<NodeWithLocationEirPat> for T {
    fn text_range(&self, sem: &EirSemantics) -> Option<hir::InFile<syntax::TextRange>> {
        let eir::NodeInfo::Ast(ast) = self.clone().into().node_info() else {
            return None;
        };
        ast.text_range(sem)
    }
}

impl<'db> EirSemantics<'db> {
    #[track_caller]
    pub fn type_of_expr(&self, expr: &eir::Expr) -> hir::TypeInfo<'db> {
        match expr.node_info() {
            eir::NodeInfo::Ast(expr) => self
                .hir
                .type_of_expr(&expr)
                .unwrap_or_else(|| panic!("unknown type expr at {}", self.location(&expr))),
            eir::NodeInfo::None => panic!("no type for expr info"),
        }
    }
    #[track_caller]
    pub fn type_of_expr_opt(&self, expr: &eir::Expr) -> Option<hir::TypeInfo<'db>> {
        match expr.node_info() {
            eir::NodeInfo::Ast(expr) => self.hir.type_of_expr(&expr),
            eir::NodeInfo::None => None,
        }
    }

    pub fn resolve_path_with_subst(
        &self,
        path: &eir::Path,
    ) -> Option<(
        hir::PathResolution<'db>,
        Option<hir::GenericSubstitution<'db>>,
    )> {
        match path {
            eir::Path::Ast(ast) => self.hir.resolve_path_with_subst(ast),
            eir::Path::MethodCall(ast) => match self.hir.resolve_method_call_fallback(ast) {
                Some((Either::Left(f), sub)) => {
                    Some((hir::PathResolution::Def(hir::ModuleDef::Function(f)), sub))
                }
                _ => None,
            },
            eir::Path::ScopedName { scope, name } => {
                let mut resolved = None;
                self.hir
                    .scope(scope)?
                    .process_all_names(&mut |cur_name, def| {
                        if cur_name.as_str() == name {
                            resolved.get_or_insert(def);
                        }
                    });
                match resolved? {
                    hir::ScopeDef::ModuleDef(r) => Some((hir::PathResolution::Def(r), None)),
                    hir::ScopeDef::GenericParam(_) => None,
                    hir::ScopeDef::ImplSelfType(r) => {
                        Some((hir::PathResolution::SelfType(r), None))
                    }
                    hir::ScopeDef::AdtSelfType(r) => {
                        Some((hir::PathResolution::Def(r.into()), None))
                    }
                    hir::ScopeDef::Local(r) => Some((hir::PathResolution::Local(r), None)),
                    hir::ScopeDef::Label(_) => None,
                    hir::ScopeDef::Unknown => None,
                }
            }
            eir::Path::BuiltinItem(_) => None,
        }
    }

    pub fn resolve_path(&self, path: &eir::Path) -> Option<hir::PathResolution<'db>> {
        self.resolve_path_with_subst(path).map(|(r, _)| r)
    }

    pub fn type_of_pat(&self, pat: &eir::Pat) -> Option<hir::TypeInfo<'db>> {
        match pat.node_info() {
            eir::NodeInfo::Ast(ast) => self.hir.type_of_pat(&ast),
            eir::NodeInfo::None => None,
        }
    }

    pub fn resolve_field(
        &self,
        field_expr: &eir::FieldExpr,
    ) -> Option<Either<hir::Field, hir::TupleField<'db>>> {
        match field_expr.node_info() {
            eir::NodeInfo::Ast(ast) => self.hir.resolve_field(&ast),
            eir::NodeInfo::None => None,
        }
    }

    pub fn resolve_variant(&self, record_expr: &eir::RecordExpr) -> Option<hir::Variant> {
        match record_expr.node_info() {
            eir::NodeInfo::Ast(ast) => self.hir.resolve_variant(ast.clone()),
            eir::NodeInfo::None => None,
        }
    }

    pub fn resolve_bind_pat_to_const(&self, pat: &eir::IdentPat) -> Option<hir::ModuleDef> {
        match pat.node_info() {
            eir::NodeInfo::Ast(ast) => self.hir.resolve_bind_pat_to_const(&ast),
            eir::NodeInfo::None => None,
        }
    }

    pub fn resolve_type(&self, ty: &ast::Type) -> Option<hir::Type<'db>> {
        self.hir.resolve_type(ty)
    }
}

impl<'db> EirSemantics<'db> {
    pub fn source<Def: HasSource>(&self, def: Def) -> Option<hir::InFile<Def::Ast>> {
        self.hir.source(def)
    }

    pub fn resolve_macro_call(&self, macro_call: &ast::MacroCall) -> Option<hir::Macro> {
        self.hir.resolve_macro_call(macro_call)
    }

    pub fn expand_macro_call(
        &self,
        macro_call: &ast::MacroCall,
    ) -> Option<hir::InFile<syntax::SyntaxNode>> {
        self.hir.expand_macro_call(macro_call)
    }

    pub fn descend_into_macros(
        &self,
        token: syntax::SyntaxToken,
    ) -> smallvec::SmallVec<[syntax::SyntaxToken; 1]> {
        self.hir.descend_into_macros(token)
    }
}

impl<'db> EirSemantics<'db> {
    pub fn to_def<T: ToDef>(&self, eir: &T) -> Option<T::Result<'db>> {
        eir.to_def_impl(self)
    }
}

pub trait ToDef {
    type Result<'db>;
    fn to_def_impl<'db>(&self, sem: &EirSemantics<'db>) -> Option<Self::Result<'db>>;
}

macro_rules! to_def_hir {
    ($ty: ty => $def: ty) => {
        impl ToDef for $ty {
            type Result<'db> = $def;

            fn to_def_impl<'db>(&self, sem: &EirSemantics<'db>) -> Option<Self::Result<'db>> {
                sem.hir.to_def(self)
            }
        }
    };
}
to_def_hir!(ast::IdentPat => hir::Local<'db>);
to_def_hir!(ast::Fn => hir::Function);
to_def_hir!(ast::Enum => hir::Enum);
to_def_hir!(ast::Struct => hir::Struct);
to_def_hir!(ast::Impl => hir::Impl);
to_def_hir!(ast::Const => hir::Const);

impl ToDef for eir::IdentPat {
    type Result<'db> = hir::Local<'db>;

    fn to_def_impl<'db>(&self, sem: &EirSemantics<'db>) -> Option<Self::Result<'db>> {
        match &self.node_info() {
            eir::NodeInfo::Ast(ast) => sem.hir.to_def(ast),
            eir::NodeInfo::None => None,
        }
    }
}

impl ToDef for eir::SelfParam {
    type Result<'db> = hir::Local<'db>;

    fn to_def_impl<'db>(&self, sem: &EirSemantics<'db>) -> Option<Self::Result<'db>> {
        match &self.node_info() {
            eir::NodeInfo::Ast(ast) => sem.hir.to_def(ast),
            eir::NodeInfo::None => None,
        }
    }
}

impl ToDef for eir::Fn {
    type Result<'db> = hir::Function;

    fn to_def_impl<'db>(&self, sem: &EirSemantics<'db>) -> Option<Self::Result<'db>> {
        match &self.node_info() {
            eir::NodeInfo::Ast(ast) => sem.hir.to_def(ast),
            eir::NodeInfo::None => None,
        }
    }
}
