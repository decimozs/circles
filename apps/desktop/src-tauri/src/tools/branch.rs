//! Conventional branch names: `type/short-description`, in lowercase words joined by dashes. A
//! Composer always names its branches this way, so a branch says what it is for.

/// The types a branch may start with, the same ones commits use.
pub const TYPES: &[&str] = &[
    "feat", "fix", "chore", "docs", "refactor", "test", "perf", "build", "ci", "style", "revert",
];

const LONGEST: usize = 60;

/// The example shown with every refusal, so the model can correct itself.
pub const EXAMPLE: &str = "feat/add-dark-mode-toggle";

/// Checks that a name is `type/words-like-this`. The error says what is wrong.
pub fn check_conventional(name: &str) -> Result<(), String> {
    let fail = |why: &str| {
        Err(format!(
            "\"{name}\" is not a conventional branch name: {why}. Use `type/short-description` with a type of {}, for example {EXAMPLE}.",
            TYPES.join(", ")
        ))
    };
    let Some((kind, slug)) = name.split_once('/') else {
        return fail("it has no type before a slash");
    };
    if !TYPES.contains(&kind) {
        return fail("the type is not one of the usual ones");
    }
    if slug.is_empty() {
        return fail("nothing follows the type");
    }
    if name.chars().count() > LONGEST {
        return fail(&format!("it is longer than {LONGEST} characters"));
    }
    if slug.contains('/') {
        return fail("it has more than one slash");
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return fail("the description uses only lowercase letters, numbers and dashes");
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return fail("the dashes must sit between words");
    }
    Ok(())
}

/// The rule a project without its own uses.
pub const DEFAULT_PATTERN: &str = "{type}/{slug}";

const LONGEST_CUSTOM: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Lit(String),
    Type,
    Slug,
    Ticket,
}

/// Reads a pattern such as `{ticket}-{slug}` or `feature/{type}/{slug}`. `{type}` is one of the
/// usual types, `{slug}` lowercase words joined by dashes, `{ticket}` an issue key like PROJ-123.
fn parse(pattern: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        lit.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            return Err("a { is never closed".into());
        };
        let name = &rest[open + 1..open + close];
        let tok = match name {
            "type" => Tok::Type,
            "slug" => Tok::Slug,
            "ticket" => Tok::Ticket,
            other => {
                return Err(format!(
                    "{{{other}}} is not known. Use {{type}}, {{slug}} or {{ticket}}"
                ))
            }
        };
        if !lit.is_empty() {
            out.push(Tok::Lit(std::mem::take(&mut lit)));
        }
        out.push(tok);
        rest = &rest[open + close + 1..];
    }
    lit.push_str(rest);
    if !lit.is_empty() {
        out.push(Tok::Lit(lit));
    }
    if !out.iter().any(|t| matches!(t, Tok::Slug | Tok::Ticket)) {
        return Err("it needs {slug} or {ticket}, so each name says what it is for".into());
    }
    if out
        .windows(2)
        .any(|w| !matches!(w[0], Tok::Lit(_)) && !matches!(w[1], Tok::Lit(_)))
    {
        return Err("two placeholders in a row need something between them".into());
    }
    Ok(out)
}

/// Whether a pattern can be used. The error says what is wrong.
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    if pattern
        .chars()
        .any(|c| c.is_whitespace() || c == '~' || c == '^' || c == ':')
    {
        return Err("a branch name has no spaces, ~, ^ or :".into());
    }
    parse(pattern).map(|_| ())
}

/// A name that follows the pattern, to show with a refusal and in the project's listing.
pub fn example(pattern: &str) -> String {
    pattern
        .replace("{type}", "feat")
        .replace("{slug}", "add-dark-mode-toggle")
        .replace("{ticket}", "PROJ-123")
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}

