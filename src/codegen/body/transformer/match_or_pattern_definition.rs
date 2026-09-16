use crate::codegen::body::eir::*;
use crate::codegen::body::transformer::take_eir;
use std::ops::ControlFlow;

def_transformer!(
    /// This transformer transforms or patterns to multiple match branches if each or hand
    /// has some variable declaration.
    ///
    /// This fixes the problem that C# doesn't support variable declaration in or patterns,
    /// even when all of the or hands have some variable declaration.
    pub struct MatchOrPatternDefinitions;
);

impl MutatingEirVisitor for MatchOrPatternDefinitions<'_, '_> {
    type Break = std::convert::Infallible;

    fn visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        let Expr::MatchExpr(match_expr) = expr else {
            return expr.accept_children_mut(self);
        };
        let match_expr = match_expr.as_mut();

        let arms = &mut match_expr.match_arm_list.as_mut().arms;
        replace_node!(
            arms as vec = {
                vec.into_iter()
                    .flat_map(|mut arm| {
                        let pat = take_eir(&mut arm.as_mut().pat);
                        if let Some(patterns) = expand_or_pattern(self, &pat).0 {
                            patterns
                                .into_iter()
                                .map(|pat| {
                                    let mut arm = arm.clone();
                                    arm.as_mut().pat = pat;
                                    arm
                                })
                                .collect()
                        } else {
                            arm.as_mut().pat = pat;
                            vec![arm]
                        }
                    })
                    .collect::<Vec<_>>()
                    .into()
            }
        );

        expr.accept_children_mut(self)
    }
}

macro_rules! process_child {
    (
        visitor = $visitor:expr,
        Pat::$Variant:ident($variant:ident) = $pat:expr,
        $(has_local = $has_local:expr,)?
        children = ( $( $childs:tt )* ) $(,)?
    ) => { {
        #[allow(unused_mut)] // macros
        let mut has_local = false;
        #[allow(unused_mut)] // macros
        let mut result: Option<Vec<Pat>> = None;

        $(has_local |= $has_local;)?

        #[allow(unused_variables)] // macros
        let child = $variant;

        process_child!(
            @children_entry
            [
                [$visitor]
                [result]
                [Pat::$Variant($variant) = $pat]
                [has_local]
            ]
            [child]
            [ $( $childs )* ]
        );

        /*
        if let Some(child) = child.pat() {
            let (expanded, has_local_child) = expand_or_pattern($visitor, child);
            has_local |= has_local_child;

            if let Some(expanded) = expanded {
                let result = result.get_or_insert_with(|| {
                    let mut vec = Vec::with_capacity(expanded.len());
                    vec.push(pat.clone());
                    vec
                });

                // We do cross-join.
                result.reserve(result.len() * (expanded.len() - 1));
                for i in 0..result.len() {
                    let pat = take_eir(&mut result[i]);

                    let mut is_first = true;
                    for expanded in &expanded {
                        let mut new_pat = pat.clone();
                        let Pat::$Variant($variant) = &mut new_pat else {
                            unreachable!()
                        };

                        let child = $variant;
                        let child = &mut child.as_mut().pat;

                        *child = Some(expanded.clone());
                        if is_first {
                            result[i] = new_pat;
                            is_first = false;
                        } else {
                            result.push(new_pat)
                        }
                    }
                }
            }
        }
         */

        (result, has_local)
    }};

    (@children_entry
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        []
    ) => {
    };
    (@children_entry
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        [ $( $unused:ident $(. $field:ident $([$index:ident])? $(? $($optional:ident)?)? )* ),* $(,)? ]
    ) => {
        process_child!(
            @children_entry
            [ $($descend_pass_through)* ]
            [ $child ]
            [ $(($($field $([$index])? $(? $($optional)?)? .)*))* ]
        );
    };
    (@children_entry
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        [ ($($field:tt)*) $($rest:tt)* ]
    ) => {
        process_child!(
            @descend
            [
                $($descend_pass_through)*
                [ $($field)* ]
            ]
            [ $child ]
            [ $($field)* ]
        );
        process_child!(
            @children_entry
            [ $($descend_pass_through)* ]
            [ $child ]
            [ $($rest)* ]
        );
    };

    (@descend
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        [ $field:ident [$index:ident] . $($fields_rest:tt)* ]
    ) => {
        for ($index, $child) in $child.$field().enumerate() {
            process_child!(
                @descend
                [ $($descend_pass_through)* ]
                [ $child ]
                [ $($fields_rest)* ]
            );
        }
    };
    (@descend
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        [ $field:ident ? . $($fields_rest:tt)* ]
    ) => {
        if let Some($child) = $child.$field() {
            process_child!(
                @descend
                [ $($descend_pass_through)* ]
                [ $child ]
                [ $($fields_rest)* ]
            );
        }
    };
    (@descend
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        [ $field:ident . $($fields_rest:tt)* ]
    ) => {
        let $child = $child.$field();
        process_child!(
            @descend
            [ $($descend_pass_through)* ]
            [ $child ]
            [ $($fields_rest)* ]
        );
    };
    (@descend
        [ $($descend_pass_through:tt)* ]
        [ $child:ident ]
        [ . $field:ident $([$index:ident])? $(? $($optional:ident)?)? $($fields_rest:tt)* ]
    ) => {
        process_child!(
            @descend
            [ $($descend_pass_through)* ]
            [ $child ]
            [ $($fields_rest)* ]
        );
    };
    (@descend
        [
            [$visitor:expr]
            [$result:expr]
            [Pat::$Variant:ident($variant:ident) = $pat:expr]
            [$has_local:ident]
            [ $( $field:ident $([$index:ident])? $(? $($optional:ident)?)? . )* ]
        ]
        [ $child:ident ]
        [  ]
    ) => {
        let (expanded, has_local_child) = expand_or_pattern($visitor, $child);
        $has_local |= has_local_child;

        if let Some(expanded) = expanded {
            let result = $result.get_or_insert_with(|| {
                let mut vec = Vec::with_capacity(expanded.len());
                vec.push($pat.clone());
                vec
            });

            // We do cross-join.
            result.reserve(result.len() * (expanded.len() - 1));
            for i in 0..result.len() {
                let pat = take_eir(&mut result[i]);

                let mut is_first = true;
                for expanded in &expanded {
                    let mut new_pat = pat.clone();
                    let Pat::$Variant($variant) = &mut new_pat else {
                        unreachable!()
                    };

                    let child = $variant;
                    $( process_child!(@descend_mut child $field $([$index])? $(? $($optional)?)?); )*

                    *child = expanded.clone();
                    if is_first {
                        result[i] = new_pat;
                        is_first = false;
                    } else {
                        result.push(new_pat)
                    }
                }
            }
        }
    };

    (@descend_mut $child:ident $field:ident) => {
        let $child = &mut $child.as_mut().$field;
    };
    (@descend_mut $child:ident $field:ident [$index:ident]) => {
        let $child = &mut $child.as_mut().$field.as_mut()[$index];
    };
    (@descend_mut $child:ident $field:ident ?) => {
        let $child = $child.as_mut().$field.as_mut().unwrap();
    };
}

