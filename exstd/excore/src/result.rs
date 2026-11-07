pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

impl<T, E> Result<T, E> {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }

    pub fn ok(self) -> Option<T> {
        match self {
            Self::Ok(t) => Some(t),
            Self::Err(_) => None,
        }
    }

    pub fn err(self) -> Option<E> {
        match self {
            Self::Ok(_) => None,
            Self::Err(e) => Some(e),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Result<U, E> {
        match self {
            Self::Ok(t) => Result::Ok(f(t)),
            Self::Err(e) => Result::Err(e),
        }
    }

    pub fn map_err<U>(self, f: impl FnOnce(E) -> U) -> Result<T, U> {
        match self {
            Self::Ok(t) => Result::Ok(t),
            Self::Err(e) => Result::Err(f(e)),
        }
    }

    pub fn and_then<U>(self, f: impl FnOnce(T) -> Result<U, E>) -> Result<U, E> {
        match self {
            Self::Ok(t) => f(t),
            Self::Err(e) => Result::Err(e),
        }
    }

    pub fn unwrap(self) -> T {
        match self {
            Self::Ok(t) => t,
            Self::Err(e) => panic!("called `Result::unwrap()` on an `Err` value"),
        }
    }

    pub fn unwrap_err(self) -> E {
        match self {
            Self::Ok(t) => panic!("called `Result::unwrap_err()` on an `Ok` value"),
            Self::Err(e) => e,
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Ok(t) => t,
            Self::Err(_) => default,
        }
    }

    pub fn unwrap_or_else(self, f: impl FnOnce(E) -> T) -> T {
        match self {
            Self::Ok(t) => t,
            Self::Err(e) => f(e),
        }
    }
}
