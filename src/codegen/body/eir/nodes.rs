use super::*;

use super::children::ChildrenContainer;
use super::eir_macros::LowerCast;
use syntax::ast;

def_eir!(
    pub enum Expr {
        #[manual_construct]
        #[common_info_ast_ty = NoNode]
        RawCodeExpr {
            code: output::Code,
            divergent: bool,
        },
        #[manual_construct]
        #[common_info_ast_ty = ast::ArrayExpr]
        ArrayRepeatExpr {
            initializer: Expr,
            repeat: Expr,
        },
        #[manual_construct]
        #[common_info_ast_ty = ast::ArrayExpr]
        ArrayElementListExpr {
            elements: ChildrenContainer<Expr>,
        },
        // AsmExpr not supported
        #[common_info_ast_ty = ast::Expr]
        AwaitExpr {
            expr: Expr,
        },
        // BecomeExpr not supported
        #[common_info_ast_ty = ast::Expr]
        BinExpr {
            lhs: Expr,
            rhs: Expr,
            op_kind: BinaryOp,
        },
        #[manual_construct]
        BlockExpr {
            modifier: Option<BlockModifier>,
            statements: ChildrenContainer<Stmt>,
            tail_expr: Option<Expr>,
        },
        BreakExpr {
            expr: Option<Expr>,
            lifetime: Option<Lifetime>,
        },
        #[common_info_ast_ty = ast::Expr]
        CallExpr {
            expr: Expr,
            arg_list: ArgList,
        },
        CastExpr {
            expr: Expr,
            ty: ast::Type,
        },
        ClosureExpr {
            async_token: bool,
            param_list: ParamList,
            body: Expr,
        },
        ContinueExpr {
            lifetime: Option<Lifetime>,
        },
        FieldExpr {
            expr: Expr,
            name_ref: NameRef,
        },
        ForExpr {
            pat: Pat,
            iterable: Expr,
            label: Option<Label>,
            loop_body: BlockExpr,
        },
        // FormatArgsExpr : we process at macro level
        IfExpr {
            condition: Expr,
            then_branch: BlockExpr,
            else_branch: Option<ElseBranch>,
        },
        // IncludeBytesExpr: not supported; process at macro
        IndexExpr {
            base: Expr,
            index: Expr,
        },
        // Used in if let.
        LetExpr {
            pat: Pat,
            expr: Expr,
        },
        Literal {
            kind: LiteralKind,
        },
        LoopExpr {
            label: Option<Label>,
            loop_body: BlockExpr,
        },
        // MacroExpr: expanded
        MatchExpr {
            expr: Expr,
            match_arm_list: MatchArmList,
        },
        // OffsetOfExpr: we do in macro level
        // ParenExpr: lowered
        PathExpr {
            // empty body; resolve is important
            path: Path,
        },
        PrefixExpr {
            op_kind: UnaryOp,
            expr: Expr,
        },
        RangeExpr {
            start: Option<Expr>,
            end: Option<Expr>,
            op_kind: RangeOp,
        },
        RecordExpr {
            // path: resolve is important
            record_expr_field_list: RecordExprFieldList,
        },
        RefExpr {
            expr: Expr,
        },
        ReturnExpr {
            expr: Option<Expr>,
        },
        TryExpr {
            expr: Expr,
        },
        TupleExpr {
            fields: ChildrenContainer<Expr>,
        },
        UnderscoreExpr {
            // there is zero body for this expr
        },
        WhileExpr {
            label: Option<Label>,
            condition: Expr,
            loop_body: BlockExpr,
        },
        // YeetExpr: not stable
        // YieldExpr: not stable

        // macro expanded exprs
        #[manual_construct]
        #[common_info_ast_ty = ast::MacroExpr]
        FormatArgsExpr {
            segments: ChildrenContainer<FormatArgsSegment>,
        },
        #[manual_construct]
        #[common_info_ast_ty = ast::MacroExpr]
        VecRepeatExpr {
            initializer: Expr,
            repeat: Expr,
        },
        #[manual_construct]
        #[common_info_ast_ty = ast::MacroExpr]
        VecListExpr {
            elements: ChildrenContainer<Expr>,
        },
        #[manual_construct]
        #[common_info_ast_ty = ast::MacroExpr]
        // similar to block, but does not introduce scope.
        // This only be valid for ExprStmt or tail_expr(s)
        MacroStmts {
            statements: ChildrenContainer<Stmt>,
            tail_expr: Option<Expr>,
        },
    }

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Eir {
        match self {
            ast::Expr::ParenExpr(paren) => ctx.lower(paren.expr().unwrap()),
            /*ast::Expr::BinExpr(bin) => From::from(new_eir_node!(BinExpr {
                lhs: ctx.lower(bin.lhs()).lower_cast("lhs"),
                rhs: ctx.lower(bin.rhs()).lower_cast("rhs"),
                op_details: LowerCast::<(_, _)>::lower_cast(bin.op_details(), "op_details").1,
                node_info: ExprInfo::Ast(bin),
            })),*/
            ast::Expr::MethodCallExpr(method_call) => From::from(new_eir_node!(CallExpr {
                expr: From::from(new_eir_node!(PathExpr {
                    path: Path::MethodCall(method_call.clone()),
                    node_info: NodeInfo::None,
                })),
                arg_list: new_eir_node!(ArgList {
                    args: From::from(
                        [method_call.receiver().unwrap()]
                            .into_iter()
                            .chain(method_call.arg_list().unwrap().args())
                            .map(|x| ctx.lower(x))
                            .collect::<Vec<_>>()
                    ),
                    node_info: NodeInfo::None,
                }),
                node_info: NodeInfo::Ast(method_call.into()),
            })),
            /*
             */
            ast::Expr::MacroExpr(macro_expr) => ctx.emit_expr_macro(macro_expr),
            ast::Expr::ArrayExpr(array) => match array.kind() {
                ArrayExprKind::Repeat {
                    initializer,
                    repeat,
                } => From::from(new_eir_node!(ArrayRepeatExpr {
                    initializer: ctx.lower(initializer).lower_cast("initializer"),
                    repeat: ctx.lower(repeat).lower_cast("repeat"),
                    node_info: NodeInfo::Ast(array),
                })),
                ArrayExprKind::ElementList(elements) => {
                    From::from(new_eir_node!(ArrayElementListExpr {
                        elements: ctx.lower(elements),
                        node_info: NodeInfo::Ast(array),
                    }))
                }
            },
            ast::Expr::BlockExpr(block) => From::from(ctx.lower(block)),
            test => panic!("{:?}", test),
        }
    }
);

