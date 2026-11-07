pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl std::fmt::Display) -> Self {
        Self {
            kind,
            message: message.to_string(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Copy)]
pub enum ErrorKind {
    Interrupted,
    InvalidInput,
    InvalidData,
    UnexpectedEof,
    NotFound,
    AlreadyExists,
}

impl Clone for ErrorKind {
    fn clone(&self) -> Self {
        *self
    }
}
