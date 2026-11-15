pub fn slugify(input: &str) -> String {
    let ascii = deunicode::deunicode(input).to_lowercase();
    let mut out = String::with_capacity(ascii.len());
    let mut pending_hyphen = false;
    for ch in ascii.chars() {
        if matches!(ch, '\'' | '`' | '\u{2019}') {
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.push(ch);
        } else {
            pending_hyphen = true;
        }
    }
    out
}

pub fn is_slug(value: &str) -> bool {
    !value.is_empty() && slugify(value) == value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_examples() {
        assert_eq!(slugify("Jay's Penthouse"), "jays-penthouse");
        assert_eq!(slugify("Night Club 59"), "night-club-59");
        assert_eq!(
            slugify("The Café on Shishman Street"),
            "the-cafe-on-shishman-street"
        );
        assert_eq!(slugify("  --Mrs. Petrova!! "), "mrs-petrova");
        assert_eq!(slugify("Thea’s Flat"), "theas-flat");
    }

    #[test]
    fn slug_check() {
        assert!(is_slug("the-cafe"));
        assert!(!is_slug("The Cafe"));
        assert!(!is_slug(""));
    }
}
