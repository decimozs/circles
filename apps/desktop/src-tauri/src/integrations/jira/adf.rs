//! Jira's documents (Atlassian Document Format) to plain text and back. Only what the tools need:
//! paragraphs, headings, lists, code and links on the way out, and paragraphs, bullet lists and code
//! blocks on the way in.

use serde_json::{json, Value};

/// Reads a document as text. Unknown nodes keep their text and lose their shape.
pub fn to_text(node: &Value) -> String {
    let mut out = String::new();
    walk(node, &mut out, 0);
    out.trim().to_string()
}

fn walk(node: &Value, out: &mut String, depth: usize) {
    match node["type"].as_str() {
        Some("text") => out.push_str(node["text"].as_str().unwrap_or("")),
        Some("hardBreak") => out.push('\n'),
        Some("mention") => out.push_str(node["attrs"]["text"].as_str().unwrap_or("@someone")),
        Some("emoji") => out.push_str(node["attrs"]["text"].as_str().unwrap_or("")),
        Some("inlineCard") => out.push_str(node["attrs"]["url"].as_str().unwrap_or("")),
        Some("listItem") => {
            out.push_str(&"  ".repeat(depth.saturating_sub(1)));
            out.push_str("- ");
            children(node, out, depth);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        Some("bulletList" | "orderedList") => children(node, out, depth + 1),
        Some("codeBlock") => {
            out.push_str("```\n");
            children(node, out, depth);
            out.push_str("\n```\n");
        }
        Some("heading") => {
            out.push_str("# ");
            children(node, out, depth);
            out.push('\n');
        }
        Some("paragraph" | "blockquote" | "panel") => {
            children(node, out, depth);
            out.push('\n');
        }
        _ => children(node, out, depth),
    }
}

fn children(node: &Value, out: &mut String, depth: usize) {
    if let Some(items) = node["content"].as_array() {
        for item in items {
            walk(item, out, depth);
        }
    }
}

fn paragraph(text: &str) -> Value {
    json!({ "type": "paragraph", "content": [{ "type": "text", "text": text }] })
}

/// Text as a document. A blank line starts a paragraph, lines starting with `- ` make a bullet
/// list, and a fenced block becomes code. Everything else is plain text.
pub fn from_text(text: &str) -> Value {
    let mut blocks: Vec<Value> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            let mut code = Vec::new();
            for next in lines.by_ref() {
                if next.trim_start().starts_with("```") {
                    break;
                }
                code.push(next);
            }
            let code = code.join("\n");
            blocks.push(if code.is_empty() {
                json!({ "type": "codeBlock" })
            } else {
                json!({ "type": "codeBlock", "content": [{ "type": "text", "text": code }] })
            });
        } else if line.trim_start().starts_with("- ") {
            let mut items = vec![line];
            while let Some(next) = lines.peek() {
                if next.trim_start().starts_with("- ") {
                    items.push(lines.next().unwrap());
                } else {
                    break;
                }
            }
            blocks.push(json!({
                "type": "bulletList",
                "content": items.iter().map(|i| json!({
                    "type": "listItem",
                    "content": [paragraph(i.trim_start().trim_start_matches("- ").trim())],
                })).collect::<Vec<_>>(),
            }));
        } else if !line.trim().is_empty() {
            let mut para = vec![line];
            while let Some(next) = lines.peek() {
                let t = next.trim_start();
                if t.is_empty() || t.starts_with("- ") || t.starts_with("```") {
                    break;
                }
                para.push(lines.next().unwrap());
            }
            blocks.push(paragraph(&para.join("\n")));
        }
    }
    if blocks.is_empty() {
        blocks.push(json!({ "type": "paragraph" }));
    }
    json!({ "type": "doc", "version": 1, "content": blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_goes_in_and_comes_back_out() {
        let text = "First line\nsecond line\n\n- one\n- two\n\n```\nlet x = 1;\n```";
        let doc = from_text(text);
        assert_eq!(doc["type"], "doc");
        let back = to_text(&doc);
        assert!(back.contains("First line\nsecond line"));
        assert!(back.contains("- one\n- two"));
        assert!(back.contains("let x = 1;"));
    }

    #[test]
    fn reads_what_jira_sends() {
        let doc = json!({"type":"doc","version":1,"content":[
            {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Why"}]},
            {"type":"paragraph","content":[{"type":"text","text":"Hello "},{"type":"mention","attrs":{"text":"@Sam"}}]},
            {"type":"orderedList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"step"}]}]}]}
        ]});
        let text = to_text(&doc);
        assert!(text.contains("# Why"));
        assert!(text.contains("Hello @Sam"));
        assert!(text.contains("- step"));
    }

    #[test]
    fn empty_text_is_still_a_valid_document() {
        assert_eq!(from_text("")["content"].as_array().unwrap().len(), 1);
        assert_eq!(to_text(&Value::Null), "");
    }
}
