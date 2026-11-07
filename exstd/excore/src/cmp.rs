pub trait PartialEq<Rhs = Self>
where
    Rhs: ?Sized,
{
    fn eq(&self, other: &Rhs) -> bool;
}

pub trait Eq: PartialEq<Self> {}

pub trait PartialOrd<Rhs = Self>
where
    Rhs: ?Sized,
{
    fn partial_cmp(&self, other: &Rhs) -> Option<Ordering>;
}

pub trait Ord: Eq + PartialOrd<Self> {}

#[derive(Copy)]
pub enum Ordering {
    Less,
    Equal,
    Greater,
}

impl Ordering {
    pub fn is_eq(self) -> bool {
        matches!(self, Ordering::Equal)
    }

    pub fn is_ne(self) -> bool {
        !self.is_eq()
    }

    pub fn is_lt(self) -> bool {
        matches!(self, Ordering::Less)
    }

    pub fn is_gt(self) -> bool {
        matches!(self, Ordering::Greater)
    }

    pub fn is_le(self) -> bool {
        matches!(self, Ordering::Less | Ordering::Equal)
    }

    pub fn is_ge(self) -> bool {
        matches!(self, Ordering::Greater | Ordering::Equal)
    }

    pub fn then_with(self, f: impl FnOnce() -> Ordering) -> Ordering {
        match self {
            Ordering::Equal => f(),
            _ => self,
        }
    }
}

impl Clone for Ordering {
    fn clone(&self) -> Self {
        *self
    }
}
