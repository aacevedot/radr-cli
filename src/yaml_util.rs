pub fn escape_yaml(input: &str) -> String {
    // For simple front matter, quote if contains special chars or starts with digit
    let needs_quotes = input.chars().any(|c| c == ':' || c == '"' || c == '\'')
        || input.starts_with(|c: char| c.is_ascii_digit());
    if needs_quotes {
        let escaped = input.replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_when_contains_colon() {
        assert_eq!(escape_yaml("Title: With Colon"), "\"Title: With Colon\"");
    }

    #[test]
    fn quotes_when_starts_with_digit() {
        assert_eq!(escape_yaml("123 Plan"), "\"123 Plan\"");
    }

    #[test]
    fn escapes_double_quotes_inside() {
        assert_eq!(
            escape_yaml("He said \"Hello\""),
            "\"He said \\\"Hello\\\"\""
        );
    }

    #[test]
    fn leaves_simple_text_unquoted() {
        assert_eq!(escape_yaml("simple title"), "simple title");
    }
}
