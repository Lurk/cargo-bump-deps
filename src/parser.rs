use semver::Version;

#[derive(Debug, Clone)]
pub struct OutdatedPackage {
    pub name: String,
    pub old_version: Version,
    pub new_version: Version,
}

pub fn version_from_req(req: &str) -> String {
    if let Ok(version_req) = semver::VersionReq::parse(req)
        && let Some(comp) = version_req.comparators.first()
    {
        let major = comp.major;
        let minor = comp.minor.unwrap_or(0);
        let patch = comp.patch.unwrap_or(0);
        if comp.pre.is_empty() {
            return format!("{}.{}.{}", major, minor, patch);
        }
        return format!("{}.{}.{}-{}", major, minor, patch, comp.pre);
    }
    // Fallback: manual parsing for non-standard formats
    let first = req.split(',').next().unwrap_or(req);
    let version = first
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .trim();
    if version.is_empty() {
        return "0.0.0".to_string();
    }
    let dot_count = version.chars().filter(|&c| c == '.').count();
    match dot_count {
        0 => format!("{}.0.0", version),
        1 => format!("{}.0", version),
        _ => version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_from_req_caret() {
        assert_eq!(version_from_req("^1.0.100"), "1.0.100");
    }

    #[test]
    fn test_version_from_req_tilde() {
        assert_eq!(version_from_req("~1.2.3"), "1.2.3");
    }

    #[test]
    fn test_version_from_req_bare() {
        assert_eq!(version_from_req("1.0.0"), "1.0.0");
    }

    #[test]
    fn test_version_from_req_gte() {
        assert_eq!(version_from_req(">=0.5.0"), "0.5.0");
    }

    #[test]
    fn test_version_from_req_compound_gte_lt() {
        assert_eq!(version_from_req(">=1.0.0, <2.0.0"), "1.0.0");
    }

    #[test]
    fn test_version_from_req_compound_caret() {
        assert_eq!(version_from_req("^1.0, <1.5"), "1.0.0");
    }

    #[test]
    fn test_version_from_req_compound_spaces() {
        assert_eq!(version_from_req(">=0.3.0 , <0.4.0"), "0.3.0");
    }

    #[test]
    fn test_version_from_req_major_only() {
        assert_eq!(version_from_req("1"), "1.0.0");
    }

    #[test]
    fn test_version_from_req_major_minor_only() {
        assert_eq!(version_from_req("^1.2"), "1.2.0");
    }

    #[test]
    fn test_version_from_req_exact() {
        assert_eq!(version_from_req("=1.2.3"), "1.2.3");
    }

    #[test]
    fn test_version_from_req_wildcard() {
        assert_eq!(version_from_req("*"), "0.0.0");
    }

    #[test]
    fn test_version_from_req_prerelease() {
        assert_eq!(version_from_req("^1.0.0-alpha.1"), "1.0.0-alpha.1");
    }

    #[test]
    fn test_version_from_req_prerelease_bare() {
        assert_eq!(version_from_req("1.0.0-beta.2"), "1.0.0-beta.2");
    }

    #[test]
    fn test_version_from_req_le() {
        assert_eq!(version_from_req("<=2.0.0"), "2.0.0");
    }

    #[test]
    fn test_version_from_req_lt() {
        assert_eq!(version_from_req("<3.0.0"), "3.0.0");
    }
}
