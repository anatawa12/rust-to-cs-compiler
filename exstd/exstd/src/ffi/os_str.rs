use std::borrow::Cow;
use std::ops::Deref;

pub struct OsStr([u8]);

/// Native functions
impl OsStr {
    #[cfg_attr(r2cs, r2cs_native)]
    #[allow(clippy::unconditional_recursion)]
    pub fn from_encoded_bytes_unchecked(bytes: &[u8]) -> &OsStr {
        OsStr::from_encoded_bytes_unchecked(bytes)
    }
}

impl OsStr {
    pub fn new(str: &str) -> &Self {
        OsStr::from_encoded_bytes_unchecked(str.as_bytes())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_os_str(&self) -> &Self {
        self
    }

    pub fn to_owned(&self) -> OsString {
        OsString(self.0.to_vec())
    }

    pub fn starts_with(&self, prefix: &OsStr) -> bool {
        self.0.starts_with(&prefix.0)
    }

    pub fn ends_with(&self, suffix: &OsStr) -> bool {
        self.0.ends_with(&suffix.0)
    }

    pub fn to_string_lossy(&self) -> Box<str> {
        let mut result = String::new();
        let bytes = &self.0;

        let mut index = 0;
        while index < bytes.len() {
            let mut char;
            match bytes[index] {
                0x00..=0x7F => {
                    char = bytes[index] as u32;
                    index += 1;
                }
                0xC0..=0xDF => {
                    char = (bytes[index] as u32 & 0x1F) << 6;
                    char |= bytes[index + 1] as u32 & 0x3F;
                    index += 2;
                }
                0xE0..=0xEF => {
                    char = (bytes[index] as u32 & 0x0F) << 12;
                    char |= (bytes[index + 1] as u32 & 0x3F) << 6;
                    char |= bytes[index + 2] as u32 & 0x3F;
                    index += 3;
                }
                _ /*0xF0..=0xF7*/ => {
                    char = (bytes[index] as u32 & 0x07) << 18;
                    char |= (bytes[index + 1] as u32 & 0x3F) << 12;
                    char |= (bytes[index + 2] as u32 & 0x3F) << 6;
                    char |= bytes[index + 3] as u32 & 0x3F;
                    index += 4;
                }
            };
            result.push(char::from_u32(char).unwrap_or('\u{FFFD}'));
        }

        result.into_boxed_str()
    }
}

pub struct OsString(Vec<u8>);

/// Native functions
impl OsString {}

impl OsString {
    pub fn with_capacity(capacity: usize) -> Self {
        OsString(Vec::with_capacity(capacity))
    }

    pub fn push(&mut self, other: &OsStr) {
        self.0.extend_from_slice(&other.0);
    }
}

impl From<&str> for OsString {
    fn from(value: &str) -> Self {
        OsString(value.as_bytes().to_vec())
    }
}

impl Deref for OsString {
    type Target = OsStr;

    fn deref(&self) -> &Self::Target {
        OsStr::from_encoded_bytes_unchecked(self.0.as_slice())
    }
}

impl Clone for OsString {
    fn clone(&self) -> Self {
        OsString(self.0.clone())
    }
}

impl AsRef<OsStr> for OsString {
    fn as_ref(&self) -> &OsStr {
        self.deref()
    }
}
