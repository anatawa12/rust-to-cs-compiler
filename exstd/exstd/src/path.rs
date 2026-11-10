use crate::ffi::{OsStr, OsString};
use crate::path::platform::{MAIN_SEPARATOR_STR, is_sep_byte, is_verbatim_sep};
use excore::fmt;
use std::ops::Deref;

mod platform;

#[derive(Copy)]
pub enum Component<'a> {
    Prefix(PrefixComponent<'a>),
    RootDir,
    CurDir,
    ParentDir,
    Normal(&'a OsStr),
}

impl Clone for Component<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

impl PartialEq for Component<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Component::Prefix(this), Component::Prefix(other)) => this == other,
            (Component::RootDir, Component::RootDir) => true,
            (Component::CurDir, Component::CurDir) => true,
            (Component::ParentDir, Component::ParentDir) => true,
            (Component::Normal(this), Component::Normal(other)) => this == other,
            _ => false,
        }
    }
}

impl<'a> Component<'a> {
    pub fn as_os_str(&self) -> &OsStr {
        match self {
            Component::Prefix(s) => s.as_os_str(),
            Component::RootDir => OsStr::new("/"),
            Component::CurDir => OsStr::new("."),
            Component::ParentDir => OsStr::new(".."),
            &Component::Normal(s) => s,
        }
    }
}

#[derive(Copy)]
pub struct PrefixComponent<'a> {
    raw: &'a OsStr,
    parsed: Prefix<'a>,
}

impl<'a> PrefixComponent<'a> {
    pub fn as_os_str(&self) -> &OsStr {
        self.raw
    }
}

impl Clone for PrefixComponent<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

impl PartialEq for PrefixComponent<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

#[derive(Copy)]
pub enum Prefix<'a> {
    Verbatim(&'a OsStr),
    VerbatimUNC(&'a OsStr, &'a OsStr),
    VerbatimDisk(u8),
    DeviceNS(&'a OsStr),
    UNC(&'a OsStr, &'a OsStr),
    Disk(u8),
}

impl<'a> Clone for Prefix<'a> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a> Prefix<'a> {
    fn len(&self) -> usize {
        use self::Prefix::*;
        match *self {
            Verbatim(x) => 4 + x.len(),
            VerbatimUNC(x, y) => 8 + x.len() + if y.len() > 0 { 1 + y.len() } else { 0 },
            VerbatimDisk(_) => 6,
            UNC(x, y) => 2 + x.len() + if y.len() > 0 { 1 + y.len() } else { 0 },
            DeviceNS(x) => 4 + x.len(),
            Disk(_) => 2,
        }
    }

    pub fn is_verbatim(&self) -> bool {
        use self::Prefix::*;
        matches!(*self, Verbatim(_) | VerbatimDisk(_) | VerbatimUNC(..))
    }

    fn is_drive(&self) -> bool {
        matches!(*self, Prefix::Disk(_))
    }

    fn has_implicit_root(&self) -> bool {
        !self.is_drive()
    }
}

pub struct StripPrefixError(());

pub struct Path {
    inner: OsStr,
}

/// R2CS Native
impl Path {
    #[cfg_attr(r2cs, r2cs_native)]
    #[allow(clippy::unconditional_recursion)]
    pub fn new(s: &OsStr) -> &Path {
        Self::new(s)
    }
}

impl Path {
    pub fn to_string_lossy(&self) -> Box<str> {
        self.inner.to_string_lossy()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.to_owned()
    }

    pub fn to_owned(&self) -> PathBuf {
        PathBuf {
            inner: self.inner.to_owned(),
        }
    }

    pub fn as_os_str(&self) -> &OsStr {
        &self.inner
    }

    pub fn components(&self) -> Components<'_> {
        Components::new(self.inner.as_encoded_bytes())
    }

    fn prefix(&self) -> Option<Prefix<'_>> {
        self.components().prefix
    }

    pub fn has_root(&self) -> bool {
        self.components().has_root()
    }

    pub fn is_absolute(&self) -> bool {
        if cfg!(windows) {
            self.has_root() && self.prefix().is_some()
        } else {
            self.has_root()
        }
    }

    pub fn join(&self, other: &Path) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.push(other);
        buf
    }

    pub fn with_extension(&self, extension: &mut OsStr) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.set_extension(extension);
        buf
    }

    pub fn file_name(&self) -> Option<&OsStr> {
        if let Some(Component::Normal(normal)) = self.components().collect::<Vec<_>>().last() {
            Some(normal)
        } else {
            None
        }
    }

    pub fn parent(&self) -> Option<&Path> {
        let mut comps = self.components();
        let comp = comps.next_back();
        comp.and_then(|p| match p {
            Component::Normal(_) | Component::CurDir | Component::ParentDir => {
                Some(comps.as_path())
            }
            _ => None,
        })
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.file_name()
            .map(rsplit_file_at_dot)
            .and_then(|(before, after)| before.and(after))
    }

    pub fn file_stem(&self) -> Option<&OsStr> {
        self.file_name()
            .map(rsplit_file_at_dot)
            .and_then(|(before, after)| before.or(after))
    }

    pub fn starts_with(&self, base: &Path) -> bool {
        iter_after(self.components(), base.components()).is_some()
    }

    pub fn strip_prefix(&self, base: &Path) -> Result<&Path, StripPrefixError> {
        iter_after(self.components(), base.components())
            .map(|c| c.as_path())
            .ok_or(StripPrefixError(()))
    }
}

