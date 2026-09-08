use crate::codegen::body::eir;
use crate::codegen::body::eir::LowerToEirCtx;
use crate::codegen::{CodeGenerator, item_exclusion};
use ide_db::FxHashMap;
use itertools::{Either, Itertools};
use std::iter;
use std::str::FromStr;
use syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxToken, T, ast};

macro_rules! raw_code_expr {
    (node: $expr: expr, $($tt:tt)*) => {
        crate::codegen::body::eir::Expr::from(new_eir_node!(crate::codegen::body::eir::RawCodeExpr {
            code: fcode!($($tt)*),
            node_info: crate::codegen::body::eir::NodeInfo::Ast($expr.into()),
        }))
    };
    ($($tt:tt)*) => {
        crate::codegen::body::eir::Expr::from(new_eir_node!(crate::codegen::body::eir::RawCodeExpr {
            code: fcode!($($tt)*),
            node_info: crate::codegen::body::eir::NodeInfo::None,
        }))
    };
}

impl<'g, 'db> LowerToEirCtx<'g, 'db> {
    pub(super) fn emit_expr_macro(&self, macro_expr: ast::MacroExpr) -> eir::Expr {
        let macro_call = &macro_expr.macro_call().unwrap();
        let Some(macro_) = self.cg.eir_sem.resolve_macro_call(macro_call) else {
            panic!(
                "Unresolved macro call at {}",
                self.cg.expr_location_ast(macro_call)
            )
        };
        let tt = macro_call.token_tree().unwrap();

        if self.should_inline_macro(macro_call) {
            // Macro that expands
            let expanded = self.cg.eir_sem.expand_macro_call(macro_call).unwrap().value;
            if let Some(stmts) = ast::MacroStmts::cast(expanded.clone()) {
                From::from(new_eir_node!(eir::MacroStmts {
                    statements: self.lower(stmts.statements()),
                    tail_expr: self.lower(stmts.expr()),
                    node_info: eir::NodeInfo::Ast(macro_expr),
                }))
            } else if let Some(expr) = ast::Expr::cast(expanded.clone()) {
                self.lower(expr)
            } else {
                panic!(
                    "{:?} at {}",
                    expanded,
                    self.cg.expr_location_ast(macro_call)
                )
            }
        } else if Some(macro_) == self.cg.lang_items.unreachable() {
            raw_code_expr!(r#"throw new PanicException("unreachable")"#)
        } else if Some(macro_) == self.cg.lang_items.panic() {
            raw_code_expr!(r#"throw new PanicException("panic")"#)
        } else if Some(macro_) == self.cg.lang_items.vec() {
            // let ty = (self.cg.sem.type_of_expr(expr)).unwrap_or_else(|| {
            //     panic!(
            //         "Unresolved macro call at {}",
            //         self.cg.expr_location_ast(expr)
            //     )
            // });
            //let (_vec, args) = ty.original().as_adt_with_args().unwrap();
            //let element_ty = { args }.swap_remove(0).unwrap();
            let mut parser = MacroParser::new(self.cg, &tt);

            if let Some(first_expr) = parser.next_expr() {
                if parser.take_token(T![;]) {
                    //let count = parser.next_expr().unwrap();
                    panic!(
                        "Unsupported vec![T; N] call at {}",
                        self.cg.expr_location_ast(macro_call)
                    )
                } else if parser.take_token(T![,]) || parser.is_end() {
                    let mut args = Vec::new();
                    args.push(self.lower(first_expr));

                    while parser.take_token(T![,])
                        && let Some(next_expr) = parser.next_expr()
                    {
                        args.push(self.lower(next_expr));
                    }

                    From::from(new_eir_node!(eir::VecListExpr {
                        elements: args.into(),
                        node_info: eir::NodeInfo::Ast(macro_expr),
                    }))
                } else {
                    panic!(
                        "Unsupported vec![] call at {}",
                        self.cg.expr_location_ast(macro_call)
                    )
                }
            } else {
                From::from(new_eir_node!(eir::VecListExpr {
                    elements: eir_children![],
                    node_info: eir::NodeInfo::Ast(macro_expr),
                }))
            }
        } else if Some(macro_) == self.cg.lang_items.pin() {
            let mut parser = MacroParser::new(self.cg, &tt);
            self.lower(parser.next_expr().unwrap())
        } else if Some(macro_) == self.cg.lang_items.format_args()
            || Some(macro_) == self.cg.lang_items.format()
        {
            // NOTE: this might breaks execution order of each format args
            parse_rest_as_format_args(self, macro_call, &mut MacroParser::new(self.cg, &tt)).into()
        } else if Some(macro_) == self.cg.lang_items.log_trace() {
            log_macro("trace", self, macro_expr, macro_call, tt)
        } else if Some(macro_) == self.cg.lang_items.log_debug() {
            log_macro("debug", self, macro_expr, macro_call, tt)
        } else if Some(macro_) == self.cg.lang_items.log_info() {
            log_macro("info", self, macro_expr, macro_call, tt)
        } else if Some(macro_) == self.cg.lang_items.log_warn() {
            log_macro("warn", self, macro_expr, macro_call, tt)
        } else if Some(macro_) == self.cg.lang_items.log_error() {
            log_macro("error", self, macro_expr, macro_call, tt)
        } else if Some(macro_) == self.cg.lang_items.assert() {
            let parser = &mut MacroParser::new(self.cg, &tt);
            let Some(condition) = parser.next_expr() else {
                panic!(
                    "Expected assert!() first argument to be a condition at {}",
                    self.cg.expr_location_ast(macro_call)
                )
            };
            parser.take_token(T![,]);
            From::from(new_eir_node!(eir::CallExpr {
                expr: raw_code_expr!("Asserts.assert").into(),
                arg_list: new_eir_node!(eir::ArgList {
                    args: eir_children![
                        self.lower(condition),
                        From::from(new_eir_node!(eir::ClosureExpr {
                            async_token: false,
                            param_list: new_eir_node!(eir::ParamList {
                                self_param: None,
                                params: eir_children![],
                            }),
                            body: parse_rest_as_format_args(self, macro_call, parser).into(),
                            node_info: eir::NodeInfo::None,
                        })),
                    ]
                }),
                node_info: eir::NodeInfo::Ast(macro_expr.into()),
            }))
        } else if Some(macro_) == self.cg.lang_items.cfg() {
            let tt_as_str = tt.syntax().to_string();
            if tt_as_str == "(windows)" {
                From::from(new_eir_node!(eir::CallExpr {
                    expr: raw_code_expr!("Cfg.IsWindows"),
                    arg_list: new_eir_node!(eir::ArgList {
                        args: eir_children![]
                    }),
                    node_info: eir::NodeInfo::Ast(macro_expr.into()),
                }))
            } else {
                panic!("Unsupported cfg expression: {}", tt_as_str);
            }
        } else if Some(macro_) == self.cg.lang_items.lazy_static() {
            // TODO? consider generating static initializer pattern?
            raw_code_expr!("/* lazy_static placeholder */")
        } else if Some(macro_) == self.cg.lang_items.try_join() {
            let parser = &mut MacroParser::new(self.cg, &tt);

            let mut exprs = vec![];
            while !parser.is_end() {
                let Some(expr) = parser.next_expr() else {
                    panic!("Expected expression");
                };
                exprs.push(self.lower(expr));
                if !parser.take_token(T![,]) && !parser.is_end() {
                    panic!(
                        "Expected comma at {}",
                        self.cg.expr_location_ast(macro_call)
                    );
                }
            }

            From::from(new_eir_node!(eir::AwaitExpr {
                expr: From::from(new_eir_node!(eir::CallExpr {
                    expr: raw_code_expr!("RustTask.TryJoin"),
                    arg_list: new_eir_node!(eir::ArgList { args: exprs.into() }),
                    node_info: eir::NodeInfo::None,
                })),
                node_info: eir::NodeInfo::Ast(macro_expr.into()),
            }))
        } else {
            panic!(
                "unsupported macro {macro_path} at {loc}",
                macro_path = macro_call.path().unwrap().syntax().text(),
                loc = self.cg.expr_location_ast(macro_call),
            );
        }
    }

    pub(super) fn should_inline_macro(&self, macro_call: &ast::MacroCall) -> bool {
        let Some(macro_) = self.cg.eir_sem.resolve_macro_call(macro_call) else {
            panic!(
                "Unresolved macro call at {}",
                self.cg.expr_location_ast(macro_call)
            )
        };
        let is_inline = item_exclusion::has_attr(macro_, "r2cs_inline", self.cg.db);
        Some(macro_) == self.cg.lang_items.matches()
            || Some(macro_) == self.cg.lang_items.ready()
            || Some(macro_) == self.cg.lang_items.write()
            || is_inline
    }
}

fn log_macro(
    level: &str,
    ctx: &LowerToEirCtx,
    macro_expr: ast::MacroExpr,
    macro_call: &ast::MacroCall,
    tt: ast::TokenTree,
) -> eir::Expr {
    let mut parser = MacroParser::new(ctx.cg, &tt);

    while !parser
        .peek()
        .is_some_and(|x| x.as_token().is_some_and(|x| x.kind() == SyntaxKind::STRING))
    {
        // kv
        eprintln!("parsing kv with {:?}", parser.peek());
        assert!(
            parser
                .next()
                .is_some_and(|x| x.as_token().is_some_and(|x| x.kind() == SyntaxKind::IDENT)),
            "not ident at {}",
            ctx.cg.any_location_ast(parser.peek().unwrap())
        );
        assert!(
            parser
                .next()
                .is_some_and(|x| x.as_token().is_some_and(|x| x.kind() == SyntaxKind::EQ)),
            "not eq at {}",
            ctx.cg.any_location_ast(parser.peek().unwrap())
        );
        parser.next_expr();
        assert!(
            parser.next().is_some_and(|x| {
                x.as_token()
                    .is_some_and(|x| x.kind() == SyntaxKind::SEMICOLON)
            }),
            "not SEMI at {}",
            ctx.cg.any_location_ast(parser.peek().unwrap())
        );
    }

    From::from(new_eir_node!(eir::CallExpr {
        expr: raw_code_expr!("Logging.{level}").into(),
        arg_list: new_eir_node!(eir::ArgList {
            args: eir_children![parse_rest_as_format_args(ctx, macro_call, &mut parser).into(),],
        }),
        node_info: eir::NodeInfo::Ast(macro_expr.into()),
    }))
}

type TokenTreeElement = NodeOrToken<ast::TokenTree, SyntaxToken>;

#[derive(Clone)]
struct MacroParser<'g, 'db, I: Iterator<Item = TokenTreeElement>> {
    cg: &'g CodeGenerator<'db>,
    #[allow(dead_code)]
    surrounding: SyntaxToken,
    iterator: iter::Peekable<I>,
}

impl<'g, 'db> MacroParser<'g, 'db, std::vec::IntoIter<TokenTreeElement>> {
    pub fn new(
        cg: &'g CodeGenerator<'db>,
        token: &ast::TokenTree,
    ) -> MacroParser<'g, 'db, impl Iterator<Item = TokenTreeElement> + Clone> {
        let mut iterator = token
            .syntax()
            .children_with_tokens()
            .filter_map(|not| match not {
                NodeOrToken::Node(node) => ast::TokenTree::cast(node).map(NodeOrToken::Node),
                NodeOrToken::Token(t) => Some(NodeOrToken::Token(t)),
            })
            .filter(|x| {
                x.as_token()
                    .is_none_or(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE))
            })
            .peekable();
        MacroParser {
            cg,
            surrounding: iterator.next().unwrap().into_token().unwrap(),
            iterator,
        }
    }
}

impl<'g, 'db, I: Iterator<Item = TokenTreeElement>> MacroParser<'g, 'db, I> {
    pub fn next(&mut self) -> Option<TokenTreeElement> {
        if self.is_end() {
            None
        } else {
            self.iterator.next()
        }
    }