impl LowerToEir for ast::BlockExpr {
    type Eir = BlockExpr;

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> BlockExpr {
        new_eir_node!(BlockExpr {
            modifier: ctx.lower(self.modifier()),
            statements: ctx.lower(self.stmt_list().unwrap().statements()),
            tail_expr: ctx.lower(self.stmt_list().unwrap().tail_expr()),
            node_info: NodeInfo::Ast(self),
        })
    }
}

#[derive(Clone)]
pub enum BlockModifier {
    Async,
    Unsafe,
    /*Try*/
    Const,
    AsyncGen,
    Gen,
    Label(Label),
}

impl LowerToEir for ast::BlockModifier {
    type Eir = BlockModifier;
    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Self::Eir {
        match self {
            ast::BlockModifier::Async(_) => BlockModifier::Async,
            ast::BlockModifier::Unsafe(_) => BlockModifier::Unsafe,
            ast::BlockModifier::Try { .. } => panic!("try blocks"),
            ast::BlockModifier::Const(_) => BlockModifier::Const,
            ast::BlockModifier::AsyncGen(_) => BlockModifier::AsyncGen,
            ast::BlockModifier::Gen(_) => BlockModifier::Gen,
            ast::BlockModifier::Label(label) => BlockModifier::Label(ctx.lower(label)),
        }
    }
}