fn iter_after<'a, 'b>(
    mut iter: Components<'a>,
    mut prefix: Components<'b>,
) -> Option<Components<'a>> {
    loop {
        let mut iter_next = iter.clone();
        match (iter_next.next(), prefix.next()) {
            (Some(ref x), Some(ref y)) if x == y => (),
            (Some(_), Some(_)) => return None,
            (Some(_), None) => return Some(iter),
            (None, None) => return Some(iter),
            (None, Some(_)) => return None,
        }
        iter = iter_next;
    }
}

fn rsplit_file_at_dot(file: &OsStr) -> (Option<&OsStr>, Option<&OsStr>) {
    if file.as_encoded_bytes() == b".." {
        return (Some(file), None);
    }

    let mut iter = file.as_encoded_bytes().rsplitn(2, |b| *b == b'.');
    let after = iter.next();
    let before = iter.next();
    if before == Some(b"") {
        (Some(file), None)
    } else {
        (
            before.map(OsStr::from_encoded_bytes_unchecked),
            after.map(OsStr::from_encoded_bytes_unchecked),
        )
    }
}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        self.components() == other.components()
    }
}

impl<'a> From<&'a PathBuf> for &'a Path {
    fn from(value: &'a PathBuf) -> &'a Path {
        Path::new(value.inner.as_os_str())
    }
}

pub struct PathBuf {
    inner: OsString,
}

impl PathBuf {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: OsString::with_capacity(capacity),
        }
    }

    pub fn into_os_string(self) -> OsString {
        self.inner
    }

    pub fn push(&mut self, path: &Path) {
        // in general, a separator is needed if the rightmost byte is not a separator
        let buf = self.inner.as_encoded_bytes();
        let mut need_sep = buf.last().map(|c| !is_sep_byte(*c)).unwrap_or(false);

        // in the special case of `C:` on Windows, do *not* add a separator
        let comps = self.components();

        if comps.prefix_len() > 0
            && comps.prefix_len() == comps.path.len()
            && comps.prefix.unwrap().is_drive()
        {
            need_sep = false
        }

        let need_clear = {
            // On Unix: prefix is always None.
            path.is_absolute() || path.prefix().is_some()
        };

        if need_clear {
            // absolute `path` replaces `self`
            self.inner.clear();
        } else if comps.prefix_verbatim() && !path.inner.is_empty() {
            // verbatim paths need . and .. removed
            let mut buf: Vec<_> = comps.collect();
            for c in path.components() {
                match c {
                    Component::RootDir => {
                        buf.truncate(1);
                        buf.push(c);
                    }
                    Component::CurDir => (),
                    Component::ParentDir => {
                        if let Some(Component::Normal(_)) = buf.last() {
                            buf.pop();
                        }
                    }
                    _ => buf.push(c),
                }
            }

            let mut res = OsString::new();
            let mut need_sep = false;

            for c in buf {
                if need_sep && matches!(c, Component::RootDir) {
                    res.push(OsStr::new(MAIN_SEPARATOR_STR));
                }
                res.push(c.as_os_str());

                need_sep = match c {
                    Component::RootDir => false,
                    Component::Prefix(prefix) => {
                        !prefix.parsed.is_drive() && prefix.parsed.len() > 0
                    }
                    _ => true,
                }
            }

            self.inner = res;
            return;
        } else if path.has_root() {
            // `path` has a root but no prefix, e.g., `\windows` (Windows only)
            let prefix_len = self.components().prefix_remaining();
            self.inner.truncate(prefix_len);
        } else if need_sep {
            // `path` is a pure relative path
            self.inner.push(OsStr::new(MAIN_SEPARATOR_STR));
        }

        self.inner.push(&path.inner);
    }

    pub fn pop(&mut self) -> bool {
        match self
            .parent()
            .map(|p| p.as_os_str().as_encoded_bytes().len())
        {
            Some(len) => {
                self.inner.truncate(len);
                true
            }
            None => false,
        }
    }

    pub fn joined(mut self, component: &Path) -> PathBuf {
        self.push(component);
        self
    }

    pub fn set_extension(&mut self, extension: &mut OsStr) -> bool {
        validate_extension(extension);

        let file_stem = match self.file_stem() {
            None => return false,
            Some(f) => f.as_encoded_bytes(),
        };

        // truncate until right after the file stem
        let len = (self.inner.as_encoded_bytes())
            .subslice_range(&file_stem[file_stem.len()..])
            .unwrap()
            .start;
        self.inner.truncate(len);
        //let end_file_stem = file_stem[file_stem.len()..].as_ptr().addr();
        //let start = self.inner.as_encoded_bytes().as_ptr().addr();
        // end_file_stem.wrapping_sub(start)

        // add the new extension, if any
        let new = extension.as_encoded_bytes();
        if !new.is_empty() {
            self.inner.reserve(new.len() + 1);
            self.inner.push(OsStr::new("."));
            // SAFETY: Since a UTF-8 string was just pushed, it is not possible
            // for the buffer to end with a surrogate half.
            self.inner.push(OsStr::from_encoded_bytes_unchecked(new));
        }

        true
    }
}

