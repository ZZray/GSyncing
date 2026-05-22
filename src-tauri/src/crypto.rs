//! Credential encryption.
//!
//! On Windows we use DPAPI (CryptProtectData / CryptUnprotectData) scoped to
//! the current user — same primitive Chromium / Edge use for their password
//! stores. The ciphertext is bound to the Windows user account, so an attacker
//! who copies `config.json` off the disk cannot decrypt it on another machine
//! or under another user.
//!
//! On non-Windows the API is a no-op pass-through with a "DPAPI_OFF" marker
//! so the format on disk is still consistent and a future Linux/macOS keyring
//! integration can take over without a schema change.

use crate::error::{AppError, AppResult};

/// Marker prefix that distinguishes ciphertext from plaintext during the
/// gradual migration of an existing v0.6 config.json.
const CIPHER_MARKER: &str = "gsy1:";
/// Sentinel for "passthrough" on platforms without an encryption backend.
const PLAIN_MARKER: &str = "gsy0:";

/// Encrypt a credential string. Returns a base64 blob with a version marker.
/// Empty strings round-trip as-is (no use storing ciphertext of an empty).
pub fn encrypt(plain: &str) -> AppResult<String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    #[cfg(windows)]
    {
        match dpapi_protect(plain.as_bytes()) {
            Ok(bytes) => Ok(format!("{CIPHER_MARKER}{}", base64_encode(&bytes))),
            Err(e) => {
                tracing::warn!("DPAPI encrypt failed, falling back to plaintext: {e}");
                Ok(format!("{PLAIN_MARKER}{}", base64_encode(plain.as_bytes())))
            }
        }
    }
    #[cfg(not(windows))]
    {
        Ok(format!("{PLAIN_MARKER}{}", base64_encode(plain.as_bytes())))
    }
}

/// Decrypt. Accepts:
///   - `gsy1:<base64>` → DPAPI ciphertext
///   - `gsy0:<base64>` → trivial encoding (non-Windows fallback)
///   - anything else  → treated as legacy plaintext (pre-v0.7 configs)
pub fn decrypt(stored: &str) -> AppResult<String> {
    if stored.is_empty() {
        return Ok(String::new());
    }
    if let Some(b64) = stored.strip_prefix(CIPHER_MARKER) {
        #[cfg(windows)]
        {
            let bytes = base64_decode(b64)?;
            let plain = dpapi_unprotect(&bytes)
                .map_err(|e| AppError::Other(format!("DPAPI decrypt: {e}")))?;
            return String::from_utf8(plain)
                .map_err(|e| AppError::Other(format!("utf8 after dpapi: {e}")));
        }
        #[cfg(not(windows))]
        {
            let _ = b64;
            return Err(AppError::Other(
                "ciphertext present but DPAPI not available on this platform".into(),
            ));
        }
    }
    if let Some(b64) = stored.strip_prefix(PLAIN_MARKER) {
        let bytes = base64_decode(b64)?;
        return String::from_utf8(bytes).map_err(|e| AppError::Other(format!("utf8: {e}")));
    }
    // Legacy plaintext — return as-is for backward compat. Caller is
    // responsible for re-encrypting on next save.
    Ok(stored.to_string())
}

/// True when `stored` was already produced by `encrypt`. Used by the
/// migration pass to skip already-encrypted entries.
pub fn is_already_protected(stored: &str) -> bool {
    stored.starts_with(CIPHER_MARKER) || stored.starts_with(PLAIN_MARKER)
}

// ---------- minimal base64 (we'd rather not add a crate for this) ----------

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(input: &str) -> AppResult<Vec<u8>> {
    let s = input.trim();
    let mut out: Vec<u8> = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            other => {
                return Err(AppError::Other(format!("bad base64 char {other:#x}")));
            }
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

// ---------- Windows DPAPI ----------

#[cfg(windows)]
fn dpapi_protect(plain: &[u8]) -> AppResult<Vec<u8>> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Foundation::HLOCAL;
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    let entropy_label = b"GSyncing-v0.7\0";
    let mut entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_label.len() as u32,
        pbData: entropy_label.as_ptr() as *mut u8,
    };
    unsafe {
        CryptProtectData(
            &mut in_blob,
            windows::core::w!("GSyncing credential"),
            Some(&mut entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
        .map_err(|e| AppError::Other(format!("CryptProtectData: {e}")))?;
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData as *mut _)));
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_round_trips_as_empty() {
        let enc = encrypt("").unwrap();
        assert_eq!(enc, "");
        let dec = decrypt("").unwrap();
        assert_eq!(dec, "");
    }

    #[test]
    fn round_trip_simple() {
        let enc = encrypt("hello world").unwrap();
        // Marker prefix must be present so the loader knows how to decode.
        assert!(enc.starts_with("gsy1:") || enc.starts_with("gsy0:"));
        let dec = decrypt(&enc).unwrap();
        assert_eq!(dec, "hello world");
    }

    #[test]
    fn round_trip_unicode() {
        let s = "密钥 - 黑暗剧情线 🎮";
        let enc = encrypt(s).unwrap();
        let dec = decrypt(&enc).unwrap();
        assert_eq!(dec, s);
    }

    #[test]
    fn legacy_plaintext_pass_through() {
        // Anything without a marker is treated as a v0.6 plaintext stored
        // before DPAPI rolled out — should return as-is.
        let dec = decrypt("AKIA-LEGACY-KEY").unwrap();
        assert_eq!(dec, "AKIA-LEGACY-KEY");
    }

    #[test]
    fn already_protected_detects_markers() {
        assert!(is_already_protected("gsy1:foo"));
        assert!(is_already_protected("gsy0:foo"));
        assert!(!is_already_protected("AKIA-LEGACY"));
        assert!(!is_already_protected(""));
    }

    #[test]
    fn base64_round_trip() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let enc = base64_encode(data);
        let dec = base64_decode(&enc).unwrap();
        assert_eq!(dec, data);
    }
}

#[cfg(windows)]
fn dpapi_unprotect(cipher: &[u8]) -> AppResult<Vec<u8>> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Foundation::HLOCAL;
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: cipher.len() as u32,
        pbData: cipher.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    let entropy_label = b"GSyncing-v0.7\0";
    let mut entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_label.len() as u32,
        pbData: entropy_label.as_ptr() as *mut u8,
    };
    unsafe {
        CryptUnprotectData(
            &mut in_blob,
            None,
            Some(&mut entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
        .map_err(|e| AppError::Other(format!("CryptUnprotectData: {e}")))?;
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData as *mut _)));
        Ok(slice)
    }
}
