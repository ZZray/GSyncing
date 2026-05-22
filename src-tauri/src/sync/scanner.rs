use crate::error::{AppError, AppResult};
use crate::model::GameProfile;
use crate::paths;
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct LocalFile {
    /// Absolute path on disk.
    pub absolute: PathBuf,
    /// Path relative to its save_path root, "/"-separated.
    pub relative: String,
    /// Which save_path root this belongs to (so we can map back on download).
    pub root_index: usize,
    pub size: u64,
    pub modified_ms: i64,
    pub sha256: String,
}

pub struct ScannedGame {
    pub game_id: String,
    pub roots: Vec<PathBuf>,
    pub files: Vec<LocalFile>,
    /// Number of files where we reused the prior hash via mtime+size match.
    pub reused_hashes: usize,
}

/// Hint passed to the scanner so we can skip SHA-256 for unchanged files.
/// Key = relative path. Value = (size, modified_ms, sha256) from the previous sync.
pub type ScanHints = HashMap<String, (u64, i64, String)>;

pub fn scan(game: &GameProfile) -> AppResult<ScannedGame> {
    scan_with_hints(game, &HashMap::new())
}

pub fn scan_with_hints(game: &GameProfile, hints: &ScanHints) -> AppResult<ScannedGame> {
    if game.save_paths.is_empty() {
        return Err(AppError::Config(format!(
            "game {} has no save paths",
            game.name
        )));
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    for raw in &game.save_paths {
        let p = paths::expand(raw)?;
        roots.push(p);
    }

    let include = build_globset(&game.include, &["**/*".to_string()])?;
    let exclude = build_globset(&game.exclude, &[])?;

    let mut files: Vec<LocalFile> = Vec::new();
    let mut reused_hashes: usize = 0;
    for (idx, root) in roots.iter().enumerate() {
        if !root.exists() {
            tracing::debug!("scan: root not found: {}", root.display());
            continue;
        }
        for entry in WalkDir::new(root).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("scan: walk error: {e}");
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let abs = entry.path();
            let rel = match abs.strip_prefix(root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if !include.is_match(&rel) {
                continue;
            }
            if !exclude.is_empty() && exclude.is_match(&rel) {
                continue;
            }
            let meta = match abs.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = meta.len();
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            // FreeFileSync-style fast path: if size + mtime match the prior
            // snapshot, trust the cached SHA-256 and skip rehashing. This is the
            // single biggest perf win — typical "nothing changed" syncs go from
            // O(N · file_size) bytes hashed to ~0.
            let sha256 = if let Some((prev_size, prev_mtime, prev_hash)) = hints.get(&rel) {
                if *prev_size == size && *prev_mtime == modified_ms {
                    reused_hashes += 1;
                    prev_hash.clone()
                } else if size <= 256 * 1024 * 1024 {
                    hash_file(abs)?
                } else {
                    fallback_hash(size, modified_ms)
                }
            } else if size <= 256 * 1024 * 1024 {
                hash_file(abs)?
            } else {
                fallback_hash(size, modified_ms)
            };
            files.push(LocalFile {
                absolute: abs.to_path_buf(),
                relative: rel,
                root_index: idx,
                size,
                modified_ms,
                sha256,
            });
        }
    }
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(ScannedGame {
        game_id: game.id.clone(),
        roots,
        files,
        reused_hashes,
    })
}

fn fallback_hash(size: u64, modified_ms: i64) -> String {
    let mut h = Sha256::new();
    h.update(size.to_le_bytes());
    h.update(modified_ms.to_le_bytes());
    hex::encode(h.finalize())
}

fn build_globset(patterns: &[String], default: &[String]) -> AppResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let src: &[String] = if patterns.is_empty() {
        default
    } else {
        patterns
    };
    for p in src {
        if p.trim().is_empty() {
            continue;
        }
        let glob = Glob::new(p).map_err(|e| AppError::Config(format!("bad glob {p}: {e}")))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| AppError::Config(format!("globset build: {e}")))
}

fn hash_file(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Build a map from relative path → LocalFile for fast lookup.
pub fn index_by_relative(files: &[LocalFile]) -> BTreeMap<String, &LocalFile> {
    files.iter().map(|f| (f.relative.clone(), f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GameProfile;

    fn write_dir(root: &Path, files: &[(&str, &[u8])]) {
        for (rel, body) in files {
            let full = root.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, body).unwrap();
        }
    }

    #[test]
    fn glob_include_exclude_filters_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_dir(
            root,
            &[
                ("save.dat", b"a"),
                ("cache/index.bin", b"b"),
                ("screenshot.png", b"c"),
            ],
        );
        let game = GameProfile {
            id: "test".into(),
            name: "test".into(),
            save_paths: vec![root.to_string_lossy().into()],
            include: vec!["**/*.dat".into()],
            exclude: vec!["**/cache/**".into()],
            ..Default::default()
        };
        let scanned = scan(&game).unwrap();
        let rels: Vec<&str> = scanned.files.iter().map(|f| f.relative.as_str()).collect();
        assert_eq!(rels, vec!["save.dat"]);
    }

    #[test]
    fn hints_reuse_skips_rehash() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_dir(root, &[("save.dat", b"identical")]);
        // First scan computes a real SHA.
        let game = GameProfile {
            id: "test".into(),
            name: "test".into(),
            save_paths: vec![root.to_string_lossy().into()],
            include: vec!["**/*".into()],
            ..Default::default()
        };
        let first = scan(&game).unwrap();
        let real_sha = first.files[0].sha256.clone();
        let size = first.files[0].size;
        let mtime = first.files[0].modified_ms;
        // Now scan with hints pretending the on-disk SHA was something else
        // — and showing the same size+mtime. We expect the hint to win, so the
        // returned hash is the *hinted* value not a fresh hash.
        let hints: ScanHints = std::collections::HashMap::from_iter([(
            "save.dat".to_string(),
            (size, mtime, "FAKE".to_string()),
        )]);
        let second = scan_with_hints(&game, &hints).unwrap();
        assert_eq!(second.files[0].sha256, "FAKE");
        assert_eq!(second.reused_hashes, 1);
        assert_ne!(real_sha, "FAKE", "real SHA shouldn't collide with marker");
    }
}