fn validate_extension(extension: &OsStr) {
    for &b in extension.as_encoded_bytes() {
        if is_sep_byte(b) {
            panic!("extension cannot contain path separators");
        }
    }
}

impl From<&str> for PathBuf {
    fn from(value: &str) -> Self {
        Self::from(OsString::from(value))
    }
}

impl From<&Path> for PathBuf {
    fn from(value: &Path) -> Self {
        Self::from(value.inner.to_owned())
    }
}

impl From<String> for PathBuf {
    fn from(value: String) -> Self {
        Self::from(OsString::from(value))
    }
}

impl From<OsString> for PathBuf {
    fn from(value: OsString) -> Self {
        Self { inner: value }
    }
}

impl Deref for PathBuf {
    type Target = Path;

    fn deref(&self) -> &Path {
        Path::new(&self.inner)
    }
}

impl AsRef<Path> for PathBuf {
    fn as_ref(&self) -> &Path {
        self.deref()
    }
}

impl Clone for PathBuf {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg_attr(r2cs, r2cs_no_trait_first)]
pub struct Components<'a> {
    path: &'a [u8],
    prefix: Option<Prefix<'a>>,
    has_physical_root: bool,
    state: State,

    // C# iterator's method
    current: Component<'a>,
}

#[derive(Copy)]
enum State {
    Prefix,
    StartDir,
    Body,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (State::Prefix, State::Prefix) => true,
            (State::StartDir, State::StartDir) => true,
            (State::Body, State::Body) => true,
            _ => false,
        }
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a> Components<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let prefix = platform::parse_prefix(bytes);
        let path = if let Some(p) = prefix {
            &bytes[p.len()..]
        } else {
            bytes
        };
        let has_physical_root = !path.is_empty() && is_sep_byte(path[0]);
        Self {
            path: bytes,
            prefix,
            has_physical_root,
            state: State::Prefix,

            current: Component::CurDir,
        }
    }

    pub fn as_path(&self) -> &'a Path {
        let mut comps = self.clone();
        if comps.state == State::Body {
            comps.trim_left();
        }
        Path::new(OsStr::from_encoded_bytes_unchecked(self.path))
    }

    fn prefix_len(&self) -> usize {
        self.prefix.as_ref().map(Prefix::len).unwrap_or(0)
    }

    fn prefix_verbatim(&self) -> bool {
        self.prefix
            .as_ref()
            .map(Prefix::is_verbatim)
            .unwrap_or(false)
    }

    fn prefix_remaining(&self) -> usize {
        if self.state == State::Prefix {
            self.prefix_len()
        } else {
            0
        }
    }

    #[inline]
    fn is_sep_byte(&self, b: u8) -> bool {
        if self.prefix_verbatim() {
            is_verbatim_sep(b)
        } else {
            is_sep_byte(b)
        }
    }

    /// Is the *original* path rooted?
    fn has_root(&self) -> bool {
        if self.has_physical_root {
            return true;
        }
        if let Some(p) = self.prefix {
            if p.has_implicit_root() {
                return true;
            }
        }
        false
    }

    fn include_cur_dir(&self) -> bool {
        if self.has_root() {
            return false;
        }
        let slice = &self.path[self.prefix_remaining()..];
        match slice {
            [b'.'] => true,
            [b'.', b, ..] => self.is_sep_byte(*b),
            _ => false,
        }
    }
}

impl<'a> Components<'a> {
    fn parse_next_component(&self) -> (usize, Option<Component<'a>>) {
        assert!(self.state == State::Body, "Bad State");
        let mut rsplit = self.path.splitn(2, |b| self.is_sep_byte(*b));
        let component = rsplit.next();
        let remaining = rsplit.next();
        let extra = remaining.map(|_| 1).unwrap_or(0); // if there is remaining, there is sep_byte.
        let comp = remaining.or(component).unwrap();
        (comp.len() + extra, self.parse_single_component(comp))
    }

