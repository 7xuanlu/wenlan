use std::path::Path;

pub fn generate_token() -> String {
    use base64::Engine;
    use rand::RngExt;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn write_token(path: &Path, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn read_token(path: &Path) -> anyhow::Result<String> {
    let content = std::fs::read_to_string(path)?;
    Ok(content.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_generate_token_is_43_chars_base64url() {
        let token = generate_token();
        assert_eq!(token.len(), 43, "token should be 43 base64url chars");
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "token should only contain base64url chars: {token}"
        );
    }

    #[test]
    fn test_generate_token_is_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2, "two generated tokens should differ");
    }

    #[test]
    fn test_write_and_read_token_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-token");
        let token = "test-token-value-abc123";
        write_token(&path, token).unwrap();
        let read_back = read_token(&path).unwrap();
        assert_eq!(read_back, token);
    }

    #[test]
    fn test_write_token_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("token");
        let token = "abc";
        write_token(&path, token).unwrap();
        assert!(path.exists());
        assert_eq!(read_token(&path).unwrap(), token);
    }

    #[test]
    fn test_read_token_trims_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "  my-token-value  \n").unwrap();
        let token = read_token(&path).unwrap();
        assert_eq!(token, "my-token-value");
    }

    #[test]
    fn test_read_token_missing_file_errors() {
        let result = read_token(Path::new("/nonexistent/path/token"));
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_write_token_sets_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        write_token(&path, "secret").unwrap();
        let perms = fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600, "token file should be 0600");
    }
}
