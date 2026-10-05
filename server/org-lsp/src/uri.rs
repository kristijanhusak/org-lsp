//! Conversion between `file:` URIs and paths.

use std::path::{Path, PathBuf};

use lsp_types::Uri;

/// The path a `file:` URI names, or `None` for any other scheme.
pub fn to_path(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    // Skip the authority, which is empty or `localhost` for local files.
    let path = &rest[rest.find('/')?..];
    let decoded = percent_decode(path);
    // `/C:/dir` is a Windows path.
    let bytes = decoded.as_bytes();
    let windows = bytes.len() >= 3 && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    Some(PathBuf::from(if windows { &decoded[1..] } else { &decoded[..] }))
}

/// The `file:` URI for an absolute path.
pub fn from_path(path: &Path) -> Option<Uri> {
    let text = path.to_str()?.replace('\\', "/");
    let mut uri = String::from("file://");
    if !text.starts_with('/') {
        uri.push('/');
    }
    for byte in text.bytes() {
        // Unreserved characters, the path separator and the drive colon.
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri.parse().ok()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |offset: usize| bytes.get(at + offset).and_then(|b| char::from(*b).to_digit(16));
        match (bytes[at], hex(1), hex(2)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                at += 3;
            }
            (byte, ..) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The file's name, without its directories.
pub fn file_name(uri: &Uri) -> String {
    let path = percent_decode(uri.as_str());
    path.rsplit('/').next().unwrap_or("").to_string()
}

/// The file name without directory and extension: the category Org gives
/// a file that sets none.
pub fn file_stem(uri: &Uri) -> String {
    let path = percent_decode(uri.as_str());
    let name = path.rsplit('/').next().unwrap_or("");
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_round_trip() {
        for path in ["/home/me/org/notes.org", "/tmp/with space/日本.org", "/a/b#c%d.org"] {
            let uri = from_path(Path::new(path)).unwrap();
            assert_eq!(to_path(&uri).unwrap(), PathBuf::from(path), "{}", uri.as_str());
        }
        assert_eq!(from_path(Path::new("/tmp/a b.org")).unwrap().as_str(), "file:///tmp/a%20b.org");
    }

    #[test]
    fn other_forms() {
        let uri: Uri = "file://localhost/tmp/a.org".parse().unwrap();
        assert_eq!(to_path(&uri).unwrap(), PathBuf::from("/tmp/a.org"));
        let windows: Uri = "file:///C:/Users/me/a.org".parse().unwrap();
        assert_eq!(to_path(&windows).unwrap(), PathBuf::from("C:/Users/me/a.org"));
        let other: Uri = "org-agenda:/Org%20Agenda".parse().unwrap();
        assert_eq!(to_path(&other), None);
        assert_eq!(file_stem(&"file:///tmp/my%20notes.org".parse().unwrap()), "my notes");
        assert_eq!(file_stem(&"untitled:Untitled-1".parse().unwrap()), "untitled:Untitled-1");
    }
}
