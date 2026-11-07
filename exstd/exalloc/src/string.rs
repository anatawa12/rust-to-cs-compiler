use std::cmp::Ordering;

pub struct String(Vec<u8>);

/// native
impl String {
    #[cfg_attr(r2cs, r2cs_native)]
    pub fn as_str(&self) -> &str {
        panic!()
    }
}

impl String {
    pub fn new() -> String {
        String(Vec::new())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[allow(clippy::derivable_impls)]
impl Default for String {
    fn default() -> Self {
        String(Vec::new())
    }
}

impl PartialEq for String {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for String {}

impl PartialOrd for String {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for String {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialEq<str> for String {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
