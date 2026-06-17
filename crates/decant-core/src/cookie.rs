//! Cookie parsing and representation.

/// Represents a single cookie.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Cookie {
    /// Name of the cookie.
    pub name: String,
    /// Value of the cookie.
    pub value: String,
    /// Domain scope of the cookie.
    pub domain: String,
}

/// Parse a raw cookie header/string "name=val; name2=val2".
///
/// Since inline cookie strings do not have domain info, the returned cookies
/// will have an empty string for their domain. The caller should populate it if needed.
pub fn parse_cookie_str(s: &str) -> Vec<Cookie> {
    let mut cookies = Vec::new();
    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = part.find('=') {
            let name = part[..idx].trim();
            let value = part[idx + 1..].trim(); // everything after first '=' is the value
            if !name.is_empty() {
                cookies.push(Cookie {
                    name: name.to_string(),
                    value: value.to_string(),
                    domain: String::new(),
                });
            }
        }
    }
    cookies
}

/// Parse Netscape cookie jar file format.
///
/// Ignores comments and empty lines. Each line must be tab-separated with 7 fields.
pub fn parse_netscape_file(content: &str) -> Vec<Cookie> {
    let mut cookies = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 7 {
            let domain = fields[0].trim().to_string();
            let name = fields[5].trim().to_string();
            let value = fields[6].trim().to_string();
            cookies.push(Cookie {
                name,
                value,
                domain,
            });
        }
    }
    cookies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cookie_str() {
        let s = "session=abc123; foo=bar; ; empty_val=";
        let cookies = parse_cookie_str(s);
        assert_eq!(cookies.len(), 3);
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[0].value, "abc123");
        assert_eq!(cookies[0].domain, "");
        assert_eq!(cookies[1].name, "foo");
        assert_eq!(cookies[1].value, "bar");
        assert_eq!(cookies[2].name, "empty_val");
        assert_eq!(cookies[2].value, "");
    }

    #[test]
    fn cookie_value_with_equals_sign_preserved() {
        let cookies = parse_cookie_str("token=abc=def=xyz");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "token");
        assert_eq!(cookies[0].value, "abc=def=xyz");
    }

    #[test]
    fn empty_cookie_string_returns_empty_vec() {
        let cookies = parse_cookie_str("");
        assert!(cookies.is_empty());
    }

    #[test]
    fn whitespace_only_cookie_string_returns_empty_vec() {
        let cookies = parse_cookie_str("   ");
        assert!(cookies.is_empty());
    }

    #[test]
    fn test_parse_netscape_file() {
        let content = r#"# Netscape HTTP Cookie File
# http://curl.haxx.se/rfc/cookie_spec.html
# This is a generated file!  Do not edit.

.example.com	TRUE	/	FALSE	1774358400	session_id	xyz987
example.com	FALSE	/sub	TRUE	1774358400	foo	bar
"#;
        let cookies = parse_netscape_file(content);
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].domain, ".example.com");
        assert_eq!(cookies[0].name, "session_id");
        assert_eq!(cookies[0].value, "xyz987");
        assert_eq!(cookies[1].domain, "example.com");
        assert_eq!(cookies[1].name, "foo");
        assert_eq!(cookies[1].value, "bar");
    }

    #[test]
    fn netscape_malformed_lines_are_skipped() {
        // Lines with fewer than 7 tab-separated fields must be silently skipped.
        let content = "only\ttwo\tfields\n.example.com\tTRUE\t/\tFALSE\t1774358400\tok\tval\n";
        let cookies = parse_netscape_file(content);
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "ok");
    }
}
