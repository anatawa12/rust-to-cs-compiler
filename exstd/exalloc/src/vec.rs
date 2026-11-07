pub struct Vec<T> {
    #[cfg(not(r2cs))]
    _field: std::vec::Vec<T>,
}

/// native methods
impl<T> Vec<T> {
    #[cfg_attr(r2cs, r2cs_native)]
    pub fn new() -> Vec<T> {
        Self {
            _field: std::vec::Vec::new(),
        }
    }

    #[cfg_attr(r2cs, r2cs_native)]
    pub fn len(&self) -> usize {
        self._field.len()
    }

    #[cfg_attr(r2cs, r2cs_native)]
    pub fn as_slice(&self) -> &[T] {
        self._field.as_slice()
    }
}