def_eir!(
    pub enum ElseBranch {
        Block(BlockExpr),
        IfExpr,
    }

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Eir {
        match self {}
    }
);

def_eir!(
    pub struct MatchArmList {
        arms: ChildrenContainer<MatchArm>,
    }
);

def_eir!(
    pub struct ArgList {
        args: ChildrenContainer<Expr>,
    }
);

def_eir!(
    pub struct MatchArm {
        pat: Pat,
        guard: Option<Expr>,
        expr: Expr,
    }
);

impl LowerToEir for ast::MatchGuard {
    type Eir = Expr;

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Expr {
        ctx.lower(self.condition().unwrap())
    }
}

def_eir!(
    pub struct RecordExprFieldList {
        fields: ChildrenContainer<RecordExprField>,
        spread: Option<Expr>,
    }
);

def_eir!(
    pub struct RecordExprField {
        field_name: Option<NameRef>,
        expr: Expr,
    }
);

def_eir!(
    pub struct ParamList {
        self_param: Option<SelfParam>,
        params: ChildrenContainer<Param>,
    }
);

def_eir!(
    pub struct SelfParam {}
);

def_eir!(
    pub struct Param {
        pat: Pat,
        // ty: Option<Ty>, // resolution is important
    }
);

def_eir!(
    pub enum Stmt {
        ExprStmt,
        LetStmt,
        #[manual_construct]
        Item,
    }

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Eir {
        match self {
            ast::Stmt::Item(item) => Stmt::from(item),
        }
    }
);

def_eir!(
    pub struct ExprStmt {
        expr: Expr,
    }
);

def_eir!(
    pub struct LetStmt {
        pat: Pat,
        // ty: Type, // no type, use analyzed
        initializer: Option<Expr>,
        let_else: Option<LetElse>,
    }
);

def_eir!(
    pub struct LetElse {
        block_expr: BlockExpr,
    }
);

#[derive(Clone)]
pub enum FormatArgsSegment {
    Literal(String),
    Braces(char),
    Expr(Expr),
}

impl<'a> eir_macros::EirAccessType<'a> for FormatArgsSegment {
    type Result = &'a FormatArgsSegment;

    fn cast(&'a self) -> Self::Result {
        self
    }

    fn accept_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> ControlFlow<V::Break> {
        match self {
            FormatArgsSegment::Literal(_) => ControlFlow::Continue(()),
            FormatArgsSegment::Braces(_) => ControlFlow::Continue(()),
            FormatArgsSegment::Expr(expr) => visitor.visit_expr(expr),
        }
    }
}

def_eir!(
    pub enum Pat {
        // BoxPat // nightly
        // ConstBlockPat // nightly
        // DerefPat // nightly
        IdentPat {
            name: ast::Name,
            pat: Option<Pat>,
        },
        LiteralPat {
            literal: Literal,
        },
        // MacroPat {} // lowered
        // NotNull // nightly
        OrPat {
            pats: ChildrenContainer<Pat>,
        },
        // ParenPat // lowered
        PathPat {
            path: Path,
        },
        RangePat {
            start: Option<Pat>,
            end: Option<Pat>,
            op_kind: RangeOp,
        },
        RecordPat {
            path: Path,
            record_pat_field_list: RecordPatFieldList,
        },
        RefPat {
            pat: Pat,
            mut_token: bool,
        },
        RestPat {},
        SlicePat {
            components: SlicePatComponents,
        },
        TuplePat {
            fields: ChildrenContainer<Pat>,
        },
        TupleStructPat {
            path: Path,
            fields: ChildrenContainer<Pat>,
        },
        WildcardPat {},
    }

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Eir {
        match self {
            test => panic!("unsupported: {:?}", test),
        }
    }
);

