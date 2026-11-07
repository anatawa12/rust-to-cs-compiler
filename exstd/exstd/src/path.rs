use std::ffi::{OsStr, OsString};

pub enum Component<'a> {
    Prefix(&'a OsStr),
    RootDir,
    CurDir,
    ParentDir,
    Normal(&'a OsStr),
}

impl<'a> Component<'a> {
    pub fn as_os_str(&self) -> &OsStr {
        match self {
            Component::Prefix(s) => s,
            Component::RootDir => OsStr::new("/"),
            Component::CurDir => OsStr::new("."),
            Component::ParentDir => OsStr::new(".."),
            Component::Normal(s) => s,
        }
    }
}

pub struct StripPrefixError {}