// bool indicates whether the pattern contains variable declaration.
// None indicates that the pattern does not need to duplicated or modified,
// Some(Vec) indicates that the pattern was duplicated because of variable declaration in or-pattern
fn expand_or_pattern(
    visitor: &mut MatchOrPatternDefinitions<'_, '_>,
    pat: &Pat,
) -> (Option<Vec<Pat>>, bool) {
    match pat {
        Pat::IdentPat(ident) => {
            process_child!(
                visitor = visitor,
                Pat::IdentPat(ident) = pat,
                has_local = visitor.eir_sem.to_def(ident).is_some(),
                children = (ident.pat?)
            )
        }
        Pat::LiteralPat(literal) => {
            process_child!(
                visitor = visitor,
                Pat::LiteralPat(literal) = pat,
                children = ()
            )
        }
        Pat::OrPat(or_pat) => {
            let mut pats = or_pat.pats();
            let first_pat = pats
                .next()
                .expect("or_pat must include two or more patterns");
            let (first_mapped, first_has_local) = expand_or_pattern(visitor, first_pat);
            if first_has_local {
                let mut result = first_mapped.unwrap_or_else(|| vec![first_pat.clone()]);

                while let Some(pat) = pats.next() {
                    let (mapped, has_local) = expand_or_pattern(visitor, pat);
                    assert!(has_local, "or pattern has_local disagree among branches");
                    if let Some(mapped) = mapped {
                        result.extend(mapped);
                    } else {
                        result.push(pat.clone());
                    }
                }

                (Some(result), true)
            } else {
                assert!(
                    first_mapped.is_none(),
                    "has_local is false but mapped is some"
                );
                while let Some(pat) = pats.next() {
                    let (mapped, has_local) = expand_or_pattern(visitor, pat);
                    assert!(!has_local, "or pattern has_local disagree among branches");
                    assert!(mapped.is_none(), "has_local is false but mapped is some");
                }
                (None, false)
            }
        }
        Pat::PathPat(path) => {
            process_child!(visitor = visitor, Pat::PathPat(path) = pat, children = ())
        }
        Pat::RangePat(range) => {
            process_child!(
                visitor = visitor,
                Pat::RangePat(range) = pat,
                children = (ident.start?, ident.end?)
            )
        }
        Pat::RecordPat(record_pat) => {
            process_child!(
                visitor = visitor,
                Pat::RecordPat(record_pat) = pat,
                children = (ident.record_pat_field_list.fields[index].pat)
            )
        }
        Pat::RefPat(ref_pat) => {
            process_child!(
                visitor = visitor,
                Pat::RefPat(ref_pat) = pat,
                children = (ident.pat)
            )
        }
        Pat::RestPat(rest) => {
            process_child!(visitor = visitor, Pat::RestPat(rest) = pat, children = ())
        }
        Pat::SlicePat(slice) => {
            process_child!(
                visitor = visitor,
                Pat::SlicePat(slice) = pat,
                children = (slice.components.prefix[index])
            )
        }
        Pat::TuplePat(tuple) => {
            process_child!(
                visitor = visitor,
                Pat::TuplePat(tuple) = pat,
                children = (slice.fields[index])
            )
        }
        Pat::TupleStructPat(tuple_struct) => {
            process_child!(
                visitor = visitor,
                Pat::TupleStructPat(tuple_struct) = pat,
                children = (slice.fields[index])
            )
        }
        Pat::WildcardPat(wildcard) => {
            process_child!(
                visitor = visitor,
                Pat::WildcardPat(wildcard) = pat,
                children = ()
            )
        }
    }
}
