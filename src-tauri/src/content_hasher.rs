use std::path::Path;
use sha2::{Digest, Sha256};

/// Computes a deterministic SHA-256 hex digest of a path.
/// - For a file: hashes its contents.
/// - For a directory: recursively hashes all file contents, sorted by relative path,
///   so the result is stable regardless of filesystem ordering.
pub fn hash_path(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let mut hasher = Sha256::new();
    if path.is_file() {
        hash_file_into(path, &mut hasher)?;
    } else if path.is_dir() {
        hash_dir_into(path, path, &mut hasher)?;
    }
    Some(format!("{:x}", hasher.finalize()))
}

fn hash_file_into(path: &Path, hasher: &mut Sha256) -> Option<()> {
    let bytes = std::fs::read(path).ok()?;
    hasher.update(&bytes);
    Some(())
}

fn hash_dir_into(root: &Path, dir: &Path, hasher: &mut Sha256) -> Option<()> {
    // Collect all entries recursively, sorted for determinism
    let mut entries = collect_files(dir);
    entries.sort();

    for abs_path in entries {
        // Include the relative path in the hash so renames are detected
        let rel = abs_path.strip_prefix(root).ok()?;
        hasher.update(rel.to_string_lossy().as_bytes());
        hash_file_into(&abs_path, hasher)?;
    }
    Some(())
}

fn collect_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                files.push(p);
            } else if p.is_dir() {
                files.extend(collect_files(&p));
            }
        }
    }
    files
}
