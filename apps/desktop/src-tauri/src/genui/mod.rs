//! Generative UI: an agent puts a small, structured interface in the chat and gets the answer back
//! as data. The agent can only ask for the components listed here. Everything in a card is plain
//! text, and nothing in a card can run anything: a button returns its value, and the agent acts on
//! it under the usual permissions.

pub mod service;
pub mod store;

use crate::tools::{Tier, ToolDef};

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

/// Card events that are connected to the window later. The service exists before the window does.
#[derive(Default)]
pub struct LateEvents {
    inner: std::sync::OnceLock<std::sync::Arc<dyn service::CardEvents>>,
}

impl LateEvents {
    pub fn connect(&self, events: std::sync::Arc<dyn service::CardEvents>) {
        let _ = self.inner.set(events);
    }
}

impl service::CardEvents for LateEvents {
    fn cards_changed(&self, session_id: &str, composer_id: Option<&str>) {
        if let Some(events) = self.inner.get() {
            events.cards_changed(session_id, composer_id);
        }
    }
    fn card_requested(&self, composer_id: Option<&str>) {
        if let Some(events) = self.inner.get() {
            events.card_requested(composer_id);
        }
    }
}

/// The tool an agent calls to show a card.
pub fn tool_def() -> ToolDef {
    ToolDef {
        name: "show_ui",
        description: "Show the person a small interface in the chat and get their answer back as data. Use it when you need them to choose, tick, fill in or confirm something, instead of asking in prose and parsing a reply. `card` is {id, title?, body: [components], submit_label?}. Components: text {text}, key_value {rows:[{label,value}]}, list {items:[{label,detail?}]}, table {columns,rows}, code {text,language?}, choice_list {id, options:[{value,label,detail?}], multiple?, required?}, form {id, fields:[{name,label,kind: text|long_text|select|checkbox|number, required?, options?}]}, steps {id, steps:[{id,text,done?}]} (a plan the person can tick, remove and reorder), confirm {id, answers:[{value,label}]}, progress {items:[{label,state}]}, actions {id, buttons:[{value,label,style?}]}. Every input component needs a unique `id`. Everything is plain text. A card with inputs waits for the answer and returns {status: answered, values: {<component id>: value}}, or dismissed or expired. A button only returns its value: you then act, under the usual permissions. Never ask for passwords or tokens. Set `outside_text: true` when the card shows text from tickets or pull requests. Keep cards short, and send one at a time.",
        tier: Tier::Read,
        schema: json!({
            "type": "object",
            "properties": {
                "card": { "type": "object", "maxProperties": 8, "maxBytes": 32768,
                          "description": "The card to show." },
                "wait": { "type": "boolean",
                          "description": "Wait for the answer. Default true for a card with inputs." }
            },
            "required": ["card"],
        }),
        repeat_safe: false,
        untrusted: false,
    }
}

/// The most a card may hold.
pub const MAX_COMPONENTS: usize = 30;
pub const MAX_OPTIONS: usize = 100;
pub const MAX_TABLE_COLUMNS: usize = 8;
pub const MAX_TABLE_ROWS: usize = 50;
pub const MAX_FIELDS: usize = 12;
pub const MAX_STEPS: usize = 30;
pub const MAX_TEXT: usize = 2_000;
pub const MAX_LABEL: usize = 200;
pub const MAX_CARD_BYTES: usize = 32 * 1024;

