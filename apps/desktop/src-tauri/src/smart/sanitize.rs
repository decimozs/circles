//! What a small model says is not always what is wanted. These turn its answer into a title or a
//! suggestion, or into nothing when it is not usable. The result is plain text, never markup.

const TITLE_WORDS: usize = 6;
const TITLE_CHARS: usize = 50;
const SUGGESTION_WORDS: usize = 14;
const SUGGESTION_CHARS: usize = 100;

fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// Removes the wrapping a model likes to add: quotes, a "Title:" label, markdown marks.
fn unwrap(line: &str) -> String {
    let mut s = line.trim().to_string();
    for label in ["title:", "Title:", "TITLE:", "suggestion:", "Suggestion:"] {
        if let Some(rest) = s.strip_prefix(label) {
            s = rest.trim().to_string();
        }
    }
    s = s
        .trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | '*' | '_' | '#' | '“' | '”' | '‘' | '’' | '«' | '»'
            )
        })
        .trim()
        .to_string();
    s
}

fn trim_end_punctuation(mut s: String) -> String {
    while s.ends_with(['.', ',', ';', ':', '!', '?', '-', '…']) {
        s.pop();
    }
    s.trim().to_string()
}

/// A sentence that was cut should not end on a word that needs another one after it.
fn drop_dangling(mut s: String) -> String {
    const DANGLING: &[&str] = &[
        "or", "and", "but", "to", "the", "a", "an", "of", "in", "on", "with", "for", "that",
        "whose", "is", "are", "my", "your", "at", "by", "from", "when", "while", "if", "because",
        "so", "as", "than", "then", "into", "about",
    ];
    loop {
        s = trim_end_punctuation(s);
        match s.rsplit_once(' ') {
            Some((head, last)) if DANGLING.contains(&last.to_lowercase().as_str()) => {
                s = head.to_string();
            }
            _ => return s,
        }
    }
}

fn limit_words(s: &str, words: usize, chars: usize) -> String {
    let mut out = String::new();
    for (i, word) in s.split_whitespace().enumerate() {
        if i >= words {
            break;
        }
        let next = if out.is_empty() {
            word.to_string()
        } else {
            format!("{out} {word}")
        };
        if next.chars().count() > chars {
            break;
        }
        out = next;
    }
    out
}

/// A chat title from a model's answer: the first line, unwrapped, at most 6 words and 50
/// characters, without a full stop. `None` when nothing usable is left.
pub fn title(raw: &str) -> Option<String> {
    let line = unwrap(first_line(raw));
    // A question is the user's own, handed back. A reply or a greeting is not a title.
    if line.trim_end().ends_with('?') {
        return None;
    }
    let start = line.to_lowercase();
    if [
        "i'll ", "i will ", "i'm ", "i am ", "sure", "here ", "hello", "hi!", "hey",
    ]
    .iter()
    .any(|p| start.starts_with(p))
    {
        return None;
    }
    let was_cut = line.split_whitespace().count() > TITLE_WORDS;
    let mut line = limit_words(&line, TITLE_WORDS, TITLE_CHARS);
    if was_cut {
        line = drop_dangling(line);
    }
    let line = trim_end_punctuation(line);
    let words = line.split_whitespace().count();
    // One word is rarely a title, and a refusal or a question is never one.
    let lower = line.to_lowercase();
    let refusal = [
        "i can't",
        "i cannot",
        "i'm sorry",
        "sorry,",
        "as an ai",
        "i am unable",
    ]
    .iter()
    .any(|p| lower.starts_with(p));
    (words >= 1 && !line.is_empty() && !refusal && !line.contains('\n')).then_some(line)
}

/// A suggested next message: the first sentence, unwrapped, at most 14 words, kept with its end
/// mark so it reads as a sentence. `None` when nothing usable is left.
pub fn suggestion(raw: &str) -> Option<String> {
    let line = unwrap(first_line(raw));
    // Only the first sentence.
    let sentence = match line.find(['.', '!', '?']) {
        Some(i) => line[..=i].to_string(),
        None => line,
    };
    let was_cut = sentence.split_whitespace().count() > SUGGESTION_WORDS;
    let mut limited = limit_words(&sentence, SUGGESTION_WORDS, SUGGESTION_CHARS);
    if was_cut {
        limited = drop_dangling(limited);
    }
    let words = limited.split_whitespace().count();
    let lower = limited.to_lowercase();
    let refusal = ["i can't", "i cannot", "i'm sorry", "sorry,", "as an ai"]
        .iter()
        .any(|p| lower.starts_with(p));
    (words >= 2 && !refusal).then_some(limited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_lose_their_wrapping() {
        assert_eq!(title("\"Fix login bug\"").as_deref(), Some("Fix login bug"));
        assert_eq!(
            title("Title: Fix login bug.").as_deref(),
            Some("Fix login bug")
        );
        assert_eq!(
            title("**Fix login bug**\nBecause it fails").as_deref(),
            Some("Fix login bug")
        );
        assert_eq!(title("# Fix login bug").as_deref(), Some("Fix login bug"));
        assert_eq!(
            title("\n\n  Fix login bug  \n").as_deref(),
            Some("Fix login bug")
        );
    }

    #[test]
    fn titles_are_short() {
        assert_eq!(
            title("one two three four five six seven eight nine").as_deref(),
            Some("one two three four five six")
        );
        let long = "supercalifragilisticexpialidocious ".repeat(5);
        assert!(title(&long).unwrap().chars().count() <= 50);
    }

    #[test]
    fn nothing_usable_is_nothing() {
        assert_eq!(title(""), None);
        assert_eq!(title("   \n  "), None);
        assert_eq!(title("\"\""), None);
        assert_eq!(title("I'm sorry, I can't help with that."), None);
        assert_eq!(title("What is the issue?"), None);
        assert_eq!(title("I'll get your tickets and spawn"), None);
        assert_eq!(title("Hello"), None);
    }

    #[test]
    fn markup_cannot_get_through_as_markup() {
        // Markdown marks at the ends are removed. What is left is shown as text.
        assert_eq!(title("`rm -rf`").as_deref(), Some("rm -rf"));
    }

    #[test]
    fn a_cut_sentence_does_not_end_on_a_word_that_needs_another() {
        assert_eq!(
            suggestion(
                "Find Jira tickets assigned to me whose status is To Do, In Progress, or Blocked"
            )
            .as_deref(),
            Some("Find Jira tickets assigned to me whose status is To Do, In Progress")
        );
        // A short one that was not cut is left alone.
        assert_eq!(
            suggestion("Run the tests and").as_deref(),
            Some("Run the tests and")
        );
        assert_eq!(
            title("The app crashes on startup when the settings file").as_deref(),
            Some("The app crashes on startup")
        );
    }

    #[test]
    fn suggestions_are_one_sentence() {
        assert_eq!(
            suggestion("Open a pull request for these changes. Then merge it.").as_deref(),
            Some("Open a pull request for these changes.")
        );
        assert_eq!(
            suggestion("Run the tests again").as_deref(),
            Some("Run the tests again")
        );
        assert_eq!(suggestion("Yes"), None);
        assert_eq!(suggestion("I'm sorry, I can't suggest that."), None);
        let long = "word ".repeat(40);
        assert!(suggestion(&long).unwrap().split_whitespace().count() <= 14);
    }
}
