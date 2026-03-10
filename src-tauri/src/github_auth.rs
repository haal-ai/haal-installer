use std::path::PathBuf;

use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

use crate::errors::{AuthError, FileSystemError, HaalError, NetworkError};

const GITHUB_API_BASE: &str = "https://api.github.com";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_OAUTH_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_AGENT_VALUE: &str = "HAAL-Installer/0.1.0";
const CREDENTIALS_FILE: &str = "credentials.json";

/// The type of GitHub authentication used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthType {
    OAuth,
    PersonalAccessToken,
}

/// Stored GitHub credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitHubCredentials {
    pub auth_type: AuthType,
    pub token: String,
    pub enterprise_url: Option<String>,
}

/// Response from the GitHub Device Code endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Response from the GitHub OAuth token polling endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Minimal user info returned by the GitHub `/user` endpoint.
#[derive(Debug, Deserialize)]
struct GitHubUser {
    #[allow(dead_code)]
    login: String,
}

/// Handles GitHub OAuth and PAT authentication with file-based credential storage.
pub struct GitHubAuthenticator {
    http_client: reqwest::Client,
    config_dir: PathBuf,
}

impl GitHubAuthenticator {
    /// Creates a new GitHubAuthenticator.
    ///
    /// `config_dir` should point to `~/.haal/config/`.
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            config_dir,
        }
    }

    /// Creates a new GitHubAuthenticator with a custom HTTP client (useful for testing).
    #[cfg(test)]
    pub fn with_client(http_client: reqwest::Client, config_dir: PathBuf) -> Self {
        Self {
            http_client,
            config_dir,
        }
    }

    // -----------------------------------------------------------------------
    // URL helpers
    // -----------------------------------------------------------------------

    /// Returns the API base URL for the given enterprise URL, or the default GitHub.com API.
    fn api_base(enterprise_url: &Option<String>) -> String {
        match enterprise_url {
            Some(url) => {
                let base = url.trim_end_matches('/');
                format!("{base}/api/v3")
            }
            None => GITHUB_API_BASE.to_string(),
        }
    }

    /// Returns the device code URL for OAuth, supporting GitHub Enterprise.
    fn device_code_url(enterprise_url: &Option<String>) -> String {
        match enterprise_url {
            Some(url) => {
                let base = url.trim_end_matches('/');
                format!("{base}/login/device/code")
            }
            None => GITHUB_DEVICE_CODE_URL.to_string(),
        }
    }

    /// Returns the OAuth token URL, supporting GitHub Enterprise.
    fn oauth_token_url(enterprise_url: &Option<String>) -> String {
        match enterprise_url {
            Some(url) => {
                let base = url.trim_end_matches('/');
                format!("{base}/login/oauth/access_token")
            }
            None => GITHUB_OAUTH_TOKEN_URL.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // PAT authentication
    // -----------------------------------------------------------------------

    /// Authenticates with a personal access token by validating it against the
    /// GitHub API `/user` endpoint.
    ///
    /// Supports both GitHub.com and GitHub Enterprise when `enterprise_url` is
    /// provided.
    pub async fn authenticate_pat(
        &self,
        token: String,
        enterprise_url: Option<String>,
    ) -> Result<GitHubCredentials, HaalError> {
        if token.is_empty() {
            return Err(AuthError {
                message: "Personal access token cannot be empty".to_string(),
            }
            .into());
        }

        let api_base = Self::api_base(&enterprise_url);
        let url = format!("{api_base}/user");

        let response = self
            .http_client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| NetworkError {
                message: format!("Failed to reach GitHub API: {e}"),
                url: Some(url.clone()),
                status_code: None,
            })?;

        let status = response.status().as_u16();
        if status == 401 || status == 403 {
            return Err(AuthError {
                message: "Invalid or expired personal access token".to_string(),
            }
            .into());
        }
        if !response.status().is_success() {
            return Err(NetworkError {
                message: format!("GitHub API returned status {status}"),
                url: Some(url),
                status_code: Some(status),
            }
            .into());
        }

        // Ensure the response is valid JSON with a login field.
        response
            .json::<GitHubUser>()
            .await
            .map_err(|e| AuthError {
                message: format!("Unexpected response from GitHub API: {e}"),
            })?;

        Ok(GitHubCredentials {
            auth_type: AuthType::PersonalAccessToken,
            token,
            enterprise_url,
        })
    }

    // -----------------------------------------------------------------------
    // OAuth Device Flow
    // -----------------------------------------------------------------------

    /// Initiates the GitHub OAuth Device Flow.
    ///
    /// Returns a `DeviceCodeResponse` containing the `user_code` and
    /// `verification_uri` that should be displayed to the user. The caller
    /// must then poll `poll_oauth_token()` until the user authorises the
    /// device.
    pub async fn authenticate_oauth(
        &self,
        client_id: &str,
        enterprise_url: Option<String>,
    ) -> Result<DeviceCodeResponse, HaalError> {
        if client_id.is_empty() {
            return Err(AuthError {
                message: "OAuth client_id cannot be empty".to_string(),
            }
            .into());
        }

        let url = Self::device_code_url(&enterprise_url);

        let response = self
            .http_client
            .post(&url)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, USER_AGENT_VALUE)
            .form(&[("client_id", client_id), ("scope", "repo")])
            .send()
            .await
            .map_err(|e| NetworkError {
                message: format!("Failed to initiate OAuth device flow: {e}"),
                url: Some(url.clone()),
                status_code: None,
            })?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(NetworkError {
                message: format!("GitHub device code endpoint returned status {status}"),
                url: Some(url),
                status_code: Some(status),
            }
            .into());
        }

        let device_code: DeviceCodeResponse =
            response.json().await.map_err(|e| AuthError {
                message: format!("Failed to parse device code response: {e}"),
            })?;

        Ok(device_code)
    }

    /// Polls the GitHub OAuth token endpoint once. Returns `Ok(Some(creds))`
    /// when the user has authorised, `Ok(None)` when still waiting
    /// (`authorization_pending`), or an error for other failures.
    pub async fn poll_oauth_token(
        &self,
        client_id: &str,
        device_code: &str,
        enterprise_url: Option<String>,
    ) -> Result<Option<GitHubCredentials>, HaalError> {
        let url = Self::oauth_token_url(&enterprise_url);

        let response = self
            .http_client
            .post(&url)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, USER_AGENT_VALUE)
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(|e| NetworkError {
                message: format!("Failed to poll OAuth token: {e}"),
                url: Some(url.clone()),
                status_code: None,
            })?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(NetworkError {
                message: format!("GitHub OAuth token endpoint returned status {status}"),
                url: Some(url),
                status_code: Some(status),
            }
            .into());
        }

        let token_resp: OAuthTokenResponse =
            response.json().await.map_err(|e| AuthError {
                message: format!("Failed to parse OAuth token response: {e}"),
            })?;

        if let Some(access_token) = token_resp.access_token {
            return Ok(Some(GitHubCredentials {
                auth_type: AuthType::OAuth,
                token: access_token,
                enterprise_url,
            }));
        }

        match token_resp.error.as_deref() {
            Some("authorization_pending") | Some("slow_down") => Ok(None),
            Some("expired_token") => Err(AuthError {
                message: "Device code has expired. Please restart the OAuth flow.".to_string(),
            }
            .into()),
            Some("access_denied") => Err(AuthError {
                message: "User denied the OAuth authorization request.".to_string(),
            }
            .into()),
            Some(err) => Err(AuthError {
                message: format!(
                    "OAuth error: {err} — {}",
                    token_resp.error_description.unwrap_or_default()
                ),
            }
            .into()),
            None => Err(AuthError {
                message: "Unexpected OAuth response: no token and no error".to_string(),
            }
            .into()),
        }
    }

    // -----------------------------------------------------------------------
    // Repository access verification
    // -----------------------------------------------------------------------

    /// Verifies that the stored credentials have access to the given repository.
    ///
    /// `repo_url` can be a full GitHub URL like
    /// `https://github.com/owner/repo` or `https://ghe.example.com/owner/repo`.
    pub async fn verify_access(&self, repo_url: &str) -> Result<bool, HaalError> {
        let credentials = self.retrieve_credentials()?;
        let credentials = match credentials {
            Some(c) => c,
            None => {
                return Err(AuthError {
                    message: "No stored credentials found. Please authenticate first.".to_string(),
                }
                .into());
            }
        };

        let (api_base, owner, repo) = Self::parse_repo_url(repo_url)?;

        let url = format!("{api_base}/repos/{owner}/{repo}");

        let response = self
            .http_client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", credentials.token))
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| NetworkError {
                message: format!("Failed to verify repository access: {e}"),
                url: Some(url.clone()),
                status_code: None,
            })?;

        match response.status().as_u16() {
            200 => Ok(true),
            404 | 403 => Ok(false),
            status => Err(NetworkError {
                message: format!("GitHub API returned unexpected status {status}"),
                url: Some(url),
                status_code: Some(status),
            }
            .into()),
        }
    }

    /// Parses a GitHub repository URL into (api_base, owner, repo).
    ///
    /// Supports:
    /// - `https://github.com/owner/repo`
    /// - `https://github.com/owner/repo.git`
    /// - `https://ghe.example.com/owner/repo`
    fn parse_repo_url(repo_url: &str) -> Result<(String, String, String), HaalError> {
        let url = repo_url.trim_end_matches('/').trim_end_matches(".git");

        // Split on "://" then take the host and path
        let after_scheme = url
            .split("://")
            .nth(1)
            .ok_or_else(|| AuthError {
                message: format!("Invalid repository URL: {repo_url}"),
            })?;

        let parts: Vec<&str> = after_scheme.splitn(3, '/').collect();
        if parts.len() < 3 {
            return Err(AuthError {
                message: format!(
                    "Repository URL must contain owner and repo: {repo_url}"
                ),
            }
            .into());
        }

        let host = parts[0];
        let owner = parts[1].to_string();
        let repo = parts[2].to_string();

        let api_base = if host == "github.com" {
            GITHUB_API_BASE.to_string()
        } else {
            format!("https://{host}/api/v3")
        };

        Ok((api_base, owner, repo))
    }

    // -----------------------------------------------------------------------
    // Credential storage (file-based)
    // -----------------------------------------------------------------------

    /// Stores credentials as JSON at `{config_dir}/credentials.json`.
    pub fn store_credentials(&self, credentials: &GitHubCredentials) -> Result<(), HaalError> {
        std::fs::create_dir_all(&self.config_dir).map_err(|e| FileSystemError {
            message: format!("Failed to create config directory: {e}"),
            path: Some(self.config_dir.display().to_string()),
        })?;

        let path = self.config_dir.join(CREDENTIALS_FILE);
        let json = serde_json::to_string_pretty(credentials).map_err(|e| FileSystemError {
            message: format!("Failed to serialize credentials: {e}"),
            path: Some(path.display().to_string()),
        })?;

        std::fs::write(&path, &json).map_err(|e| FileSystemError {
            message: format!("Failed to write credentials file: {e}"),
            path: Some(path.display().to_string()),
        })?;

        Ok(())
    }

    /// Retrieves stored credentials from `{config_dir}/credentials.json`.
    /// Returns `Ok(None)` if no credentials file exists.
    pub fn retrieve_credentials(&self) -> Result<Option<GitHubCredentials>, HaalError> {
        let path = self.config_dir.join(CREDENTIALS_FILE);

        if !path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(&path).map_err(|e| FileSystemError {
            message: format!("Failed to read credentials file: {e}"),
            path: Some(path.display().to_string()),
        })?;

        let credentials: GitHubCredentials =
            serde_json::from_str(&json).map_err(|e| FileSystemError {
                message: format!("Failed to parse credentials file: {e}"),
                path: Some(path.display().to_string()),
            })?;

        Ok(Some(credentials))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_auth(dir: &std::path::Path) -> GitHubAuthenticator {
        GitHubAuthenticator::new(dir.to_path_buf())
    }

    fn sample_pat_credentials() -> GitHubCredentials {
        GitHubCredentials {
            auth_type: AuthType::PersonalAccessToken,
            token: "ghp_test1234567890".to_string(),
            enterprise_url: None,
        }
    }

    fn sample_oauth_credentials() -> GitHubCredentials {
        GitHubCredentials {
            auth_type: AuthType::OAuth,
            token: "gho_oauthtoken123".to_string(),
            enterprise_url: None,
        }
    }

    fn sample_enterprise_credentials() -> GitHubCredentials {
        GitHubCredentials {
            auth_type: AuthType::PersonalAccessToken,
            token: "ghp_enterprise_token".to_string(),
            enterprise_url: Some("https://ghe.example.com".to_string()),
        }
    }

    // -------------------------------------------------------------------
    // Serialization round-trip tests
    // -------------------------------------------------------------------

    #[test]
    fn test_credentials_serialization_roundtrip_pat() {
        let creds = sample_pat_credentials();
        let json = serde_json::to_string(&creds).unwrap();
        let deserialized: GitHubCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, deserialized);
    }

    #[test]
    fn test_credentials_serialization_roundtrip_oauth() {
        let creds = sample_oauth_credentials();
        let json = serde_json::to_string(&creds).unwrap();
        let deserialized: GitHubCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, deserialized);
    }

    #[test]
    fn test_credentials_serialization_roundtrip_enterprise() {
        let creds = sample_enterprise_credentials();
        let json = serde_json::to_string(&creds).unwrap();
        let deserialized: GitHubCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, deserialized);
    }

    #[test]
    fn test_auth_type_serialization() {
        let oauth_json = serde_json::to_string(&AuthType::OAuth).unwrap();
        let pat_json = serde_json::to_string(&AuthType::PersonalAccessToken).unwrap();
        assert_eq!(oauth_json, "\"OAuth\"");
        assert_eq!(pat_json, "\"PersonalAccessToken\"");
    }

    // -------------------------------------------------------------------
    // Credential storage tests
    // -------------------------------------------------------------------

    #[test]
    fn test_store_and_retrieve_credentials() {
        let tmp = TempDir::new().unwrap();
        let auth = make_auth(tmp.path());
        let creds = sample_pat_credentials();

        auth.store_credentials(&creds).unwrap();
        let retrieved = auth.retrieve_credentials().unwrap();

        assert_eq!(retrieved, Some(creds));
    }

    #[test]
    fn test_retrieve_credentials_when_none_stored() {
        let tmp = TempDir::new().unwrap();
        let auth = make_auth(tmp.path());

        let retrieved = auth.retrieve_credentials().unwrap();
        assert_eq!(retrieved, None);
    }

    #[test]
    fn test_store_credentials_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("nested").join("config");
        let auth = make_auth(&nested);

        auth.store_credentials(&sample_pat_credentials()).unwrap();

        assert!(nested.join(CREDENTIALS_FILE).exists());
    }

    #[test]
    fn test_store_credentials_overwrites_existing() {
        let tmp = TempDir::new().unwrap();
        let auth = make_auth(tmp.path());

        auth.store_credentials(&sample_pat_credentials()).unwrap();
        auth.store_credentials(&sample_oauth_credentials()).unwrap();

        let retrieved = auth.retrieve_credentials().unwrap();
        assert_eq!(retrieved, Some(sample_oauth_credentials()));
    }

    #[test]
    fn test_store_and_retrieve_enterprise_credentials() {
        let tmp = TempDir::new().unwrap();
        let auth = make_auth(tmp.path());
        let creds = sample_enterprise_credentials();

        auth.store_credentials(&creds).unwrap();
        let retrieved = auth.retrieve_credentials().unwrap();

        assert_eq!(retrieved, Some(creds));
    }

    #[test]
    fn test_retrieve_credentials_invalid_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CREDENTIALS_FILE);
        std::fs::write(&path, "not valid json").unwrap();

        let auth = make_auth(tmp.path());
        let result = auth.retrieve_credentials();

        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // URL parsing tests
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_repo_url_github_com() {
        let (api_base, owner, repo) =
            GitHubAuthenticator::parse_repo_url("https://github.com/octocat/hello-world").unwrap();
        assert_eq!(api_base, GITHUB_API_BASE);
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn test_parse_repo_url_github_com_with_git_suffix() {
        let (api_base, owner, repo) =
            GitHubAuthenticator::parse_repo_url("https://github.com/octocat/hello-world.git")
                .unwrap();
        assert_eq!(api_base, GITHUB_API_BASE);
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn test_parse_repo_url_enterprise() {
        let (api_base, owner, repo) =
            GitHubAuthenticator::parse_repo_url("https://ghe.example.com/team/project").unwrap();
        assert_eq!(api_base, "https://ghe.example.com/api/v3");
        assert_eq!(owner, "team");
        assert_eq!(repo, "project");
    }

    #[test]
    fn test_parse_repo_url_trailing_slash() {
        let (_, owner, repo) =
            GitHubAuthenticator::parse_repo_url("https://github.com/octocat/hello-world/")
                .unwrap();
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn test_parse_repo_url_invalid_no_scheme() {
        let result = GitHubAuthenticator::parse_repo_url("github.com/octocat/hello-world");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_repo_url_invalid_no_repo() {
        let result = GitHubAuthenticator::parse_repo_url("https://github.com/octocat");
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // API base URL helper tests
    // -------------------------------------------------------------------

    #[test]
    fn test_api_base_github_com() {
        assert_eq!(
            GitHubAuthenticator::api_base(&None),
            GITHUB_API_BASE
        );
    }

    #[test]
    fn test_api_base_enterprise() {
        let enterprise = Some("https://ghe.example.com".to_string());
        assert_eq!(
            GitHubAuthenticator::api_base(&enterprise),
            "https://ghe.example.com/api/v3"
        );
    }

    #[test]
    fn test_api_base_enterprise_trailing_slash() {
        let enterprise = Some("https://ghe.example.com/".to_string());
        assert_eq!(
            GitHubAuthenticator::api_base(&enterprise),
            "https://ghe.example.com/api/v3"
        );
    }

    #[test]
    fn test_device_code_url_github_com() {
        assert_eq!(
            GitHubAuthenticator::device_code_url(&None),
            GITHUB_DEVICE_CODE_URL
        );
    }

    #[test]
    fn test_device_code_url_enterprise() {
        let enterprise = Some("https://ghe.example.com".to_string());
        assert_eq!(
            GitHubAuthenticator::device_code_url(&enterprise),
            "https://ghe.example.com/login/device/code"
        );
    }

    #[test]
    fn test_oauth_token_url_github_com() {
        assert_eq!(
            GitHubAuthenticator::oauth_token_url(&None),
            GITHUB_OAUTH_TOKEN_URL
        );
    }

    #[test]
    fn test_oauth_token_url_enterprise() {
        let enterprise = Some("https://ghe.example.com".to_string());
        assert_eq!(
            GitHubAuthenticator::oauth_token_url(&enterprise),
            "https://ghe.example.com/login/oauth/access_token"
        );
    }

    // -------------------------------------------------------------------
    // PAT validation edge cases (no network)
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_authenticate_pat_empty_token() {
        let tmp = TempDir::new().unwrap();
        let auth = make_auth(tmp.path());

        let result = auth.authenticate_pat("".to_string(), None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            HaalError::Auth(e) => assert!(e.message.contains("cannot be empty")),
            other => panic!("Expected AuthError, got: {other:?}"),
        }
    }

    // -------------------------------------------------------------------
    // OAuth edge cases (no network)
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_authenticate_oauth_empty_client_id() {
        let tmp = TempDir::new().unwrap();
        let auth = make_auth(tmp.path());

        let result = auth.authenticate_oauth("", None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            HaalError::Auth(e) => assert!(e.message.contains("client_id cannot be empty")),
            other => panic!("Expected AuthError, got: {other:?}"),
        }
    }

    // -------------------------------------------------------------------
    // DeviceCodeResponse serialization
    // -------------------------------------------------------------------

    #[test]
    fn test_device_code_response_serialization() {
        let resp = DeviceCodeResponse {
            device_code: "dc_123".to_string(),
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            expires_in: 900,
            interval: 5,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: DeviceCodeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.device_code, "dc_123");
        assert_eq!(deserialized.user_code, "ABCD-1234");
        assert_eq!(deserialized.expires_in, 900);
        assert_eq!(deserialized.interval, 5);
    }
}