    pub fn peek(&mut self) -> Option<&TokenTreeElement> {
        if self.is_end() {
            None
        } else {
            self.iterator.peek()
        }
    }

    pub fn is_end(&mut self) -> bool {
        (self.iterator.peek())
            .and_then(NodeOrToken::as_token)
            .is_none_or(|x| matches!(x.kind(), T!(')') | T!(']') | T!('}')))
    }

    pub fn take_token(&mut self, kind: SyntaxKind) -> bool {
        self.take_token_value(kind).is_some()
    }

    pub fn take_token_value(&mut self, kind: SyntaxKind) -> Option<SyntaxToken> {
        self.iterator
            .next_if_map_mut(|t| t.as_token().filter(|token| token.kind() == kind).cloned())
    }

    pub fn next_expr(&mut self) -> Option<ast::Expr> {
        let first = self.next()?;
        let last = (self.iterator)
            .peeking_take_while(|x| {
                x.as_token().is_none_or(|t| {
                    !matches!(
                        t.kind(),
                        T!(=>) | T!(,) | T!(;) | T!(')') | T!(']') | T!('}')
                    )
                })
            })
            .last();
        let first_token = first_token(first.clone());
        let last_token = last.map(last_token);
        let first_descenders = self.cg.eir_sem.descend_into_macros(first_token.clone());
        let last_descenders = last_token
            .clone()
            .map(|token| self.cg.eir_sem.descend_into_macros(token));
        let last_descenders = last_descenders.as_ref().unwrap_or(&first_descenders);
        Some(
            (first_descenders.iter())
                .find_map(|tok| {
                    tok.parent_ancestors()
                        .filter(|x| last_descenders.contains(&x.last_token().unwrap()))
                        .find_map(ast::Expr::cast)
                })
                .unwrap_or_else(|| {
                    panic!(
                        "failed to resolve expr {}, {first:?}",
                        self.cg.node_location_ast(&first_token.parent().unwrap()),
                    )
                }),
        )
    }
}

impl<'g, 'db, I: Iterator<Item = TokenTreeElement>> MacroParser<'g, 'db, I>
where
    Self: Clone,
{
    pub fn take_look_ahead<F, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> Option<R>,
    {
        let backup = self.clone();
        f(self).or_else(|| {
            *self = backup;
            None
        })
    }
}

fn first_token(element: TokenTreeElement) -> SyntaxToken {
    match element {
        NodeOrToken::Node(n) => n.syntax().first_token().unwrap(),
        NodeOrToken::Token(t) => t,
    }
}

fn last_token(element: TokenTreeElement) -> SyntaxToken {
    match element {
        NodeOrToken::Node(n) => n.syntax().last_token().unwrap(),
        NodeOrToken::Token(t) => t,
    }
}

#[derive(Debug)]
enum FormatSegment<'a> {
    Literal(&'a str),
    Braces(char),
    Placeholder(Either<u64, &'a str>, &'a str),
}

fn parse_rest_as_format_args(
    ctx: &LowerToEirCtx,
    macro_call: &ast::MacroCall,
    parser: &mut MacroParser<impl Iterator<Item = TokenTreeElement> + Clone>,
) -> eir::FormatArgsExpr {
    let format = if let ast::Expr::Literal(format_literal) = parser.next_expr().unwrap()
        && let ast::LiteralKind::String(s) = format_literal.kind()
        && let Ok(format) = s.value()
    {
        format.into_owned()
    } else {
        panic!(
            "Expected format_args!() first argument to be a literal at {}",
            ctx.cg.expr_location_ast(macro_call)
        )
    };

    let (exprs, named_exprs) = parse_format_args_params(ctx, macro_call, parser);

    let Some(segments) = parse_format_string(&format) else {
        panic!(
            "Invalid format string at {}",
            ctx.cg.expr_location_ast(macro_call)
        )
    };

    emit_format_args_to_string(ctx, macro_call, &segments, exprs, named_exprs)
}

fn parse_format_string(format: &str) -> Option<Vec<FormatSegment<'_>>> {
    let mut rest = format;
    let mut segments = vec![];
    let mut unnamed_index = 0;

