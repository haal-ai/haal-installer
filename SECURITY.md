# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.2.x   | Yes       |
| < 0.2   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability in HAAL Installer, please report it responsibly:

1. **Do NOT open a public GitHub issue** for security vulnerabilities.
2. Use [GitHub Security Advisories](https://github.com/haal-ai/haal-installer/security/advisories/new) to privately report the vulnerability.
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We will acknowledge receipt within 48 hours and provide a timeline for a fix.

## Security Measures

- **CodeQL** static analysis runs on every push and PR to `master`
- **Dependabot** monitors npm, Cargo, and GitHub Actions dependencies weekly
- **Cargo audit** and **npm audit** run on every push and PR
- **Dependency review** gates PRs that introduce high-severity vulnerabilities
- Release binaries are signed with a Tauri signing key
