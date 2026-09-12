use crate::codegen::CodeGenerator;
use crate::codegen::body::eir;
use crate::codegen::body::eir::EirNode;
use crate::new_eir_node;

trait ReplaceDefault {
    fn replace_default() -> Self;
}

impl ReplaceDefault for eir::Expr {
    fn replace_default() -> Self {
        eir::Expr::from(new_eir_node!(eir::RawCodeExpr {
            code: code!(""),
            divergent: false,
            node_info: eir::NodeInfo::None
        }))
    }
}

impl ReplaceDefault for eir::Pat {
    fn replace_default() -> Self {
        eir::Pat::from(new_eir_node!(eir::WildcardPat {
            node_info: eir::NodeInfo::None
        }))
    }
}

impl ReplaceDefault for eir::Stmt {
    fn replace_default() -> Self {
        eir::Stmt::from(new_eir_node!(eir::ExprStmt {
            expr: eir::Expr::from(new_eir_node!(eir::RawCodeExpr {
                code: code!(""),
                divergent: false,
                node_info: eir::NodeInfo::None,
            })),
            node_info: eir::NodeInfo::None,
        }))
    }
}

macro_rules! replace_node {
    ($expr: ident as $casted: pat = $inner: expr) => {{
        #[allow(irrefutable_let_patterns)]
        let $casted = std::mem::replace(
            $expr,
            crate::codegen::body::transformer::ReplaceDefault::replace_default(),
        ) else {
            panic!("")
        };
        *$expr = $inner;
    }};
}

macro_rules! def_transformer {
    (
        $(#[$meta: meta])*
        $vis: vis struct $name: ident;
    ) => {
        $(#[$meta])*
        $vis struct $name<'a, 'db> {
            cg: &'a crate::codegen::CodeGenerator<'db>,
        }

        impl<'g, 'db> std::ops::Deref for $name<'g, 'db> {
            type Target = crate::codegen::CodeGenerator<'db>;

            fn deref(&self) -> &Self::Target {
                self.cg
            }
        }

        impl<'g, 'db> $name<'g, 'db> {
            pub fn new(cg: &'g crate::codegen::CodeGenerator<'db>) -> Self {
                Self { cg }
            }
        }

    };
}

mod expand_macro_like_functions;
mod implicit_returns;

pub fn transform(cg: &CodeGenerator, node: &mut impl EirNode) {
    macro_rules! accept {
        ($($path: ident)::+) => {
            let std::ops::ControlFlow::Continue(()) = node.accept_mut(&mut $($path)::+::new(cg));
        };
    }

    accept!(implicit_returns::ImplicitReturns);
    accept!(expand_macro_like_functions::ExpandMacroLikeFunctions);
}
