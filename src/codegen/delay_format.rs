use rustc_type_ir::inherent::SliceLike;

enum Part<T> {
    String(String),
    Data(T),
    Join(Vec<T>, String),
}

pub struct DelayedFormatString<T> {
    parts: Vec<Part<T>>,
}

impl<T> DelayedFormatString<T> {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn push(&mut self, part: T) {
        self.parts.push(Part::Data(part));
    }

    pub fn push_str(&mut self, part: &str) {
        self.parts.push(Part::String(part.to_owned()));
    }

    pub fn push_join(&mut self, collection: Vec<T>, part: &str) {
        self.parts.push(Part::Join(collection, part.to_owned()));
    }

    pub fn format(&self, f: impl Fn(&T) -> String) -> String {
        let mut result = String::new();

        for part in &self.parts {
            match *part {
                Part::String(ref s) => result.push_str(s),
                Part::Data(ref d) => result.push_str(&f(d)),
                Part::Join(ref collection, ref sep) => {
                    let mut iterator = collection.iter();
                    if let Some(value) = iterator.next() {
                        result.push_str(&f(value));
                        while let Some(value) = iterator.next() {
                            result.push_str(sep);
                            result.push_str(&f(value));
                        }
                    }
                }
            }
        }

        result
    }
}

macro_rules! delayed_format {
    ($(,)?) => {
        $crate::codegen::delay_format::DelayedFormatString::new()
    };

    (@push [$ident: ident] []) => {
    };
    (@push [$ident: ident] [$literal: literal]) => {{
        $ident.push_str($literal);
    }};
    (@push [$ident: ident] [$literal: literal, $($rest: tt)*]) => {{
        $ident.push_str($literal);
        delayed_format!(@push [$ident] [$($rest)*]);
    }};
    (@push [$ident: ident] [str($expr: expr)]) => {{
        $ident.push_str($expr);
    }};
    (@push [$ident: ident] [str($expr: expr), $($rest: tt)*]) => {{
        $ident.push_str($expr);
        delayed_format!(@push [$ident] [$($rest)*]);
    }};
    (@push [$ident: ident] [join($expr: expr, $sep: expr)]) => {{
        $ident.push_join($expr, $sep);
    }};
    (@push [$ident: ident] [join($expr: expr, $sep: expr), $($rest: tt)*]) => {{
        $ident.push_join($expr, $sep);
        delayed_format!(@push [$ident] [$($rest)*]);
    }};
    (@push [$ident: ident] [$expr: expr]) => {{
        $ident.push($expr);
    }};
    (@push [$ident: ident] [$expr: expr, $($rest: tt)*]) => {{
        $ident.push($expr);
        delayed_format!(@push [$ident] [$($rest)*]);
    }};

    (@$($tt:tt)*) => {
        compile_error!("");
    };

    ($($tt:tt)*) => {{
        let mut string = $crate::codegen::delay_format::DelayedFormatString::new();
        delayed_format!(@push [string] [$($tt)*]);
        string
    }}
}