    fn parse_next_component_back(&self) -> (usize, Option<Component<'a>>) {
        assert!(self.path.len() > self.len_before_body(), "Bad State");
        let mut rsplit = self.path.rsplitn(2, |b| self.is_sep_byte(*b));
        let component = rsplit.next();
        let remaining = rsplit.next();
        let extra = remaining.map(|_| 1).unwrap_or(0); // if there is remaining, there is sep_byte.
        let comp = remaining.or(component).unwrap();
        (comp.len() + extra, self.parse_single_component(comp))
    }

    fn parse_single_component(&self, comp: &'a [u8]) -> Option<Component<'a>> {
        match comp {
            [b'.'] if self.prefix_verbatim() => Some(Component::CurDir),
            [b'.'] => None,
            [b'.', b'.'] => Some(Component::ParentDir),
            [] => None,
            _ => Some(Component::Normal(OsStr::from_encoded_bytes_unchecked(comp))),
        }
    }

    fn trim_left(&mut self) {
        while !self.path.is_empty() {
            let (size, comp) = self.parse_next_component();
            if comp.is_some() {
                return;
            } else {
                self.path = &self.path[size..];
            }
        }
    }

    fn trim_right(&mut self) {
        while self.path.len() > self.len_before_body() {
            let (size, comp) = self.parse_next_component_back();
            if comp.is_some() {
                return;
            } else {
                self.path = &self.path[..self.path.len() - size];
            }
        }
    }

    fn len_before_body(&self) -> usize {
        assert!(self.state == State::Prefix, "bad state");
        let root = if self.has_physical_root { 1 } else { 0 };
        let cur_dir = if self.include_cur_dir() { 1 } else { 0 };
        self.prefix_remaining() + root + cur_dir
    }

    // Note: this function does not support analyzing non-prefix part.
    fn next_back(&mut self) -> Option<Component<'a>> {
        while self.path.len() > self.len_before_body() {
            let (size, comp) = self.parse_next_component_back();
            self.path = &self.path[..self.path.len() - size];
            if comp.is_some() {
                return comp;
            }
        }
        None
    }
}

impl Components<'_> {
    #[cfg_attr(r2cs, r2cs_name_override = "MoveNext")]
    fn move_next(&mut self) -> bool {
        loop {
            match self.state {
                State::Prefix if self.prefix_len() == 0 => {
                    self.state = State::StartDir;
                }
                State::Prefix => {
                    self.state = State::StartDir;
                    let raw = &self.path[..self.prefix_len()];
                    self.path = &self.path[self.prefix_len()..];
                    self.current = Component::Prefix(PrefixComponent {
                        raw: OsStr::from_encoded_bytes_unchecked(raw),
                        parsed: self.prefix.unwrap(),
                    });
                    return true;
                }

                State::StartDir => {
                    self.state = State::Body;
                    if self.has_physical_root {
                        self.path = &self.path[1..];

                        self.current = Component::RootDir;
                        return true;
                    } else if let Some(p) = self.prefix {
                        if p.has_implicit_root() && !p.is_verbatim() {
                            self.current = Component::RootDir;
                            return true;
                        }
                    } else if self.include_cur_dir() {
                        self.path = &self.path[1..];
                        self.current = Component::CurDir;
                        return true;
                    }
                }

                State::Body if !self.path.is_empty() => {
                    let (size, comp) = self.parse_next_component();
                    self.path = &self.path[size..];
                    if let Some(comp) = comp {
                        self.current = comp;
                        return true;
                    }
                }
                State::Body => {
                    return false;
                }
            }
        }
    }
}

impl<'a> Iterator for Components<'a> {
    type Item = Component<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.move_next() {
            Some(self.current)
        } else {
            None
        }
    }
}

impl<'a> Clone for Components<'a> {
    fn clone(&self) -> Self {
        Self {
            path: self.path,
            prefix: self.prefix,
            has_physical_root: self.has_physical_root,
            state: self.state,
            current: self.current,
        }
    }
}

impl<'a> PartialEq for Components<'a> {
    #[inline]
    fn eq(&self, other: &Components<'a>) -> bool {
        // Fast path for exact match,
        if self.path.len() == other.path.len()
            && self.state == other.state
            && self.prefix_verbatim() == other.prefix_verbatim()
        {
            // possible future improvement: this could bail out earlier if there were a
            // reverse memcmp/bcmp comparing back to front
            if self.path == other.path {
                return true;
            }
        }

        let mut this = self.clone();
        let mut other = other.clone();
        loop {
            match (this.next(), other.next()) {
                (Some(this_component), Some(other_component))
                    if this_component == other_component => {}
                _ => return false,
            }
        }
    }
}
