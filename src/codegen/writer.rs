/// A simple indented C# source-code writer.
///
/// All generated code goes through `CsWriter` so that indentation is always
/// consistent and the caller never has to manage whitespace manually.
pub struct CsWriter {
    output: String,
    indent_level: usize,
    indent_str: &'static str,
    /// True when the current line is empty (nothing has been written since the
    /// last newline).  Used to inject the indent prefix lazily.
    at_line_start: bool,
}

impl CsWriter {
    /// Create a new writer with 4-space indentation.
    pub fn new() -> Self {
        CsWriter {
            output: String::new(),
            indent_level: 0,
            indent_str: "    ",
            at_line_start: true,
        }
    }

    /// Increase the indentation level by one step.
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease the indentation level by one step.
    ///
    /// # Panics
    /// Panics if `dedent()` is called more times than `indent()`.
    pub fn dedent(&mut self) {
        assert!(self.indent_level > 0, "CsWriter: dedent underflow");
        self.indent_level -= 1;
    }

    /// Write `text` directly (no trailing newline, no automatic indentation).
    pub fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.at_line_start {
            for _ in 0..self.indent_level {
                self.output.push_str(self.indent_str);
            }
            self.at_line_start = false;
        }
        self.output.push_str(text);
    }

    /// Write `text` followed by a newline.
    pub fn write_line(&mut self, line: &str) {
        self.write(line);
        self.output.push('\n');
        self.at_line_start = true;
    }

    /// Write an empty line (just a newline).
    pub fn blank_line(&mut self) {
        self.output.push('\n');
        self.at_line_start = true;
    }

    /// Write `{` on its own line and increase indentation.
    pub fn open_brace(&mut self) {
        self.write_line("{");
        self.indent();
    }

    /// Decrease indentation and write `}` on its own line.
    pub fn close_brace(&mut self) {
        self.dedent();
        self.write_line("}");
    }

    /// Write a single-line comment.
    pub fn write_comment(&mut self, comment: &str) {
        self.write_line(&format!("// {comment}"));
    }

    /// Write a `/// <summary>…</summary>` XML-doc comment.
    pub fn write_xml_doc(&mut self, summary: &str) {
        self.write_line("/// <summary>");
        for line in summary.lines() {
            self.write_line(&format!("/// {line}"));
        }
        self.write_line("/// </summary>");
    }

    /// Consume the writer and return the generated source.
    pub fn finish(self) -> String {
        self.output
    }

    /// Return the current output as a `&str` without consuming the writer.
    pub fn as_str(&self) -> &str {
        &self.output
    }
}

impl Default for CsWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_writer_produces_empty_string() {
        let w = CsWriter::new();
        assert_eq!(w.finish(), "");
    }

    #[test]
    fn write_line_adds_newline() {
        let mut w = CsWriter::new();
        w.write_line("hello");
        assert_eq!(w.finish(), "hello\n");
    }

    #[test]
    fn indent_adds_spaces() {
        let mut w = CsWriter::new();
        w.indent();
        w.write_line("indented");
        assert_eq!(w.finish(), "    indented\n");
    }

    #[test]
    fn dedent_removes_spaces() {
        let mut w = CsWriter::new();
        w.indent();
        w.indent();
        w.write_line("deep");
        w.dedent();
        w.write_line("shallow");
        assert_eq!(w.finish(), "        deep\n    shallow\n");
    }

    #[test]
    fn open_close_brace_indents() {
        let mut w = CsWriter::new();
        w.write_line("class Foo");
        w.open_brace();
        w.write_line("int x;");
        w.close_brace();
        assert_eq!(w.finish(), "class Foo\n{\n    int x;\n}\n");
    }

    #[test]
    fn blank_line_inserts_empty_line() {
        let mut w = CsWriter::new();
        w.write_line("a");
        w.blank_line();
        w.write_line("b");
        assert_eq!(w.finish(), "a\n\nb\n");
    }

    #[test]
    fn write_without_newline_no_double_indent() {
        let mut w = CsWriter::new();
        w.indent();
        w.write("int");
        w.write(" x");
        w.write_line(";");
        assert_eq!(w.finish(), "    int x;\n");
    }

    #[test]
    #[should_panic(expected = "dedent underflow")]
    fn dedent_underflow_panics() {
        let mut w = CsWriter::new();
        w.dedent();
    }

    #[test]
    fn write_comment_format() {
        let mut w = CsWriter::new();
        w.write_comment("hello world");
        assert_eq!(w.finish(), "// hello world\n");
    }

    #[test]
    fn write_xml_doc_format() {
        let mut w = CsWriter::new();
        w.write_xml_doc("A class.");
        assert_eq!(w.finish(), "/// <summary>\n/// A class.\n/// </summary>\n");
    }
}
