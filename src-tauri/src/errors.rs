use std::fmt;

/// Network-related errors (connectivity, timeouts, HTTP failures).
#[derive(Debug, Clone)]
pub struct NetworkError {
    pub message: String,
    pub url: Option<String>,
    pub status_code: Option<u16>,
}

/// Authentication errors (invalid credentials, expired tokens).
#[derive(Debug, Clone)]
pub struct AuthError {
    pub message: String,
}

/// File-system errors (permissions, missing paths, disk full).
#[derive(Debug, Clone)]
pub struct FileSystemError {
    pub message: String,
    pub path: Option<String>,
}

/// Validation errors (invalid input, schema violations).
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub field: Option<String>,
}

/// Integrity errors (checksum mismatches, corrupted files).
#[derive(Debug, Clone)]
pub struct IntegrityError {
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// Conflict errors (file already exists, version mismatch).
#[derive(Debug, Clone)]
pub struct ConflictError {
    pub message: String,
    pub component_id: Option<String>,
}

/// Unified error type for the HAAL Installer.
///
/// Categorises every error the application can produce so the frontend
/// can display user-friendly messages and suggest recovery actions.
#[derive(Debug, Clone)]
pub enum HaalError {
    Network(NetworkError),
    Auth(AuthError),
    FileSystem(FileSystemError),
    Validation(ValidationError),
    Integrity(IntegrityError),
    Conflict(ConflictError),
}

// ---------------------------------------------------------------------------
// Display implementations
// ---------------------------------------------------------------------------

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Network error: {}", self.message)
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Authentication error: {}", self.message)
    }
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "File system error: {}", self.message)
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Validation error: {}", self.message)
    }
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Integrity error: {}", self.message)
    }
}

impl fmt::Display for ConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Conflict error: {}", self.message)
    }
}

impl fmt::Display for HaalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HaalError::Network(e) => write!(f, "{e}"),
            HaalError::Auth(e) => write!(f, "{e}"),
            HaalError::FileSystem(e) => write!(f, "{e}"),
            HaalError::Validation(e) => write!(f, "{e}"),
            HaalError::Integrity(e) => write!(f, "{e}"),
            HaalError::Conflict(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NetworkError {}
impl std::error::Error for AuthError {}
impl std::error::Error for FileSystemError {}
impl std::error::Error for ValidationError {}
impl std::error::Error for IntegrityError {}
impl std::error::Error for ConflictError {}
impl std::error::Error for HaalError {}

// ---------------------------------------------------------------------------
// Convenient From conversions
// ---------------------------------------------------------------------------

impl From<NetworkError> for HaalError {
    fn from(e: NetworkError) -> Self {
        HaalError::Network(e)
    }
}

impl From<AuthError> for HaalError {
    fn from(e: AuthError) -> Self {
        HaalError::Auth(e)
    }
}

impl From<FileSystemError> for HaalError {
    fn from(e: FileSystemError) -> Self {
        HaalError::FileSystem(e)
    }
}

impl From<ValidationError> for HaalError {
    fn from(e: ValidationError) -> Self {
        HaalError::Validation(e)
    }
}

impl From<IntegrityError> for HaalError {
    fn from(e: IntegrityError) -> Self {
        HaalError::Integrity(e)
    }
}

impl From<ConflictError> for HaalError {
    fn from(e: ConflictError) -> Self {
        HaalError::Conflict(e)
    }
}

impl From<std::io::Error> for HaalError {
    fn from(e: std::io::Error) -> Self {
        HaalError::FileSystem(FileSystemError {
            message: e.to_string(),
            path: None,
        })
    }
}
