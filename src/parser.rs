use semver::Version;

#[derive(Debug, Clone)]
pub struct OutdatedPackage {
    pub name: String,
    pub old_version: Version,
    pub new_version: Version,
}

pub fn parse_cargo_search_output(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    let (_, rest) = line.split_once(" = \"")?;
    let (version, _) = rest.split_once('"')?;
    Some(version.to_string())
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
    fn test_parse_cargo_search_output() {
        let output = r#"serde = "1.0.210" # A generic serialization/deserialization framework"#;
        assert_eq!(
            parse_cargo_search_output(output),
            Some("1.0.210".to_string())
        );
    }

    #[test]
    fn test_parse_cargo_search_output_empty() {
        assert_eq!(parse_cargo_search_output(""), None);
    }

    #[test]
    fn test_parse_cargo_search_output_no_quotes() {
        assert_eq!(parse_cargo_search_output("no version here"), None);
    }

    #[test]
    fn test_parse_cargo_search_output_no_equals_quote() {
        // Has quotes but not in the expected format
        assert_eq!(parse_cargo_search_output(r#"foo "1.0.0""#), None);
    }

    #[test]
    fn test_parse_cargo_search_output_multiline() {
        let output = "serde = \"1.0.210\" # description\nserde_derive = \"1.0.210\"";
        assert_eq!(
            parse_cargo_search_output(output),
            Some("1.0.210".to_string())
        );
    }

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