    while let Some(position) = rest.find(['{', '}']) {
        if position != 0 {
            segments.push(FormatSegment::Literal(&rest[..position]));
            rest = &rest[position..];
        }
        // '{{' or '}}' => it's escaped
        if rest.as_bytes().get(1) == Some(&rest.as_bytes()[0]) {
            segments.push(FormatSegment::Braces(rest.as_bytes()[0] as char));
            rest = &rest[2..];
            continue;
        }
        if rest.as_bytes()[0] == b'}' {
            return None;
        }

        let colon_or_bracket = rest.find(['}', ':'])?;

        let variable = &rest[1..colon_or_bracket];
        let format_string;
        if rest.as_bytes()[colon_or_bracket] == b'}' {
            format_string = "";
            rest = &rest[colon_or_bracket + 1..];
        } else {
            let bracket = rest.find(['}'])?;
            format_string = &rest[(colon_or_bracket + 1)..bracket];
            rest = &rest[bracket + 1..];
        }

        if let Ok(num) = u64::from_str(variable) {
            segments.push(FormatSegment::Placeholder(Either::Left(num), format_string));
        } else if variable.is_empty() {
            segments.push(FormatSegment::Placeholder(
                Either::Left(unnamed_index),
                format_string,
            ));
            unnamed_index += 1;
        } else {
            segments.push(FormatSegment::Placeholder(
                Either::Right(variable),
                format_string,
            ));
        }
    }
    if !rest.is_empty() {
        segments.push(FormatSegment::Literal(rest));
    }

