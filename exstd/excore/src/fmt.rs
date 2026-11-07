pub trait Write {
    fn write_str(&mut self, s: &str) -> Result;
}

pub trait Debug {
    #[cfg_attr(r2cs, r2cs_name_override = "DebugFmt")]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

#[derive(Copy, Clone)]
pub struct Error;

pub type Result = std::result::Result<(), Error>;

pub struct Formatter<'a> {
    write: &'a mut dyn Write,
}

impl<'a> Formatter<'a> {
    pub fn write_str(&mut self, s: &str) -> Result {
        self.write.write_str(s)
    }

    pub fn debug_struct<'b>(&'b mut self, name: &str) -> DebugStruct<'a, 'b> {
        DebugStruct {
            result: self.write_str(name),
            fmt: self,
            has_fields: false,
        }
    }
}

pub struct DebugStruct<'a, 'b> {
    fmt: &'b mut Formatter<'a>,
    result: Result,
    has_fields: bool,
}

impl<'a, 'b> DebugStruct<'a, 'b> {
    pub fn field(&mut self, name: &str, value: &dyn Debug) -> &mut Self {
        self.result = self.result.and_then(|_| {
            let prefix = if self.has_fields { ", " } else { " { " };
            self.fmt.write_str(prefix)?;
            self.fmt.write_str(name)?;
            self.fmt.write_str(": ")?;
            value.fmt(self.fmt)
        });

        self.has_fields = true;
        self
    }

    pub fn finish(&mut self) -> Result {
        if self.has_fields {
            self.result = self.result.and_then(|_| self.fmt.write_str(" }"));
        }
        self.result
    }
}
