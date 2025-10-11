use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AdrMeta {
    pub number: u32,
    pub title: String,
    pub status: String,
    pub date: String,
    pub supersedes: Option<u32>,
    pub superseded_by: Option<u32>,
    pub path: PathBuf,
}

pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if (c.is_ascii_whitespace() || c == '-' || c == '_') && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    if out.is_empty() {
        "adr".to_string()
    } else {
        out
    }
}

pub fn parse_number(s: &str) -> anyhow::Result<u32> {
    let s = s.trim();
    let s = s.trim_start_matches('0');
    if s.is_empty() {
        Ok(0)
    } else {
        Ok(s.parse::<u32>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("  Multiple   Spaces  "), "multiple-spaces");
        assert_eq!(slugify("Caps_and-Dashes"), "caps-and-dashes");
        assert_eq!(slugify("@#Weird!! Title??"), "weird-title");
        assert_eq!(slugify(""), "adr");
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_number("0003").unwrap(), 3);
        assert_eq!(parse_number("3").unwrap(), 3);
        assert_eq!(parse_number("0000").unwrap(), 0);
    }
}