fn is_ticket(s: &str) -> bool {
    let Some((key, number)) = s.rsplit_once('-') else {
        return false;
    };
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

fn matches(tokens: &[Tok], s: &str) -> bool {
    let Some((first, rest)) = tokens.split_first() else {
        return s.is_empty();
    };
    match first {
        Tok::Lit(lit) => s
            .strip_prefix(lit.as_str())
            .is_some_and(|r| matches(rest, r)),
        Tok::Type => TYPES
            .iter()
            .any(|t| s.strip_prefix(t).is_some_and(|r| matches(rest, r))),
        Tok::Slug => (1..=s.len()).any(|n| is_slug(&s[..n]) && matches(rest, &s[n..])),
        Tok::Ticket => (3..=s.len()).any(|n| is_ticket(&s[..n]) && matches(rest, &s[n..])),
    }
}

/// Checks a name against a project's own rule, or the usual one when it has none. A rule that
/// cannot be read counts as none, so a typo in a setting never blocks all work.
pub fn check(name: &str, pattern: Option<&str>) -> Result<(), String> {
    let Some(pattern) = pattern.map(str::trim).filter(|p| !p.is_empty()) else {
        return check_conventional(name);
    };
    let Ok(tokens) = parse(pattern) else {
        return check_conventional(name);
    };
    if !name.is_ascii() || name.chars().count() > LONGEST_CUSTOM || !matches(&tokens, name) {
        return Err(format!(
            "\"{name}\" does not follow this project's branch naming rule `{pattern}`. For example: {}.",
            example(pattern)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_rule_decides_the_names() {
        let ticket = Some("{ticket}-{slug}");
        assert!(check("PROJ-123-fix-login", ticket).is_ok());
        assert!(check("ab2-7-x", ticket).is_ok());
        for bad in [
            "fix-login",
            "PROJ-fix-login",
            "proj-123-",
            "PROJ-123-Fix",
            "feat/x",
        ] {
            let err = check(bad, ticket).unwrap_err();
            assert!(
                err.contains("{ticket}-{slug}") && err.contains("PROJ-123-add-dark"),
                "{bad}: {err}"
            );
        }
        let typed = Some("feature/{type}/{slug}");
        assert!(check("feature/fix/login-bug", typed).is_ok());
        assert!(check("feature/oops/login-bug", typed).is_err());
    }

    #[test]
    fn no_rule_or_an_unreadable_one_is_the_usual_rule() {
        assert!(check("feat/add-dark", None).is_ok());
        assert!(check("feat/add-dark", Some("  ")).is_ok());
        assert!(check("decimozs/abc12", None).is_err());
        // A typo in the setting does not block everything.
        assert!(check("feat/add-dark", Some("{nope}")).is_ok());
        assert!(check("decimozs/abc12", Some("{nope}")).is_err());
    }

    #[test]
    fn patterns_are_checked_before_use() {
        assert!(validate_pattern("{ticket}-{slug}").is_ok());
        assert!(validate_pattern("feature/{slug}").is_ok());
        assert!(validate_pattern("{type}").is_err());
        assert!(validate_pattern("{slug").is_err());
        assert!(validate_pattern("{type}{slug}").is_err());
        assert!(validate_pattern("my rule/{slug}").is_err());
        assert_eq!(example("{ticket}-{slug}"), "PROJ-123-add-dark-mode-toggle");
    }

    #[test]
    fn good_names_pass() {
        for name in [
            "feat/add-dark-mode-toggle",
            "fix/lint-warnings",
            "chore/bump-deps",
            "docs/readme",
            "refactor/split-auth-module-2",
            "revert/feat-login",
        ] {
            assert!(check_conventional(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn bad_names_fail_with_a_reason_and_an_example() {
        for (name, why) in [
            ("decimozs/9r1ce", "not one of the usual"),
            ("add-dark-mode", "no type"),
            ("feat/", "nothing follows"),
            ("feat/Add-Dark", "lowercase"),
            ("feat/add dark", "lowercase"),
            ("feat/add_dark", "lowercase"),
            ("feat/-add", "dashes"),
            ("feat/add-", "dashes"),
            ("feat/add--dark", "dashes"),
            ("feat/a/b", "more than one slash"),
            ("--output=/tmp/x", "not one of the usual"),
            ("feat/é", "lowercase"),
        ] {
            let err = check_conventional(name).unwrap_err();
            assert!(err.contains(why), "{name}: {err}");
            assert!(err.contains(EXAMPLE), "{name}: {err}");
        }
        let long = format!("feat/{}", "a".repeat(80));
        assert!(check_conventional(&long)
            .unwrap_err()
            .contains("longer than 60"));
    }
}