    Some(segments)
}

fn parse_format_args_params(
    ctx: &LowerToEirCtx,
    macro_call: &ast::MacroCall,
    parser: &mut MacroParser<impl Iterator<Item = TokenTreeElement> + Clone>,
) -> (Vec<eir::Expr>, FxHashMap<String, eir::Expr>) {
    let mut exprs = vec![];
    let mut named_exprs = FxHashMap::default();

    while parser.take_token(T![,]) && !parser.is_end() {
        if let Some(ident) = parser.take_look_ahead(|p| {
            if let Some(ident) = p.take_token_value(T![ident])
                && let Some(_eq) = p.take_token_value(T![=])
            {
                Some(ident)
            } else {
                None
            }
        }) {
            // ident =
            let Some(expr) = parser.next_expr() else {
                panic!(
                    "expected expression but not after '{ident} =' at {}",
                    ctx.cg.expr_location_ast(macro_call)
                );
            };

            named_exprs.insert(ident.text().to_owned(), ctx.lower(expr));
        } else {
            // simple expr
            let token = { parser.clone() }.next();
            let Some(expr) = parser.next_expr() else {
                panic!(
                    "expected expression but not at {} ({token:?})",
                    ctx.cg.expr_location_ast(macro_call)
                );
            };
            exprs.push(ctx.lower(expr));
        }
    }

    (exprs, named_exprs)
}

