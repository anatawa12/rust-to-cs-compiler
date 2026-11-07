pub enum Option<T> {
    Some(T),
    None,
}

// Those are fake impl
impl<T> Option<T> {
    pub fn as_mut(self) -> Option<T> {
        self
    }

    pub fn as_ref(self) -> Option<T> {
        self
    }

    pub fn cloned(self) -> Option<T> {
        self
    }
}

impl<T> Option<T> {
    pub fn is_none(&self) -> bool {
        matches!(self, Option::None)
    }

    pub fn is_some(&self) -> bool {
        matches!(self, Option::Some(_))
    }

    pub fn unwrap(self) -> T {
        match self {
            Option::Some(value) => value,
            Option::None => panic!("called `Option::unwrap()` on a `None` value"),
        }
    }

    pub fn unwrap_or(self, or: T) -> T {
        match self {
            Option::Some(value) => value,
            Option::None => or,
        }
    }

    pub fn unwrap_or_else(self, or: impl FnOnce() -> T) -> T {
        match self {
            Option::Some(value) => value,
            Option::None => or(),
        }
    }

    pub fn ok_or<E>(self, err: E) -> Result<T, E> {
        match self {
            Option::Some(value) => Ok(value),
            Option::None => Err(err),
        }
    }

    pub fn ok_or_else<E>(self, err: impl FnOnce() -> E) -> Result<T, E> {
        match self {
            Option::Some(value) => Ok(value),
            Option::None => Err(err()),
        }
    }

    pub fn and_then<U>(self, f: impl FnOnce(T) -> Option<U>) -> Option<U> {
        match self {
            Option::Some(value) => f(value),
            Option::None => Option::None,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Option<U> {
        match self {
            Option::Some(value) => Option::Some(f(value)),
            Option::None => Option::None,
        }
    }

    pub fn or(self, other: Option<T>) -> Option<T> {
        match self {
            Option::Some(value) => Option::Some(value),
            Option::None => other,
        }
    }

    pub fn or_else(self, other: impl FnOnce() -> Option<T>) -> Option<T> {
        match self {
            Option::Some(value) => Option::Some(value),
            Option::None => other(),
        }
    }

    pub fn filter(self, f: impl FnOnce(&T) -> bool) -> Option<T> {
        match self {
            Option::Some(value) if f(&value) => Option::Some(value),
            _ => Option::None,
        }
    }

    pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool {
        match self {
            Option::Some(value) => f(value),
            Option::None => false,
        }
    }
}

#[allow(clippy::derivable_impls)]
impl<T> Default for Option<T> {
    fn default() -> Self {
        Option::None
    }
}
