use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{FileSystemError, HaalError, IntegrityError};

/// Verifies file integrity using SHA-256 checksums.
pub struct ChecksumValidator;

/// Size of chunks used when reading files for checksum calculation (8 KiB).
const CHUNK_SIZE: usize = 8192;

impl ChecksumValidator {
    /// Creates a new ChecksumValidator.
    pub fn new() -> Self {
        Self
    }

    /// Calculates the SHA-256 checksum for a file.
    ///
    /// Reads the file in chunks for memory efficiency and returns
    /// the hex-encoded SHA-256 digest.
    pub fn calculate_checksum(&self, path: &Path) -> Result<String, HaalError> {
        use std::io::Read;

        let mut file = std::fs::File::open(path).map_err(|e| {
            FileSystemError {
                message: format!("Failed to open file for checksum: {e}"),
                path: Some(path.display().to_string()),
            }
        })?;

        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; CHUNK_SIZE];

        loop {
            let bytes_read = file.read(&mut buffer).map_err(|e| {
                FileSystemError {
                    message: format!("Failed to read file for checksum: {e}"),
                    path: Some(path.display().to_string()),
                }
            })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        Ok(hash.iter().map(|b| format!("{:02x}", b)).collect())
    }

    /// Verifies that a file matches the expected checksum.
    ///
    /// Returns `true` when the calculated SHA-256 digest equals `expected`
    /// (case-insensitive comparison).
    pub fn verify_checksum(&self, path: &Path, expected: &str) -> Result<bool, HaalError> {
        let actual = self.calculate_checksum(path)?;
        Ok(actual.eq_ignore_ascii_case(expected))
    }

    /// Calculates checksums for multiple files in parallel using tokio tasks.
    ///
    /// Each file is processed in a `spawn_blocking` task since file I/O is
    /// blocking. Returns a map from path to hex-encoded checksum.
    pub async fn calculate_checksums_parallel(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<HashMap<PathBuf, String>, HaalError> {
        let mut handles = Vec::with_capacity(paths.len());

        for path in paths {
            let handle = tokio::task::spawn_blocking(move || {
                let validator = ChecksumValidator::new();
                let checksum = validator.calculate_checksum(&path)?;
                Ok::<(PathBuf, String), HaalError>((path, checksum))
            });
            handles.push(handle);
        }

        let mut results = HashMap::new();
        for handle in handles {
            let (path, checksum) = handle.await.map_err(|e| {
                IntegrityError {
                    message: format!("Parallel checksum task failed: {e}"),
                    expected: None,
                    actual: None,
                }
            })??;
            results.insert(path, checksum);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_file(content: &[u8]) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(content).expect("failed to write temp file");
        file.flush().expect("failed to flush temp file");
        file
    }

    /// Known SHA-256 of an empty file.
    const EMPTY_SHA256: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn calculate_checksum_empty_file() {
        let file = create_temp_file(b"");
        let validator = ChecksumValidator::new();
        let checksum = validator.calculate_checksum(file.path()).unwrap();
        assert_eq!(checksum, EMPTY_SHA256);
    }

    #[test]
    fn calculate_checksum_known_content() {
        // SHA-256("hello world") is well-known
        let file = create_temp_file(b"hello world");
        let validator = ChecksumValidator::new();
        let checksum = validator.calculate_checksum(file.path()).unwrap();
        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn calculate_checksum_nonexistent_file() {
        let validator = ChecksumValidator::new();
        let result = validator.calculate_checksum(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn verify_checksum_matching() {
        let file = create_temp_file(b"hello world");
        let validator = ChecksumValidator::new();
        let ok = validator
            .verify_checksum(
                file.path(),
                "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            )
            .unwrap();
        assert!(ok);
    }

    #[test]
    fn verify_checksum_case_insensitive() {
        let file = create_temp_file(b"hello world");
        let validator = ChecksumValidator::new();
        let ok = validator
            .verify_checksum(
                file.path(),
                "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9",
            )
            .unwrap();
        assert!(ok);
    }

    #[test]
    fn verify_checksum_mismatch() {
        let file = create_temp_file(b"hello world");
        let validator = ChecksumValidator::new();
        let ok = validator
            .verify_checksum(file.path(), "0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap();
        assert!(!ok);
    }

    #[test]
    fn calculate_checksum_large_file() {
        // Create a file larger than CHUNK_SIZE to exercise chunked reading
        let data = vec![0xABu8; CHUNK_SIZE * 3 + 42];
        let file = create_temp_file(&data);
        let validator = ChecksumValidator::new();
        let checksum = validator.calculate_checksum(file.path()).unwrap();
        // Just verify it returns a valid 64-char hex string
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn calculate_checksums_parallel_multiple_files() {
        let f1 = create_temp_file(b"file one");
        let f2 = create_temp_file(b"file two");
        let f3 = create_temp_file(b"file three");

        let paths = vec![
            f1.path().to_path_buf(),
            f2.path().to_path_buf(),
            f3.path().to_path_buf(),
        ];

        let validator = ChecksumValidator::new();
        let results = validator.calculate_checksums_parallel(paths.clone()).await.unwrap();

        assert_eq!(results.len(), 3);
        // Each result should match the sequential calculation
        for path in &paths {
            let expected = validator.calculate_checksum(path).unwrap();
            assert_eq!(results[path], expected);
        }
    }

    #[tokio::test]
    async fn calculate_checksums_parallel_empty_list() {
        let validator = ChecksumValidator::new();
        let results = validator.calculate_checksums_parallel(vec![]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn calculate_checksums_parallel_with_missing_file() {
        let f1 = create_temp_file(b"valid");
        let paths = vec![
            f1.path().to_path_buf(),
            PathBuf::from("/nonexistent/missing.txt"),
        ];

        let validator = ChecksumValidator::new();
        let result = validator.calculate_checksums_parallel(paths).await;
        assert!(result.is_err());
    }
}