fn emit_format_args_to_string(
    ctx: &LowerToEirCtx,
    macro_call: &ast::MacroCall,
    segments: &[FormatSegment],
    exprs: Vec<eir::Expr>,
    named_exprs: FxHashMap<String, eir::Expr>,
) -> eir::FormatArgsExpr {
    let mut result = vec![];
    for segment in segments {
        match segment {
            FormatSegment::Literal(literal) => {
                result.push(eir::FormatArgsSegment::Literal(literal.to_string()));
            }
            &FormatSegment::Braces(c) => {
                // C# also uses '{' '}'
                result.push(eir::FormatArgsSegment::Braces(c));
            }
            &FormatSegment::Placeholder(var, f) => {
                // display
                let expr = match var {
                    Either::Left(i) => exprs[i as usize].clone(),
                    Either::Right(name) => named_exprs.get(name).cloned().unwrap_or_else(|| {
                        From::from(new_eir_node!(eir::PathExpr {
                            path: eir::Path::ScopedName {
                                scope: macro_call.syntax().clone(),
                                name: name.to_string(),
                            },
                            node_info: eir::NodeInfo::None,
                        }))
                    }),
                };

                let formatter = match f {
                    "" => "DisplayStr",
                    "?" => "DebugStr",
                    _ => panic!(
                        "Unsupported format specifier: {f} at {}",
                        ctx.cg.expr_location_ast(macro_call)
                    ),
                };

                let _ = formatter;
                result.push(eir::FormatArgsSegment::Expr(From::from(new_eir_node!(
                    eir::MethodCallExpr {
                        receiver: expr.clone(),
                        name_ref: eir::NameRef::CSharp(formatter.to_string()),
                        arg_list: new_eir_node!(eir::ArgList {
                            args: eir_children![]
                        }),
                        node_info: eir::NodeInfo::None,
                    }
                ))));
            }
        }
    }

    new_eir_node!(eir::FormatArgsExpr {
        segments: result.into(),
        node_info: eir::NodeInfo::None,
    })
}