def_eir!(
    #[manual_construct]
    pub struct RecordPatFieldList {
        fields: ChildrenContainer<RecordPatField>,
        rest: bool,
    }
);
impl LowerToEir for ast::RecordPatFieldList {
    type Eir = RecordPatFieldList;

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> RecordPatFieldList {
        new_eir_node!(RecordPatFieldList {
            fields: ctx.lower(self.fields()),
            rest: self.rest_pat().is_some(),
            node_info: NodeInfo::Ast(self),
        })
    }
}

def_eir!(
    #[common_info_ast_ty = NoNode]
    #[manual_construct]
    pub struct SlicePatComponents {
        prefix: Vec<Pat>,
        slice: Option<Pat>,
        suffix: Vec<Pat>,
    }
);

impl LowerToEir for ast::SlicePatComponents {
    type Eir = SlicePatComponents;

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> SlicePatComponents {
        new_eir_node!(SlicePatComponents {
            prefix: ctx.lower(self.prefix.clone()),
            slice: ctx.lower(self.slice.clone()),
            suffix: ctx.lower(self.suffix.clone()),
            node_info: NodeInfo::None,
        })
    }
}

def_eir!(
    pub struct RecordPatField {
        field_name: NameOrNameRef,
        pat: Pat,
    }
);

// Path, and NameRef are a special node might have special meaning
#[derive(Clone)]
pub enum Path {
    Ast(ast::Path),
    MethodCall(ast::MethodCallExpr),
    BuiltinItem(BuiltinItem),
    ScopedName {
        scope: syntax::SyntaxNode,
        name: String,
    },
}

impl LowerToEir for ast::Path {
    type Eir = Path;

    fn lower_to_eir(self, _: &LowerToEirCtx) -> Path {
        Path::Ast(self)
    }
}

impl EirNode for Path {
    type AstNode = AnySyntax;

    fn node_info(&self) -> NodeInfo<Self::AstNode> {
        match self {
            Path::Ast(ast) => NodeInfo::Ast(AnySyntax(syntax::AstNode::syntax(ast).clone())),
            Path::MethodCall(ast) => NodeInfo::Ast(AnySyntax(syntax::AstNode::syntax(ast).clone())),
            Path::BuiltinItem(_) => NodeInfo::None,
            Path::ScopedName { .. } => NodeInfo::None,
        }
    }

    fn accept_children_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        _: &mut V,
    ) -> ControlFlow<V::Break> {
        ControlFlow::Continue(())
    }
}

#[derive(Clone)]
pub enum BuiltinItem {
    DisplayStr,
    DebugStr,
}

#[derive(Clone)]
pub enum NameRef {
    Ast(ast::NameRef),
}

impl LowerToEir for ast::NameRef {
    type Eir = NameRef;

    fn lower_to_eir(self, _: &LowerToEirCtx) -> NameRef {
        NameRef::Ast(self)
    }
}

impl NameRef {
    pub fn text(&self) -> &str {
        match self {
            NameRef::Ast(ast) => ast.text(),
        }
    }
}

impl EirNode for NameRef {
    type AstNode = ast::NameRef;

    fn node_info(&self) -> NodeInfo<Self::AstNode> {
        match self {
            NameRef::Ast(ast) => NodeInfo::Ast(ast.clone()),
        }
    }

    fn accept_children_mut<V: ?Sized + MutatingEirVisitor>(
        &mut self,
        _: &mut V,
    ) -> ControlFlow<V::Break> {
        ControlFlow::Continue(())
    }
}

def_eir!(
    pub enum NameOrNameRef {
        Name,
        NameRef,
    }

    fn lower_to_eir(self, ctx: &LowerToEirCtx) -> Eir {
        match self {}
    }
);

// Container of Expr
def_eir!(
    pub struct Fn {
        param_list: ParamList,
        body: Option<BlockExpr>,
    }
);
