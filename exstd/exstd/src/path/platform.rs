use crate::ffi::OsStr;
use crate::path::Prefix;

pub static MAIN_SEPARATOR_STR: &str = if cfg!(windows) { "\\" } else { "/" };

pub fn is_sep_byte(byte: u8) -> bool {
    if cfg!(windows) {
        byte == b'/' || byte == b'\\'
    } else {
        byte == b'/'
    }
}

pub fn is_verbatim_sep(byte: u8) -> bool {
    if cfg!(windows) {
        byte == b'\\'
    } else {
        byte == b'/'
    }
}

pub fn parse_prefix(bytes: &[u8]) -> Option<Prefix> {
    if !cfg!(windows) {
        return None;
    }

    #[cfg_attr(r2cs, r2cs_inline)]
    macro_rules! s {
        () => {
            b'\\' | b'/'
        };
    }

    fn split_once_both(bytes: &[u8]) -> (&[u8], &[u8]) {
        if let Some(pos) = bytes.iter().position(|&b| b == b'\\' || b == b'/') {
            (&bytes[..pos], &bytes[pos + 1..])
        } else {
            (bytes, &[])
        }
    }

    fn split_once_verbatim(bytes: &[u8]) -> (&[u8], &[u8]) {
        if let Some(pos) = bytes.iter().position(|&b| b == b'\\') {
            (&bytes[..pos], &bytes[pos + 1..])
        } else {
            (bytes, &[])
        }
    }

    match bytes {
        [b'\\', b'\\', b'?', b'\\', ..] => {
            // \\?\ verbatim path. This doesn't allow '/' separator

            match bytes[4..] {
                [b'U', b'N', b'C', b'\\', ..] => {
                    // \\?\UNC\server\share
                    let bytes = &bytes[8..];
                    let (server, bytes) = split_once_verbatim(bytes);
                    let (share, _) = split_once_verbatim(bytes);

                    Some(Prefix::VerbatimUNC(
                        OsStr::from_encoded_bytes_unchecked(server),
                        OsStr::from_encoded_bytes_unchecked(share),
                    ))
                }
                [b'a'..=b'z' | b'A'..=b'Z', b':', b'\\', ..]
                | [b'a'..=b'z' | b'A'..=b'Z', b':'] => {
                    // \\?\C:
                    Some(Prefix::VerbatimDisk(bytes[4]))
                }
                _ => {
                    // \\?\prefix
                    let bytes = &bytes[4..];
                    let (verbatim, _) = split_once_verbatim(bytes);
                    Some(Prefix::Verbatim(OsStr::from_encoded_bytes_unchecked(
                        verbatim,
                    )))
                }
            }
        }
        [s!(), s!(), b'.', s!(), ..] => {
            // \\.\COM42
            let (device, _) = split_once_both(&bytes[4..]);

            Some(Prefix::DeviceNS(OsStr::from_encoded_bytes_unchecked(
                device,
            )))
        }
        [s!(), s!(), ..] => {
            // \\server\share
            let (server, rest) = split_once_both(&bytes[2..]);
            let (share, _) = split_once_both(rest);
            if !server.is_empty() && !share.is_empty() {
                Some(Prefix::UNC(
                    OsStr::from_encoded_bytes_unchecked(server),
                    OsStr::from_encoded_bytes_unchecked(rest),
                ))
            } else {
                None
            }
        }
        [b'a'..=b'z' | b'A'..=b'Z', b':', s!(), ..] | [b'a'..=b'z' | b'A'..=b'Z', b':'] => {
            // C:
            Some(Prefix::Disk(bytes[0]))
        }
        _ => None,
    }
}
