use crate::error::{AppError, AppResult};
use std::path::PathBuf;

/// Expand %USERPROFILE% / %APPDATA% / %LOCALAPPDATA% / $HOME / ~ in a path string.
pub fn expand(input: &str) -> AppResult<PathBuf> {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return Err(AppError::Path("empty path".into()));
    }

    // ~ at start
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        let home = dirs::home_dir().ok_or_else(|| AppError::Path("no home dir".into()))?;
        let rest = if s.len() > 1 { &s[2..] } else { "" };
        return Ok(home.join(rest));
    }

    // Windows-style env vars
    for (var, getter) in &[
        ("%USERPROFILE%", dirs::home_dir as fn() -> Option<PathBuf>),
        ("%APPDATA%", dirs::config_dir as fn() -> Option<PathBuf>),
        (
            "%LOCALAPPDATA%",
            dirs::data_local_dir as fn() -> Option<PathBuf>,
        ),
        ("%HOME%", dirs::home_dir as fn() -> Option<PathBuf>),
        ("%DOCUMENTS%", dirs::document_dir as fn() -> Option<PathBuf>),
    ] {
        if s.to_uppercase().contains(var) {
            if let Some(p) = getter() {
                let p_str = p.to_string_lossy().to_string();
                // case-insensitive replace
                s = case_insensitive_replace(&s, var, &p_str);
            }
        }
    }

    // POSIX-style env vars
    if s.contains('$') {
        if let Some(home) = dirs::home_dir() {
            s = s.replace("$HOME", &home.to_string_lossy());
        }
    }

    Ok(PathBuf::from(s))
}

fn case_insensitive_replace(haystack: &str, needle: &str, replacement: &str) -> String {
    // env-var tokens are pure ASCII (`%USERPROFILE%` etc.), so byte-level
    // ASCII compare is safe and avoids `to_lowercase()` byte-length drift
    // on non-ASCII haystacks (e.g. paths with German ß / Turkish İ).
    let h_bytes = haystack.as_bytes();
    let n_bytes = needle.as_bytes();
    if n_bytes.is_empty() || n_bytes.len() > h_bytes.len() {
        return haystack.to_string();
    }
    let mut result: Vec<u8> = Vec::with_capacity(haystack.len());
    let mut i = 0;
    while i < h_bytes.len() {
        if i + n_bytes.len() <= h_bytes.len()
            && h_bytes[i..i + n_bytes.len()]
                .iter()
                .zip(n_bytes.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            result.extend_from_slice(replacement.as_bytes());
            i += n_bytes.len();
        } else {
            result.push(h_bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Resolve the GSyncing data directory.
pub fn data_dir() -> AppResult<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| AppError::Path("no data dir".into()))?;
    Ok(base.join("GSyncing"))
}

pub fn config_file() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("config.json"))
}

pub fn log_file() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("gsyncing.log"))
}

/// Normalize a relative path into "/"-separated form for remote keys.
pub fn to_remote_key(prefix: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_start_matches('/');
    let prefix = prefix.trim_end_matches('/');
    if prefix.is_empty() {
        rel.to_string()
    } else {
        format!("{prefix}/{rel}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_remote_key_handles_separators() {
        assert_eq!(to_remote_key("games/x", "a/b.dat"), "games/x/a/b.dat");
        assert_eq!(to_remote_key("games/x/", "a/b.dat"), "games/x/a/b.dat");
        assert_eq!(to_remote_key("games/x", "/a/b.dat"), "games/x/a/b.dat");
        assert_eq!(to_remote_key("games/x", "a\\b\\c.dat"), "games/x/a/b/c.dat");
        assert_eq!(to_remote_key("", "a/b"), "a/b");
    }

    #[test]
    fn case_insensitive_replace_ascii_token() {
        let out =
            case_insensitive_replace("C:\\Users\\foo\\%appdata%\\Game", "%APPDATA%", "C:/Roaming");
        assert_eq!(out, "C:\\Users\\foo\\C:/Roaming\\Game");
    }

    #[test]
    fn case_insensitive_replace_preserves_non_ascii() {
        // No %APPDATA% in input → should be identical (no panic on Chinese).
        let s = "C:/用户/张三/AppData";
        let out = case_insensitive_replace(s, "%APPDATA%", "X");
        assert_eq!(out, s);
    }

    #[test]
    fn case_insensitive_replace_empty_needle_is_noop() {
        let out = case_insensitive_replace("hello", "", "x");
        assert_eq!(out, "hello");
    }

    #[test]
    fn expand_empty_errors() {
        assert!(expand("").is_err());
        assert!(expand("   ").is_err());
    }

    #[test]
    fn expand_tilde_uses_home() {
        let home = dirs::home_dir().expect("test env has HOME");
        let out = expand("~/foo").unwrap();
        assert_eq!(out, home.join("foo"));
        let out2 = expand("~").unwrap();
        assert_eq!(out2, home);
    }
}