/// Field names that suggest a secret. A card never asks for one.
const SECRET_WORDS: &[&str] = &[
    "password",
    "passcode",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "credential",
    "private_key",
    "ssn",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Opt {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Row {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Item {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Field {
    pub name: String,
    pub label: String,
    /// `text`, `long_text`, `select`, `checkbox` or `number`.
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<Opt>,
    #[serde(default)]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Step {
    pub id: String,
    pub text: String,
    #[serde(default = "yes")]
    pub done: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Button {
    pub value: String,
    pub label: String,
    /// `primary`, `secondary` or `danger`.
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProgressItem {
    pub label: String,
    /// `pending`, `running`, `done` or `failed`.
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    Text {
        text: String,
    },
    KeyValue {
        rows: Vec<Row>,
    },
    List {
        items: Vec<Item>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code {
        text: String,
        #[serde(default)]
        language: Option<String>,
    },
    ChoiceList {
        id: String,
        options: Vec<Opt>,
        #[serde(default)]
        multiple: bool,
        #[serde(default)]
        required: bool,
    },
    Form {
        id: String,
        fields: Vec<Field>,
    },
    Steps {
        id: String,
        steps: Vec<Step>,
    },
    Confirm {
        id: String,
        answers: Vec<Opt>,
    },
    Progress {
        items: Vec<ProgressItem>,
    },
    Actions {
        id: String,
        buttons: Vec<Button>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Card {
    /// The agent's name for this card, so it can tell its answers apart.
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub body: Vec<Component>,
    /// The label of the button that sends the answers. "Send" when not given.
    #[serde(default)]
    pub submit_label: Option<String>,
    /// The card shows text that came from outside (tickets, pull requests). It is marked so.
    #[serde(default)]
    pub outside_text: bool,
}

impl Card {
    /// Whether the card asks for anything. A card that does not has nothing to answer.
    pub fn has_input(&self) -> bool {
        self.body.iter().any(|c| c.input_id().is_some())
    }
}

impl Component {
    /// The id of the answer this component gives, when it gives one.
    pub fn input_id(&self) -> Option<&str> {
        match self {
            Component::ChoiceList { id, .. }
            | Component::Form { id, .. }
            | Component::Steps { id, .. }
            | Component::Confirm { id, .. }
            | Component::Actions { id, .. } => Some(id),
            _ => None,
        }
    }
}

fn short(text: &str, max: usize, what: &str) -> Result<(), String> {
    if text.chars().count() > max {
        return Err(format!("{what} is longer than {max} characters."));
    }
    if text.contains('\0') {
        return Err(format!("{what} has a character that is not allowed."));
    }
    Ok(())
}

fn label(text: &str, what: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("{what} cannot be empty."));
    }
    short(text, MAX_LABEL, what)
}

fn options(list: &[Opt], what: &str, at_least: usize) -> Result<(), String> {
    if list.len() < at_least {
        return Err(format!("{what} needs at least {at_least} option(s)."));
    }
    if list.len() > MAX_OPTIONS {
        return Err(format!("{what} has more than {MAX_OPTIONS} options."));
    }
    let mut seen = HashSet::new();
    for o in list {
        label(&o.label, &format!("An option label in {what}"))?;
        label(&o.value, &format!("An option value in {what}"))?;
        if let Some(d) = &o.detail {
            short(d, MAX_LABEL, &format!("An option detail in {what}"))?;
        }
        if !seen.insert(o.value.as_str()) {
            return Err(format!(
                "{what} has two options with the value \"{}\".",
                o.value
            ));
        }
    }
    Ok(())
}

fn id_ok(id: &str, what: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 60
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(format!(
            "{what} must be 1 to 60 letters, digits, `-`, `_` or `.`, not \"{id}\"."
        ));
    }
    Ok(())
}

/// Reads a card from what the agent sent, and checks every limit. The error is written for the
/// agent to read, so it can fix the card and send it again.
pub fn parse_card(value: &Value) -> Result<Card, String> {
    let bytes = value.to_string().len();
    if bytes > MAX_CARD_BYTES {
        return Err(format!(
            "The card is {bytes} bytes. At most {MAX_CARD_BYTES} are allowed."
        ));
    }
    let card: Card = serde_json::from_value(value.clone()).map_err(|e| {
        format!(
            "That is not a card the app can show: {e}. Use only these components: text, key_value, list, table, code, choice_list, form, steps, confirm, progress, actions."
        )
    })?;
    validate(&card)?;
    Ok(card)
}

pub fn validate(card: &Card) -> Result<(), String> {
    id_ok(&card.id, "The card id")?;
    if let Some(title) = &card.title {
        short(title, MAX_LABEL, "The title")?;
    }
    if let Some(submit) = &card.submit_label {
        label(submit, "The submit label")?;
    }
    if card.body.is_empty() {
        return Err("The card is empty.".into());
    }
    if card.body.len() > MAX_COMPONENTS {
        return Err(format!(
            "A card can have at most {MAX_COMPONENTS} components."
        ));
    }
    let mut inputs: HashSet<&str> = HashSet::new();
    for component in &card.body {
        if let Some(id) = component.input_id() {
            id_ok(id, "A component id")?;
            if !inputs.insert(id) {
                return Err(format!("Two components have the id \"{id}\"."));
            }
        }
        match component {
            Component::Text { text } => short(text, MAX_TEXT, "A text")?,
            Component::Code { text, language } => {
                short(text, MAX_TEXT * 2, "A code block")?;
                if let Some(l) = language {
                    short(l, 30, "A language")?;
                }
            }
            Component::KeyValue { rows } => {
                if rows.len() > MAX_TABLE_ROWS {
                    return Err(format!("A key_value has more than {MAX_TABLE_ROWS} rows."));
                }
                for r in rows {
                    short(&r.label, MAX_LABEL, "A key_value label")?;
                    short(&r.value, MAX_TEXT, "A key_value value")?;
                }
            }
            Component::List { items } => {
                if items.len() > MAX_OPTIONS {
                    return Err(format!("A list has more than {MAX_OPTIONS} items."));
                }
                for i in items {
                    label(&i.label, "A list item")?;
                    if let Some(d) = &i.detail {
                        short(d, MAX_LABEL, "A list item detail")?;
                    }
                }
            }
            Component::Table { columns, rows } => {
                if columns.is_empty() || columns.len() > MAX_TABLE_COLUMNS {
                    return Err(format!("A table has 1 to {MAX_TABLE_COLUMNS} columns."));
                }
                if rows.len() > MAX_TABLE_ROWS {
                    return Err(format!("A table has at most {MAX_TABLE_ROWS} rows."));
                }
                for c in columns {
                    short(c, MAX_LABEL, "A table column")?;
                }
                for row in rows {
                    if row.len() != columns.len() {
                        return Err("Every table row needs one cell per column.".into());
                    }
                    for cell in row {
                        short(cell, MAX_LABEL * 2, "A table cell")?;
                    }
                }
            }
            Component::ChoiceList { options: o, .. } => options(o, "A choice_list", 1)?,
            Component::Confirm { answers, .. } => options(answers, "A confirm", 2)?,
            Component::Actions { buttons, .. } => {
                if buttons.is_empty() || buttons.len() > 8 {
                    return Err("actions has 1 to 8 buttons.".into());
                }
                let mut seen = HashSet::new();
                for b in buttons {
                    label(&b.label, "A button label")?;
                    label(&b.value, "A button value")?;
                    if !seen.insert(b.value.as_str()) {
                        return Err(format!("Two buttons have the value \"{}\".", b.value));
                    }
                    if let Some(style) = &b.style {
                        if !["primary", "secondary", "danger"].contains(&style.as_str()) {
                            return Err("A button style is primary, secondary or danger.".into());
                        }
                    }
                }
            }
            Component::Steps { steps, .. } => {
                if steps.is_empty() || steps.len() > MAX_STEPS {
                    return Err(format!("steps has 1 to {MAX_STEPS} steps."));
                }
                let mut seen = HashSet::new();
                for s in steps {
                    id_ok(&s.id, "A step id")?;
                    label(&s.text, "A step")?;
                    if !seen.insert(s.id.as_str()) {
                        return Err(format!("Two steps have the id \"{}\".", s.id));
                    }
                }
            }
            Component::Progress { items } => {
                if items.len() > MAX_OPTIONS {
                    return Err(format!("progress has more than {MAX_OPTIONS} items."));
                }
                for i in items {
                    label(&i.label, "A progress label")?;
                    if !["pending", "running", "done", "failed"].contains(&i.state.as_str()) {
                        return Err("A progress state is pending, running, done or failed.".into());
                    }
                }
            }
            Component::Form { fields, .. } => {
                if fields.is_empty() || fields.len() > MAX_FIELDS {
                    return Err(format!("A form has 1 to {MAX_FIELDS} fields."));
                }
                let mut seen = HashSet::new();
                for f in fields {
                    id_ok(&f.name, "A field name")?;
                    label(&f.label, "A field label")?;
                    if !seen.insert(f.name.as_str()) {
                        return Err(format!("Two fields have the name \"{}\".", f.name));
                    }
                    let lowered = format!("{} {}", f.name, f.label).to_lowercase();
                    if SECRET_WORDS.iter().any(|w| lowered.contains(w)) {
                        return Err(format!(
                            "The field \"{}\" looks like it asks for a secret. A card never does.",
                            f.name
                        ));
                    }
                    if !["text", "long_text", "select", "checkbox", "number"]
                        .contains(&f.kind.as_str())
                    {
                        return Err(format!(
                            "The field \"{}\" has kind \"{}\". Use text, long_text, select, checkbox or number.",
                            f.name, f.kind
                        ));
                    }
                    if f.kind == "select" {
                        options(&f.options, &format!("The select \"{}\"", f.name), 1)?;
                    }
                    if let Some(p) = &f.placeholder {
                        short(p, MAX_LABEL, "A placeholder")?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Checks what a person sent against the card, and returns it in the shape the agent receives:
/// an object with one entry per component that asks for something. A value that the card does not
/// offer is refused, so what reaches the agent is always one of its own options.
pub fn check_answer(card: &Card, values: &Value) -> Result<Value, String> {
    let given = values.as_object().ok_or("The answer must be an object.")?;
    let mut out = serde_json::Map::new();
    for component in &card.body {
        let Some(id) = component.input_id() else {
            continue;
        };
        let value = given.get(id);
        match component {
            Component::ChoiceList {
                options: opts,
                multiple,
                required,
                ..
            } => {
                let chosen: Vec<String> = match value {
                    Some(Value::Array(a)) => a
                        .iter()
                        .map(|v| v.as_str().map(str::to_string).ok_or("Choose by value."))
                        .collect::<Result<_, _>>()?,
                    Some(Value::String(s)) => vec![s.clone()],
                    None | Some(Value::Null) => vec![],
                    _ => return Err(format!("\"{id}\" must be a list of values.")),
                };
                if chosen.iter().any(|c| !opts.iter().any(|o| &o.value == c)) {
                    return Err(format!(
                        "\"{id}\" has a value that is not one of the options."
                    ));
                }
                if !multiple && chosen.len() > 1 {
                    return Err(format!("\"{id}\" takes one choice."));
                }
                if *required && chosen.is_empty() {
                    return Err(format!("Choose something for \"{id}\"."));
                }
                let unique: HashSet<_> = chosen.iter().collect();
                if unique.len() != chosen.len() {
                    return Err(format!("\"{id}\" has the same value twice."));
                }
                out.insert(id.to_string(), json!(chosen));
            }
            Component::Confirm { answers, .. } => {
                let s = value
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("Answer \"{id}\"."))?;
                if !answers.iter().any(|a| a.value == s) {
                    return Err(format!("\"{id}\" has an answer that is not offered."));
                }
                out.insert(id.to_string(), json!(s));
            }
            Component::Actions { buttons, .. } => {
                // A card of buttons is answered by pressing one, and the others are left out.
                if let Some(s) = value.and_then(Value::as_str) {
                    if !buttons.iter().any(|b| b.value == s) {
                        return Err(format!("\"{id}\" has a button that is not there."));
                    }
                    out.insert(id.to_string(), json!(s));
                }
            }
            Component::Steps { steps, .. } => {
                let list = value
                    .and_then(Value::as_array)
                    .ok_or_else(|| format!("\"{id}\" must be the list of steps."))?;
                let mut result = Vec::new();
                let mut seen = HashSet::new();
                for item in list {
                    let step_id = item["id"].as_str().unwrap_or_default();
                    let known = steps.iter().find(|s| s.id == step_id);
                    let Some(known) = known else {
                        return Err(format!("\"{id}\" has a step that is not there."));
                    };
                    if !seen.insert(step_id.to_string()) {
                        return Err(format!("\"{id}\" has a step twice."));
                    }
                    let text = item["text"].as_str().unwrap_or(&known.text);
                    short(text, MAX_LABEL, "A step")?;
                    result.push(json!({
                        "id": step_id,
                        "text": text,
                        "done": item["done"].as_bool().unwrap_or(known.done),
                    }));
                }
                out.insert(id.to_string(), json!(result));
            }
            Component::Form { fields, .. } => {
                let object = value
                    .and_then(Value::as_object)
                    .ok_or_else(|| format!("\"{id}\" must be an object."))?;
                let mut result = serde_json::Map::new();
                for f in fields {
                    let v = object.get(&f.name);
                    let empty = matches!(v, None | Some(Value::Null))
                        || v.and_then(Value::as_str).is_some_and(str::is_empty);
                    if empty {
                        if f.required {
                            return Err(format!("Fill in \"{}\".", f.label));
                        }
                        continue;
                    }
                    let v = v.expect("not empty");
                    match f.kind.as_str() {
                        "checkbox" => {
                            let b = v
                                .as_bool()
                                .ok_or_else(|| format!("\"{}\" is yes or no.", f.name))?;
                            result.insert(f.name.clone(), json!(b));
                        }
                        "number" => {
                            let n = v
                                .as_f64()
                                .ok_or_else(|| format!("\"{}\" must be a number.", f.name))?;
                            result.insert(f.name.clone(), json!(n));
                        }
                        "select" => {
                            let s = v.as_str().unwrap_or_default();
                            if !f.options.iter().any(|o| o.value == s) {
                                return Err(format!("\"{}\" is not one of the options.", f.name));
                            }
                            result.insert(f.name.clone(), json!(s));
                        }
                        _ => {
                            let s = v
                                .as_str()
                                .ok_or_else(|| format!("\"{}\" must be text.", f.name))?;
                            short(s, MAX_TEXT, &format!("\"{}\"", f.name))?;
                            result.insert(f.name.clone(), json!(s));
                        }
                    }
                }
                out.insert(id.to_string(), Value::Object(result));
            }
            _ => {}
        }
    }
    // Nothing the card did not ask for gets through.
    let allowed: BTreeMap<&str, ()> = card
        .body
        .iter()
        .filter_map(|c| c.input_id())
        .map(|i| (i, ()))
        .collect();
    if given.keys().any(|k| !allowed.contains_key(k.as_str())) {
        return Err("The answer has something the card did not ask for.".into());
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(body: Value) -> Result<Card, String> {
        parse_card(&json!({ "id": "c1", "title": "Pick", "body": body }))
    }

    #[test]
    fn a_good_card_of_every_kind_is_accepted() {
        let c = card(json!([
            {"type": "text", "text": "Hello"},
            {"type": "key_value", "rows": [{"label": "A", "value": "1"}]},
            {"type": "list", "items": [{"label": "x", "detail": "y"}]},
            {"type": "table", "columns": ["a", "b"], "rows": [["1", "2"]]},
            {"type": "code", "text": "let x = 1;", "language": "rust"},
            {"type": "choice_list", "id": "pick", "multiple": true,
             "options": [{"value": "a", "label": "A"}, {"value": "b", "label": "B"}]},
            {"type": "form", "id": "f", "fields": [{"name": "branch", "label": "Branch", "kind": "text"}]},
            {"type": "steps", "id": "plan", "steps": [{"id": "s1", "text": "Do it"}]},
            {"type": "confirm", "id": "ok", "answers": [{"value": "yes", "label": "Yes"}, {"value": "no", "label": "No"}]},
            {"type": "progress", "items": [{"label": "x", "state": "running"}]},
            {"type": "actions", "id": "go", "buttons": [{"value": "go", "label": "Go"}]},
        ]))
        .unwrap();
        assert_eq!(c.body.len(), 11);
        assert!(c.has_input());
    }

    #[test]
    fn hostile_and_broken_cards_are_refused_with_a_reason_the_agent_can_use() {
        let bad = |body: Value| card(body).unwrap_err();
        assert!(bad(json!([{"type": "html", "html": "<script>"}])).contains("components"));
        assert!(
            bad(json!([{"type": "text", "text": "x".repeat(MAX_TEXT + 1)}])).contains("longer")
        );
        assert!(bad(json!([])).contains("empty"));
        assert!(bad(json!([{"type": "form", "id": "f",
            "fields": [{"name": "api_token", "label": "Token", "kind": "text"}]}]))
        .contains("secret"));
        assert!(bad(json!([{"type": "form", "id": "f",
            "fields": [{"name": "x", "label": "Your password", "kind": "text"}]}]))
        .contains("secret"));
        assert!(bad(json!([
            {"type": "choice_list", "id": "a", "options": [{"value": "1", "label": "1"}]},
            {"type": "confirm", "id": "a", "answers": [{"value": "y", "label": "Y"}, {"value": "n", "label": "N"}]}
        ]))
        .contains("Two components"));
        assert!(bad(json!([{"type": "choice_list", "id": "a", "options": [
            {"value": "1", "label": "1"}, {"value": "1", "label": "again"}]}]))
        .contains("two options"));
        assert!(
            bad(json!([{"type": "table", "columns": ["a"], "rows": [["1", "2"]]}]))
                .contains("one cell per column")
        );
        assert!(bad(json!([{"type": "choice_list", "id": "bad id!", "options": [{"value": "1", "label": "1"}]}]))
            .contains("letters"));
        let many: Vec<Value> = (0..=MAX_COMPONENTS)
            .map(|_| json!({"type": "text", "text": "x"}))
            .collect();
        assert!(bad(json!(many)).contains("at most"));
    }

    #[test]
    fn a_card_that_is_too_big_is_refused_before_it_is_read() {
        let huge =
            json!({"id": "c", "body": [{"type": "text", "text": "x".repeat(MAX_CARD_BYTES)}]});
        assert!(parse_card(&huge).unwrap_err().contains("bytes"));
    }

    #[test]
    fn answers_are_checked_against_the_card() {
        let c = card(json!([
            {"type": "choice_list", "id": "pick", "multiple": true, "required": true,
             "options": [{"value": "a", "label": "A"}, {"value": "b", "label": "B"}]},
            {"type": "form", "id": "f", "fields": [
                {"name": "branch", "label": "Branch", "kind": "text", "required": true},
                {"name": "mode", "label": "Mode", "kind": "select", "options": [{"value": "fast", "label": "Fast"}]},
                {"name": "draft", "label": "Draft", "kind": "checkbox"}]},
            {"type": "steps", "id": "plan", "steps": [{"id": "s1", "text": "One"}, {"id": "s2", "text": "Two"}]},
        ]))
        .unwrap();
        let good = json!({
            "pick": ["a"],
            "f": {"branch": "feat/x", "mode": "fast", "draft": true},
            "plan": [{"id": "s2", "text": "Two", "done": true}, {"id": "s1", "text": "One", "done": false}],
        });
        let out = check_answer(&c, &good).unwrap();
        assert_eq!(out["pick"], json!(["a"]));
        assert_eq!(
            out["plan"][0]["id"], "s2",
            "the order the person chose is kept"
        );
        assert_eq!(out["f"]["draft"], true);

        let refuse = |v: Value| check_answer(&c, &v).unwrap_err();
        assert!(
            refuse(json!({"pick": ["zzz"], "f": {"branch": "x"}, "plan": []}))
                .contains("not one of")
        );
        assert!(
            refuse(json!({"pick": [], "f": {"branch": "x"}, "plan": []}))
                .contains("Choose something")
        );
        assert!(refuse(json!({"pick": ["a"], "f": {}, "plan": []})).contains("Fill in"));
        assert!(
            refuse(json!({"pick": ["a"], "f": {"branch": "x", "mode": "slow"}, "plan": []}))
                .contains("options")
        );
        assert!(
            refuse(json!({"pick": ["a"], "f": {"branch": "x"}, "plan": [{"id": "s9"}]}))
                .contains("not there")
        );
        assert!(
            refuse(json!({"pick": ["a"], "f": {"branch": "x"}, "plan": [], "extra": 1}))
                .contains("did not ask")
        );
    }
}
