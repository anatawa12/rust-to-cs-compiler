/// Indented C# code output buffer.

#[derive(Clone, Default, Eq, PartialEq)]
pub struct Code {
    buf: String,
    indent: usize,
}

impl Code {
    const INC_INDENT: u8 = 0x11; // DEVICE CONTROL ONE
    const DEC_INDENT: u8 = 0x12; // DEVICE CONTROL TWO

    fn special(c: &char) -> bool {
        *c == Self::INC_INDENT as char || *c == Self::DEC_INDENT as char
    }

    pub fn new() -> Self {
        Self {
            buf: String::new(),
            indent: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn indent(&mut self) {
        assert_eq!(self.buf.chars().rfind(|x| !Self::special(x)), Some('\n'));
        self.buf.push(Self::INC_INDENT as char);
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        assert_eq!(self.buf.chars().rfind(|x| !Self::special(x)), Some('\n'));
        self.buf.push(Self::DEC_INDENT as char);
        self.indent = self.indent.saturating_sub(1);
    }

    /// Write a string without a newline. Inserts indentation at the start of a new line.
    pub fn w(&mut self, s: impl WriteToCode) -> &mut Self {
        s.append(self);
        self
    }

    pub fn wln(&mut self, s: impl WriteToCode) -> &mut Self {
        self.w(s);
        self.w("\n")
    }

    fn write_str(&mut self, s: &str) -> &mut Self {
        self.buf.push_str(s);
        self
    }

    fn write_inner(&mut self, s: &Self) -> &mut Self {
        assert_eq!(s.indent, 0);
        self.buf.push_str(&s.buf);
        self
    }

    pub fn blank_line(&mut self) {
        // Avoid consecutive blank lines.
        if !self.buf.ends_with("\n\n") {
            self.buf.push('\n');
        }
    }

    pub fn open_brace(&mut self) {
        self.wln("{");
        self.indent();
    }

    pub fn close_brace(&mut self) {
        self.dedent();
        self.wln("}");
    }

    pub fn finish(self) -> String {
        let mut result = String::new();

        let mut buf = self.buf.as_str();

        let mut indent = 0;
        while !buf.is_empty() {
            let line;
            (line, buf) = buf.split_once('\n').unwrap_or((buf, ""));
            if !line.is_empty() {
                for _ in 0..indent {
                    result.push_str("    ");
                }
                result.push_str(line);
            }
            result.push('\n');
            loop {
                match buf.as_bytes().first() {
                    Some(&Self::INC_INDENT) => indent += 1,
                    Some(&Self::DEC_INDENT) => indent -= 1,
                    _ => break,
                }
                buf = &buf[1..];
            }
        }

        result
    }

    pub fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) {
        std::fmt::Write::write_fmt(&mut self.buf, args).unwrap();
    }
}

impl std::fmt::Write for Code {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.w(s);
        Ok(())
    }
}

pub trait WriteToCode {
    fn append(&self, out: &mut Code);
}

impl<T: WriteToCode> WriteToCode for Option<T> {
    fn append(&self, out: &mut Code) {
        if let Some(value) = self {
            value.append(out);
        }
    }
}

impl<T: WriteToCode + ?Sized> WriteToCode for &T {
    fn append(&self, out: &mut Code) {
        (*self).append(out);
    }
}

impl WriteToCode for String {
    fn append(&self, out: &mut Code) {
        self.as_str().append(out);
    }
}

impl WriteToCode for str {
    fn append(&self, out: &mut Code) {
        out.write_str(self);
    }
}

impl WriteToCode for char {
    fn append(&self, out: &mut Code) {
        (*(*self).encode_utf8(&mut [0; 4])).append(out)
    }
}

impl WriteToCode for Code {
    fn append(&self, out: &mut Code) {
        out.write_inner(self);
    }
}

macro_rules! to_string_to_code {
    ($($ty: ty),*) => {
        $(impl WriteToCode for $ty {
            fn append(&self, out: &mut Code) {
                out.w(self.to_string());
            }
        })*
    };
}

to_string_to_code!(u8, u16, u32, u64, usize);

impl From<&str> for Code {
    fn from(value: &str) -> Self {
        let mut result = Self::new();
        result.w(value);
        result
    }
}

impl From<String> for Code {
    fn from(value: String) -> Self {
        let mut result = Self::new();
        result.w(&value);
        result
    }
}

macro_rules! fcode {
    ($($arg:tt)*) => {
        $crate::codegen::output::Code::from(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! code {
    (@args [$output: expr] []) => {
    };

    (@args [$output: expr] [join($iter:expr, $sep: expr) $(, $($rem:tt)*)?]) => {
        {match ::core::iter::IntoIterator::into_iter($iter) { mut it => {
            let sep = $sep;
            if let Some(first) = it.next() {
                $output.w(first);
                while let Some(second) = it.next() {
                    $output.w(&sep);
                    $output.w(&second);
                }
            }
        }}}

        $crate::codegen::output::code!(@args [$output] [$($($rem)*)?]);
    };

    (@args [$output: expr] [format($($f:tt)*) $(, $($rem:tt)*)?]) => {
        $output.w(format!($($f)*));

        $crate::codegen::output::code!(@args [$output] [$($($rem)*)?]);
    };

    (@args [$output: expr] [indent $(, $($rem:tt)*)?]) => {
        $output.indent();
        $crate::codegen::output::code!(@args [$output] [$($($rem)*)?]);
    };

    (@args [$output: expr] [dedent $(, $($rem:tt)*)?]) => {
        $output.dedent();
        $crate::codegen::output::code!(@args [$output] [$($($rem)*)?]);
    };

    (@args [$output: expr] [$arg:expr $(, $($rem:tt)*)?]) => {
        $output.w(&$arg);
        $crate::codegen::output::code!(@args [$output] [$($($rem)*)?]);
    };

    ($($arg:tt)*) => {
        {
            let mut output = $crate::codegen::output::Code::new();
            $crate::codegen::output::code!(@args [output] [$($arg)*]);
            output
        }
    };
}

pub(crate) use code;
