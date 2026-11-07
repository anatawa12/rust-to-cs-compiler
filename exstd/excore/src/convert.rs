pub trait From<T> {
    fn from(t: T) -> Self;
}

pub trait TryFrom<T>: Sized {
    type Error;
    fn try_from(t: T) -> Result<Self, Self::Error>;
}

pub trait AsRef<T> {
    fn as_ref(&self) -> &T;
}
