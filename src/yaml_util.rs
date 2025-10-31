pub fn escape_yaml(input: &str) -> String {
    // Quote if contains special chars or starts with digit, but allow Windows drive paths like C:\foo
    let is_windows_drive_path = input.len() >= 3
        && input.as_bytes()[0].is_ascii_alphabetic()
        && input.as_bytes()[1] == b':'
        && (input.as_bytes()[2] == b'\\' || input.as_bytes()[2] == b'/');

    let contains_colon = input.contains(':');
    let contains_double_quote = input.contains('"');
    let contains_single_quote = input.contains("'");
    let starts_with_digit = input.chars().next().map_or(false, |c| c.is_ascii_digit());

    let needs_quotes = (contains_colon && !is_windows_drive_path)
        || contains_double_quote
        || contains_single_quote
        || starts_with_digit;

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

    #[test]
    fn quotes_when_contains_single_quote() {
        assert_eq!(escape_yaml("Bob's Book"), "\"Bob's Book\"");
    }

    #[test]
    fn quotes_when_contains_colon_without_space() {
        assert_eq!(escape_yaml("key:value"), "\"key:value\"");
    }

    #[test]
    fn quotes_when_contains_multiple_colons() {
        assert_eq!(escape_yaml("time 10:30:20"), "\"time 10:30:20\"");
    }

    #[test]
    fn quotes_date_like_string() {
        assert_eq!(escape_yaml("2024-10-31"), "\"2024-10-31\"");
    }

    #[test]
    fn quotes_when_starts_with_single_digit() {
        assert_eq!(escape_yaml("9"), "\"9\"");
    }

    #[test]
    fn leaves_boolean_like_words_unquoted() {
        for s in [
            "true", "false", "True", "False", "yes", "no", "null", "Null", "on", "off",
        ] {
            assert_eq!(escape_yaml(s), s);
        }
    }

    #[test]
    fn leaves_hash_start_unquoted() {
        assert_eq!(escape_yaml("#hashtag"), "#hashtag");
    }

    #[test]
    fn leaves_empty_string_unquoted() {
        assert_eq!(escape_yaml(""), "");
    }

    #[test]
    fn leaves_whitespace_only_unquoted() {
        assert_eq!(escape_yaml("   "), "   ");
    }

    #[test]
    fn leaves_leading_and_trailing_spaces_unquoted() {
        assert_eq!(escape_yaml("  title  "), "  title  ");
    }

    #[test]
    fn leaves_multiline_unquoted() {
        let input = "line1\nline2";
        assert_eq!(escape_yaml(input), input);
    }

    #[test]
    fn leaves_backslashes_unquoted() {
        let input = r"C:\\Path\\File";
        assert_eq!(escape_yaml(input), input);
    }

    #[test]
    fn handles_both_single_and_double_quotes() {
        let input = "He said \"Hello\" and it's Bob's";
        let expected = "\"He said \\\"Hello\\\" and it's Bob's\"";
        assert_eq!(escape_yaml(input), expected);
    }

    #[test]
    fn already_quoted_input_gets_wrapped_and_escaped() {
        let input = "\"foo\""; // literal string with quotes at both ends
        let expected = "\"\\\"foo\\\"\"";
        assert_eq!(escape_yaml(input), expected);
    }

    #[test]
    fn unicode_is_left_unquoted() {
        let input = "Café ☕ – naïve résumé";
        assert_eq!(escape_yaml(input), input);
    }

    #[test]
    fn does_not_quote_when_starts_with_hyphen() {
        assert_eq!(escape_yaml("- item"), "- item");
    }

    #[test]
    fn curly_quotes_do_not_trigger_quoting() {
        let input = "“Hello”"; // curly unicode quotes
        assert_eq!(escape_yaml(input), input);
    }
}
