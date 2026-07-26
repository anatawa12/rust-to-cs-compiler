use super::BodyGen;
use crate::codegen::CodeGenerator;
use crate::codegen::output::Code;
use itertools::Itertools;
use std::iter;
use syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxToken, T, ast};

impl<'g, 'db> BodyGen<'g, 'db> {
    pub(super) fn emit_expr_macro(&self, expr: &ast::Expr, macro_call: &ast::MacroCall) -> Code {
        let path = macro_call.path().unwrap();
        let Some(macro_) = self.sem.resolve_macro_call(macro_call) else {
            panic!(
                "Unresolved macro call at {}",
                self.expr_location_ast(macro_call)
            )
        };
        let tt = macro_call.token_tree().unwrap();

        if Some(macro_) == self.lang_items.matches() || Some(macro_) == self.lang_items.ready() {
            // Macro that expands
            let macro_call = self.sem.expand_macro_call(macro_call).unwrap().value;
            let expr = ast::MacroStmts::cast(macro_call.clone())
                .and_then(|x| x.expr())
                .or_else(|| ast::Expr::cast(macro_call.clone()))
                .unwrap_or_else(|| panic!("{:?}", macro_call));
            self.emit_expr_str_ast(&expr)
        } else if Some(macro_) == self.lang_items.unreachable() {
            fcode!(r#"throw new PanicException("unreachable")"#)
        } else if Some(macro_) == self.lang_items.panic() {
            fcode!(r#"throw new PanicException("panic")"#)
        } else if Some(macro_) == self.lang_items.vec() {
            let ty = (self.sem.type_of_expr(expr)).unwrap_or_else(|| {
                panic!("Unresolved macro call at {}", self.expr_location_ast(expr))
            });
            let (_vec, args) = ty.original().as_adt_with_args().unwrap();
            let element_ty = { args }.swap_remove(0).unwrap();
            let mut parser = MacroParser::new(self.cg, &tt);

            if let Some(first_expr) = parser.next_expr() {
                if parser.take_token(T![;]) {
                    let count = parser.next_expr().unwrap();
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
                    code
                } else {
                    panic!(
                        "Unsupported vec![] call at {}",
                        self.expr_location_ast(expr)
                    )
                }
            } else {
                fcode!("new List<{}>()", self.rust_type_to_cs(&element_ty))
            }
        } else {
            fcode!(
                "/* macro name {macro_path} */ macro_{short_name}()",
                macro_path = macro_call.path().unwrap().syntax().text(),
                short_name = (macro_call.path().unwrap().segments().last().unwrap())
                    .syntax()
                    .text(),
            )
        }
    }
}

type TokenTreeElement = NodeOrToken<ast::TokenTree, SyntaxToken>;

struct MacroParser<'g, 'db, I: Iterator<Item = TokenTreeElement>> {
    cg: &'g CodeGenerator<'db>,
    surrounding: SyntaxToken,
    iterator: iter::Peekable<I>,
}

impl<'g, 'db> MacroParser<'g, 'db, std::vec::IntoIter<TokenTreeElement>> {
    pub fn new(
        cg: &'g CodeGenerator<'db>,
        token: &ast::TokenTree,
    ) -> MacroParser<'g, 'db, impl Iterator<Item = TokenTreeElement>> {
        let mut iterator = token.token_trees_and_tokens().peekable();
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
        self.iterator
            .next_if(|t| t.as_token().is_some_and(|token| token.kind() == kind))
            .is_some()
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
        let first_token = first_token(first);
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
                        "failed to resolve expr {}",
                        self.cg.node_location_ast(&first_token.parent().unwrap()),
                    )
                }),
        )
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
