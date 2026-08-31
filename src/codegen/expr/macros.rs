use super::{BodyGen, EmittedExprInfo};
use crate::codegen::output::Code;
use crate::codegen::{CodeGenerator, item_exclusion, names};
use itertools::{Either, Itertools};
use std::collections::HashMap;
use std::iter;
use std::str::FromStr;
use syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxToken, T, ast};

impl<'g, 'db> BodyGen<'g, 'db> {
    pub(super) fn emit_expr_macro(
        &self,
        expr: &ast::Expr,
        macro_call: &ast::MacroCall,
    ) -> (Code, EmittedExprInfo) {
        let Some(macro_) = self.sem.resolve_macro_call(macro_call) else {
            panic!(
                "Unresolved macro call at {}",
                self.expr_location_ast(macro_call)
            )
        };
        let tt = macro_call.token_tree().unwrap();

        if self.should_inline_macro(macro_call) {
            // Macro that expands
            let macro_call = self.sem.expand_macro_call(macro_call).unwrap().value;
            let expr =
                ast::Expr::cast(macro_call.clone()).unwrap_or_else(|| panic!("{:?}", macro_call));
            (
                self.emit_expr_str_ast(&expr),
                EmittedExprInfo::non_diverging(),
            )
        } else if Some(macro_) == self.lang_items.unreachable() {
            (
                fcode!(r#"throw new PanicException("unreachable")"#),
                EmittedExprInfo::diverging(),
            )
        } else if Some(macro_) == self.lang_items.panic() {
            (
                fcode!(r#"throw new PanicException("panic")"#),
                EmittedExprInfo::diverging(),
            )
        } else if Some(macro_) == self.lang_items.vec() {
            let ty = (self.sem.type_of_expr(expr)).unwrap_or_else(|| {
                panic!("Unresolved macro call at {}", self.expr_location_ast(expr))
            });
            let (_vec, args) = ty.original().as_adt_with_args().unwrap();
            let element_ty = { args }.swap_remove(0).unwrap();
            let mut parser = MacroParser::new(self.cg, &tt);

            if let Some(first_expr) = parser.next_expr() {
                if parser.take_token(T![;]) {
                    //let count = parser.next_expr().unwrap();
                    panic!(
                        "Unsupported vec![T; N] call at {}",
                        self.expr_location_ast(expr)
                    )
                } else if parser.take_token(T![,]) || parser.is_end() {
                    let mut code = Code::new();
                    code.w("new List<")
                        .w(self.rust_type_to_cs(&element_ty))
                        .w(">{ ");
                    code.w(self.emit_expr_str_ast(&first_expr));

                    while parser.take_token(T![,])
                        && let Some(next_expr) = parser.next_expr()
                    {
                        code.w(", ");
                        code.w(self.emit_expr_str_ast(&next_expr));
                    }

                    code.w("} ");
                    (code, EmittedExprInfo::non_diverging())
                } else {
                    panic!(
                        "Unsupported vec![] call at {}",
                        self.expr_location_ast(expr)
                    )
                }
            } else {
                (
                    fcode!("new List<{}>()", self.rust_type_to_cs(&element_ty)),
                    EmittedExprInfo::non_diverging(),
                )
            }
        } else if Some(macro_) == self.lang_items.pin() {
            let mut parser = MacroParser::new(self.cg, &tt);
            (
                self.emit_expr_str_ast(&parser.next_expr().unwrap()),
                EmittedExprInfo::non_diverging(),
            )
        } else if Some(macro_) == self.lang_items.format_args()
            || Some(macro_) == self.lang_items.format()
        {
            // NOTE: this might breaks execution order of each format args
            (
                parse_rest_as_format_args(self, macro_call, &mut MacroParser::new(self.cg, &tt)),
                EmittedExprInfo::non_diverging(),
            )
        } else if Some(macro_) == self.lang_items.log_trace() {
            log_macro("trace", self, macro_call, tt)
        } else if Some(macro_) == self.lang_items.log_debug() {
            log_macro("debug", self, macro_call, tt)
        } else if Some(macro_) == self.lang_items.log_info() {
            log_macro("info", self, macro_call, tt)
        } else if Some(macro_) == self.lang_items.log_warn() {
            log_macro("warn", self, macro_call, tt)
        } else if Some(macro_) == self.lang_items.log_error() {
            log_macro("error", self, macro_call, tt)
        } else {
            (
                fcode!(
                    "/* macro name {macro_path} */ macro_{short_name}()",
                    macro_path = macro_call.path().unwrap().syntax().text(),
                    short_name = (macro_call.path().unwrap().segments().last().unwrap())
                        .syntax()
                        .text(),
                ),
                EmittedExprInfo::non_diverging(),
            )
        }
    }

    pub(super) fn should_inline_macro(&self, macro_call: &ast::MacroCall) -> bool {
        let Some(macro_) = self.sem.resolve_macro_call(macro_call) else {
            panic!(
                "Unresolved macro call at {}",
                self.expr_location_ast(macro_call)
            )
        };
        let is_inline = item_exclusion::has_attr(macro_, "r2cs_inline", self.db);
        Some(macro_) == self.lang_items.matches()
            || Some(macro_) == self.lang_items.ready()
            || Some(macro_) == self.lang_items.write()
            || is_inline
    }
}

fn log_macro(
    level: &str,
    bg: &BodyGen,
    macro_call: &ast::MacroCall,
    tt: ast::TokenTree,
) -> (Code, EmittedExprInfo) {
    let format_string =
        parse_rest_as_format_args(bg, macro_call, &mut MacroParser::new(bg.cg, &tt));
    (
        code!("Logging.", level, "(", format_string, ")"),
        EmittedExprInfo::non_diverging(),
    )
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
        let first_descenders = self.cg.sem.descend_into_macros(first_token.clone());
        let last_descenders = last_token
            .clone()
            .map(|token| self.cg.sem.descend_into_macros(token));
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
    Escaped(char),
    Placeholder(Either<u64, &'a str>, &'a str),
}

fn parse_rest_as_format_args(
    bg: &BodyGen,
    macro_call: &ast::MacroCall,
    parser: &mut MacroParser<impl Iterator<Item = TokenTreeElement> + Clone>,
) -> Code {
    let format = if let ast::Expr::Literal(format_literal) = parser.next_expr().unwrap()
        && let ast::LiteralKind::String(s) = format_literal.kind()
        && let Ok(format) = s.value()
    {
        format.into_owned()
    } else {
        panic!(
            "Expected format_args!() first argument to be a literal at {}",
            bg.expr_location_ast(macro_call)
        )
    };

    let (exprs, named_exprs) = parse_format_args_params(bg, macro_call, parser);

    let Some(segments) = parse_format_string(&format) else {
        panic!(
            "Invalid format string at {}",
            bg.expr_location_ast(macro_call)
        )
    };

    emit_format_args_to_string(bg, macro_call, &segments, &exprs, &named_exprs)
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
            segments.push(FormatSegment::Escaped(rest.as_bytes()[0] as char));
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
    bg: &BodyGen,
    macro_call: &ast::MacroCall,
    parser: &mut MacroParser<impl Iterator<Item = TokenTreeElement> + Clone>,
) -> (Vec<ast::Expr>, HashMap<String, ast::Expr>) {
    let mut exprs = vec![];
    let mut named_exprs = HashMap::new();

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
                    bg.expr_location_ast(macro_call)
                );
            };

