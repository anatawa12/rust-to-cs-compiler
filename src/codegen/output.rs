/// Indented C# code output buffer.

pub struct Output {
    buf: String,
    indent: usize,
    at_line_start: bool,
}

impl Output {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            indent: 0,
            at_line_start: true,
        }
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    /// Write a string without a newline. Inserts indentation at the start of a new line.
    pub fn write(&mut self, s: &str) {
        for ch in s.chars() {
            if self.at_line_start && ch != '\n' {
                for _ in 0..self.indent {
                    self.buf.push_str("    ");
                }
                self.at_line_start = false;
            }
            self.buf.push(ch);
            if ch == '\n' {
                self.at_line_start = true;
            }
        }
    }

    pub fn writeln(&mut self, s: &str) {
        self.write(s);
        self.write("\n");
    }

    pub fn blank_line(&mut self) {
        // Avoid consecutive blank lines.
        if !self.buf.ends_with("\n\n") {
            self.buf.push('\n');
        }
    }

    pub fn open_brace(&mut self) {
        self.writeln("{");
        self.indent();
    }

    pub fn close_brace(&mut self) {
        self.dedent();
        self.writeln("}");
    }

    pub fn finish(self) -> String {
        self.buf
    }
}
