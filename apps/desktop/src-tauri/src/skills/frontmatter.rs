//! The front matter of a `SKILL.md`: the lines between the two `---` at the top.
//!
//! Skills only use flat keys, so this reads that and nothing more: plain and quoted text, folded and
//! literal blocks (`>-`, `|`), a continued plain value, `[a, b]` lists and `- item` lists. A value it does
//! not understand is read as text; nothing here can fail.

use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Front {
    values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Text(String),
    List(Vec<String>),
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Splits on commas that are not inside brackets or parentheses, so `Bash(git add:*), Read` is two.
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth <= 0 => {
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    out.push(current);
    out.into_iter()
        .map(|p| unquote(&p))
        .filter(|p| !p.is_empty())
        .collect()
}

pub fn parse(text: &str) -> Front {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Front::default();
    }
    let mut body: Vec<&str> = Vec::new();
    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        body.push(line);
    }
    let mut values = BTreeMap::new();
    let mut i = 0;
    while i < body.len() {
        let line = body[i];
        i += 1;
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        if indented || key.trim().is_empty() || key.contains(' ') {
            continue;
        }
        let key = key.trim().to_string();
        let rest = rest.trim();
        // The lines that belong to this key: the indented ones, and `- item` lines.
        let mut more: Vec<&str> = Vec::new();
        while i < body.len() {
            let next = body[i];
            let belongs = next.trim().is_empty()
                || next.starts_with(' ')
                || next.starts_with('\t')
                || next.trim_start().starts_with("- ");
            if !belongs {
                break;
            }
            more.push(next);
            i += 1;
        }
        while more.last().is_some_and(|l| l.trim().is_empty()) {
            more.pop();
        }
        let value = if matches!(rest, ">" | ">-" | ">+" | "|" | "|-" | "|+") {
            let parts: Vec<&str> = more.iter().map(|l| l.trim()).collect();
            if rest.starts_with('>') {
                // Folded: lines join with a space, a blank line is a break.
                let mut text = String::new();
                for part in parts {
                    if part.is_empty() {
                        text.push('\n');
                    } else {
                        if !text.is_empty() && !text.ends_with('\n') {
                            text.push(' ');
                        }
                        text.push_str(part);
                    }
                }
                Value::Text(text)
            } else {
                Value::Text(parts.join("\n"))
            }
        } else if rest.is_empty() && more.iter().any(|l| l.trim_start().starts_with("- ")) {
            Value::List(
                more.iter()
                    .filter_map(|l| l.trim_start().strip_prefix("- "))
                    .map(unquote)
                    .collect(),
            )
        } else if rest.starts_with('[') && rest.ends_with(']') {
            Value::List(split_top_level(&rest[1..rest.len() - 1]))
        } else {
            // A plain value, continued on the indented lines below it.
            let mut text = unquote(rest);
            for l in more.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(l);
            }
            Value::Text(text)
        };
        values.insert(key, value);
    }
    Front { values }
}

impl Front {
    pub fn text(&self, key: &str) -> Option<String> {
        match self.values.get(key)? {
            Value::Text(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
            Value::List(l) if !l.is_empty() => Some(l.join(", ")),
            _ => None,
        }
    }

    /// A list written as a list, or as text with commas, such as `allowed-tools: Read, Bash(ls:*)`.
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.values.get(key) {
            Some(Value::List(l)) => l.clone(),
            Some(Value::Text(t)) => split_top_level(t),
            None => Vec::new(),
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        self.text(key)
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_quoted_and_folded_values_are_read() {
        let f = parse(
            "---\nname: amplify-pr-title\ndescription: >-\n  Write the title of a pull request.\n  Use it from the PR flow.\nargument-hint: \"[KEY-n]\"\n---\n# Body\nname: not this\n",
        );
        assert_eq!(f.text("name").as_deref(), Some("amplify-pr-title"));
        assert_eq!(
            f.text("description").as_deref(),
            Some("Write the title of a pull request. Use it from the PR flow.")
        );
        assert_eq!(f.text("argument-hint").as_deref(), Some("[KEY-n]"));
    }

    #[test]
    fn literal_blocks_keep_their_lines_and_a_plain_value_can_continue() {
        let f = parse("---\nnotes: |\n  one\n  two\ndescription: A skill that\n  goes on\n---\n");
        assert_eq!(f.text("notes").as_deref(), Some("one\ntwo"));
        assert_eq!(
            f.text("description").as_deref(),
            Some("A skill that goes on")
        );
    }

    #[test]
    fn lists_come_from_every_spelling() {
        let f = parse(
            "---\nallowed-tools: Read, Bash(git add:*), Grep\nother: [a, \"b c\"]\nthird:\n  - x\n  - y\n---\n",
        );
        assert_eq!(f.list("allowed-tools"), ["Read", "Bash(git add:*)", "Grep"]);
        assert_eq!(f.list("other"), ["a", "b c"]);
        assert_eq!(f.list("third"), ["x", "y"]);
        assert!(f.list("missing").is_empty());
    }

    #[test]
    fn flags_and_missing_front_matter() {
        let f = parse("---\ndisable-model-invocation: true\n---\n");
        assert!(f.flag("disable-model-invocation"));
        assert!(!f.flag("user-invocable"));
        assert_eq!(parse("# No front matter\nname: x\n"), Front::default());
        assert_eq!(parse("").text("name"), None);
        // An unclosed block still gives what is there.
        assert_eq!(parse("---\nname: a\n").text("name").as_deref(), Some("a"));
    }
}