            named_exprs.insert(ident.text().to_owned(), expr);
        } else {
            // simple expr
            let token = { parser.clone() }.next();
            let Some(expr) = parser.next_expr() else {
                panic!(
                    "expected expression but not at {} ({token:?})",
                    bg.expr_location_ast(macro_call)
                );
            };
            exprs.push(expr);
        }
    }

    (exprs, named_exprs)
}

fn emit_format_args_to_string(
    bg: &BodyGen,
    macro_call: &ast::MacroCall,
    segments: &[FormatSegment],
    exprs: &[ast::Expr],
    named_exprs: &HashMap<String, ast::Expr>,
) -> Code {
    let sem_scope = bg.sem.scope(macro_call.syntax()).unwrap();

    let mut result = code!(r##"$""##);
    for segment in segments {
        match segment {
            FormatSegment::Literal(literal) => {
                write!(result, "{}", literal.escape_default());
            }
            FormatSegment::Escaped(c) => write!(result, "{c}{c}"), // C# also uses '{' '}'
            &FormatSegment::Placeholder(var, f) => {
                // display
                let expr = match var {
                    Either::Left(i) => Either::Left(&exprs[i as usize]),
                    Either::Right(name) => named_exprs
                        .get(name)
                        .map(Either::Left)
                        .unwrap_or(Either::Right(name)),
                };

                let formatter = match f {
                    "" => "DisplayStr",
                    "?" => "DebugStr",
                    _ => panic!(
                        "Unsupported format specifier: {f} at {}",
                        bg.expr_location_ast(macro_call)
                    ),
                };

                match expr {
                    Either::Left(expr) => {
                        result
                            .w("{")
                            .w(bg.emit_expr_str_ast(expr))
                            .w(".")
                            .w(formatter)
                            .w("()}");
                    }
                    Either::Right(name) => {
                        let mut resolved = None;
                        sem_scope.process_all_names(&mut |cur_name, def| {
                            if cur_name.as_str() == name {
                                resolved.get_or_insert(def);
                            }
                        });
                        let Some(resolved) = resolved else {
                            panic!(
                                "Unresolved format param name {name} at {}",
                                bg.expr_location_ast(macro_call)
                            );
                        };
                        let expr = match resolved {
                            hir::ScopeDef::Local(l) => bg.binding_name_ast(l),
                            hir::ScopeDef::ModuleDef(hir::ModuleDef::Const(const_)) => {
                                bg.const_path_cs(const_)
                            }
                            hir::ScopeDef::ModuleDef(hir::ModuleDef::Static(static_)) => {
                                let mut path = bg.module_class_cs(static_.module(bg.db));
                                path.push('.');
                                path.push_str(&names::static_name(static_.name(bg.db).as_str()));
                                path
                            }
                            _ => {
                                sem_scope.speculative_resolve(&ast::make::path_from_text(name));
                                panic!(
                                    "Unsupported path in format: {name} at {} ({resolved:?})",
                                    bg.expr_location_ast(macro_call),
                                )
                            }
                        };
                        result.w("{").w(expr).w(".").w(formatter).w("()}");
                    }
                }
            }
        }
    }

    result.w("\"");

    result
}
