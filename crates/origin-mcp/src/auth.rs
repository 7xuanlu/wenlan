use std::net::IpAddr;
use subtle::ConstantTimeEq;

pub fn verify_token(provided: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    // Fixed-size buffer comparison to eliminate length timing oracle.
    // Tokens are base64url-encoded 256-bit values (~43 chars).
    const BUF_SIZE: usize = 136; // 128 content + 8 length
    let pb = provided.as_bytes();
    let eb = expected.as_bytes();
    if pb.len() > 128 || eb.len() > 128 {
        return false;
    }
    let mut p = [0u8; BUF_SIZE];
    let mut e = [0u8; BUF_SIZE];
    p[..pb.len()].copy_from_slice(pb);
    e[..eb.len()].copy_from_slice(eb);
    p[128..].copy_from_slice(&(pb.len() as u64).to_le_bytes());
    e[128..].copy_from_slice(&(eb.len() as u64).to_le_bytes());
    p.ct_eq(&e).into()
}

pub fn is_loopback(addr: &IpAddr) -> bool {
    addr.is_loopback()
}

pub fn is_origin_allowed(origin: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| a == "*" || a == origin)
}

pub fn extract_bearer_token(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_token_matching() {
        assert!(verify_token("abc123", "abc123"));
    }

    #[test]
    fn test_verify_token_different() {
        assert!(!verify_token("abc123", "xyz789"));
    }

    #[test]
    fn test_verify_token_different_lengths() {
        assert!(!verify_token("short", "much-longer-token"));
    }

    #[test]
    fn test_verify_token_empty_expected_always_false() {
        assert!(!verify_token("", ""));
        assert!(!verify_token("", "notempty"));
        assert!(!verify_token("anything", ""));
    }

    #[test]
    fn test_loopback_ipv4() {
        let addr: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(is_loopback(&addr));
    }

    #[test]
    fn test_loopback_ipv6() {
        let addr: IpAddr = "::1".parse().unwrap();
        assert!(is_loopback(&addr));
    }

    #[test]
    fn test_not_loopback_0000() {
        let addr: IpAddr = "0.0.0.0".parse().unwrap();
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn test_not_loopback_ipv6_unspecified() {
        let addr: IpAddr = "::".parse().unwrap();
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn test_not_loopback_lan() {
        let addr: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn test_origin_allowed_exact_match() {
        let allowed = vec!["https://claude.ai".into(), "https://chatgpt.com".into()];
        assert!(is_origin_allowed("https://claude.ai", &allowed));
    }

    #[test]
    fn test_origin_not_allowed() {
        let allowed = vec!["https://claude.ai".into()];
        assert!(!is_origin_allowed("https://evil.com", &allowed));
    }

    #[test]
    fn test_origin_wildcard_allows_all() {
        let allowed = vec!["*".into()];
        assert!(is_origin_allowed("https://anything.com", &allowed));
    }

    #[test]
    fn test_origin_empty_list_denies_all() {
        let allowed: Vec<String> = vec![];
        assert!(!is_origin_allowed("https://claude.ai", &allowed));
    }

    #[test]
    fn test_extract_bearer_valid() {
        assert_eq!(
            extract_bearer_token("Bearer mytoken123"),
            Some("mytoken123")
        );
    }

    #[test]
    fn test_extract_bearer_no_prefix() {
        assert_eq!(extract_bearer_token("mytoken123"), None);
    }

    #[test]
    fn test_extract_bearer_wrong_scheme() {
        assert_eq!(extract_bearer_token("Basic abc123"), None);
    }

    #[test]
    fn test_extract_bearer_case_sensitive() {
        assert_eq!(extract_bearer_token("bearer mytoken"), None);
    }

    #[test]
    fn test_extract_bearer_empty_token() {
        assert_eq!(extract_bearer_token("Bearer "), Some(""));
    }
}
