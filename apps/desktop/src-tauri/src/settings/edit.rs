//! Changing a settings file without damaging it: comments and layout survive, and nothing is
//! written unless the result is valid.

use std::path::{Path, PathBuf};

use serde_json::Value;
use toml_edit::{value, Array, DocumentMut, Item, Table};

use super::{merge_tables, read_table, Scope, Settings};
use crate::error::{AppError, AppResult};

const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

fn bad(message: impl Into<String>) -> AppError {
    AppError::Settings(message.into())
}

fn one_of(key: &str, found: &Option<String>, allowed: &[&str]) -> Result<(), String> {
    match found {
        Some(v) if !allowed.contains(&v.as_str()) => {
            Err(format!("{key} must be one of: {}.", allowed.join(", ")))
        }
        _ => Ok(()),
    }
}

/// Checks the values, which the types alone cannot: a word from a short list, a file that exists.
pub fn validate(settings: &Settings) -> Result<(), String> {
    one_of(
        "general.send_key",
        &settings.general.send_key,
        &["enter", "mod_enter"],
    )?;
    one_of(
        "general.follow_up",
        &settings.general.follow_up,
        &["steer", "queue"],
    )?;
    one_of(
        "appearance.theme",
        &settings.appearance.theme,
        &["system", "light", "dark"],
    )?;
    one_of(
        "appearance.font_size",
        &settings.appearance.font_size,
        &["small", "medium", "large"],
    )?;
    one_of(
        "appearance.density",
        &settings.appearance.density,
        &["comfortable", "compact"],
    )?;
    one_of("models.effort", &settings.models.effort, EFFORTS)?;
    one_of(
        "agents.permission_mode",
        &settings.agents.permission_mode,
        &["manual", "accept_edits", "plan", "bypass"],
    )?;
    one_of(
        "git.merge_method",
        &settings.git.merge_method,
        &["squash", "merge", "rebase"],
    )?;
    one_of(
        "advanced.log_level",
        &settings.advanced.log_level,
        &["error", "warn", "info", "debug", "trace"],
    )?;
    if let Some(cap) = settings.composers.daily_run_cap {
        if !(1..=500).contains(&cap) {
            return Err("composers.daily_run_cap must be between 1 and 500.".into());
        }
    }
    if let Some(days) = settings.advanced.checkpoint_days {
        if !(1..=365).contains(&days) {
            return Err("advanced.checkpoint_days must be between 1 and 365.".into());
        }
    }
    if let Some(model) = &settings.models.model {
        if model.trim().is_empty() {
            return Err("models.model cannot be empty.".into());
        }
    }
    if let Some(path) = &settings.agents.claude_path {
        if !Path::new(path).is_file() {
            return Err(format!("agents.claude_path: {path} is not a file."));
        }
    }
    if settings
        .agents
        .extra_args
        .iter()
        .any(|a| a.trim().is_empty())
    {
        return Err("agents.extra_args cannot hold an empty argument.".into());
    }
    if let Some(mode) = &settings.scripts.run_mode {
        if mode != "concurrent" && mode != "nonconcurrent" {
            return Err("scripts.run_mode must be concurrent or nonconcurrent.".into());
        }
    }
    Ok(())
}

fn to_item(json: &Value) -> AppResult<Item> {
    Ok(match json {
        Value::String(s) => value(s.as_str()),
        Value::Bool(b) => value(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => value(i),
            None => value(n.as_f64().unwrap_or(0.0)),
        },
        Value::Array(items) => {
            let mut array = Array::new();
            for item in items {
                match item {
                    Value::String(s) => array.push(s.as_str()),
                    Value::Bool(b) => array.push(*b),
                    Value::Number(n) if n.is_i64() => array.push(n.as_i64().unwrap_or(0)),
                    _ => return Err(bad("A list can only hold text, numbers or yes and no.")),
                }
            }
            value(array)
        }
        Value::Null | Value::Object(_) => {
            return Err(bad("That kind of value cannot be saved here."))
        }
    })
}

/// `text` with the setting at `path` set to `new`, or removed when `new` is `None`. Everything else
/// in the file, comments included, stays as it was.
pub fn apply(text: &str, path: &[String], new: Option<&Value>) -> AppResult<String> {
    let Some((last, parents)) = path.split_last() else {
        return Err(bad("Which setting?"));
    };
    if path.iter().any(|segment| segment.trim().is_empty()) {
        return Err(bad("A setting name cannot be empty."));
    }
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| bad(e.message().to_string()))?;
    let mut table: &mut Table = doc.as_table_mut();
    for segment in parents {
        if !table.contains_key(segment) {
            if new.is_none() {
                return Ok(doc.to_string());
            }
            let mut created = Table::new();
            created.set_implicit(true);
            table.insert(segment, Item::Table(created));
        }
        table = table
            .get_mut(segment)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| bad(format!("{segment} is not a table.")))?;
    }
    match new {
        Some(json) => {
            let mut item = to_item(json)?;
            match table.get_mut(last) {
                // The key stays where it is, with the comments above it, and a comment after the
                // old value stays after the new one.
                Some(existing) => {
                    if let (Some(old), Item::Value(fresh)) = (existing.as_value(), &mut item) {
                        *fresh.decor_mut() = old.decor().clone();
                    }
                    *existing = item;
                }
                None => {
                    table.insert(last, item);
                }
            }
        }
        None => {
            table.remove(last);
        }
    }
    Ok(doc.to_string())
}

/// Writes through a temporary file, so a crash never leaves half a settings file.
fn write_atomic(path: &Path, text: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

/// The settings that would apply if `text` were the content of the layer `scope`.
fn merged_with(layers: &[(Scope, PathBuf)], scope: Scope, text: &str) -> AppResult<Settings> {
    let mut merged = toml::Table::new();
    for (layer, path) in layers {
        let table = if *layer == scope {
            Some(
                text.parse::<toml::Table>()
                    .map_err(|e| bad(e.message().to_string()))?,
            )
        } else {
            read_table(path)?
        };
        if let Some(table) = table {
            merge_tables(&mut merged, table);
        }
    }
    toml::Value::Table(merged)
        .try_into()
        .map_err(|e: toml::de::Error| bad(e.message().to_string()))
}

/// Sets or removes one setting in the file of `scope`. The result is checked, together with the
/// other layers, before anything is written. A value that does not pass is an error and the file
/// is left as it was.
pub fn edit(
    layers: &[(Scope, PathBuf)],
    scope: Scope,
    path: &[String],
    new: Option<&Value>,
) -> AppResult<()> {
    let file = layers
        .iter()
        .find(|(layer, _)| *layer == scope)
        .map(|(_, file)| file.clone())
        .ok_or_else(|| bad("That level of settings is not available here."))?;
    let current = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let updated = apply(&current, path, new)?;
    let settings = merged_with(layers, scope, &updated)?;
    validate(&settings).map_err(bad)?;
    if updated != current {
        write_atomic(&file, &updated)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    const FILE: &str = "# My settings. Keep this note.\n\n[general]\n# How a message is sent.\nsend_key = \"enter\" # the default\nnotifications = true\n\n[git]\nmerge_method = \"squash\"\n";

    #[test]
    fn setting_a_value_keeps_the_comments_and_the_layout() {
        let out = apply(
            FILE,
            &key(&["general", "send_key"]),
            Some(&json!("mod_enter")),
        )
        .unwrap();
        assert!(out.contains("# My settings. Keep this note."));
        assert!(out.contains("# How a message is sent."));
        assert!(
            out.contains("send_key = \"mod_enter\" # the default"),
            "{out}"
        );
        assert!(out.contains("notifications = true"));
        assert!(out.contains("[git]\nmerge_method = \"squash\""));
    }

    #[test]
    fn a_new_key_and_a_new_table_are_added() {
        let out = apply(FILE, &key(&["general", "sound"]), Some(&json!(false))).unwrap();
        assert!(out.contains("sound = false"));
        assert!(out.contains("# My settings. Keep this note."));
        let out = apply(FILE, &key(&["models", "effort"]), Some(&json!("high"))).unwrap();
        assert!(out.contains("[models]\neffort = \"high\""), "{out}");
        let empty = apply(
            "",
            &key(&["agents", "extra_args"]),
            Some(&json!(["--verbose", "x y"])),
        )
        .unwrap();
        assert!(
            empty.contains("extra_args = [\"--verbose\", \"x y\"]"),
            "{empty}"
        );
        assert!(empty.parse::<toml::Table>().is_ok());
    }

    #[test]
    fn removing_a_value_leaves_the_rest() {
        let out = apply(FILE, &key(&["general", "notifications"]), None).unwrap();
        assert!(!out.contains("notifications"));
        assert!(out.contains("# How a message is sent.") && out.contains("send_key"));
        assert_eq!(
            apply(FILE, &key(&["nothing", "here"]), None).unwrap(),
            FILE,
            "removing what is not there changes nothing"
        );
    }

    #[test]
    fn numbers_and_odd_names_are_written_properly() {
        let out = apply(
            "",
            &key(&["environment_variables", "API_URL"]),
            Some(&json!("http://x?a=1&b=\"2\"")),
        )
        .unwrap();
        let table: toml::Table = out.parse().unwrap();
        assert_eq!(
            table["environment_variables"]["API_URL"].as_str(),
            Some("http://x?a=1&b=\"2\"")
        );
        let out = apply("", &key(&["advanced", "cap"]), Some(&json!(4))).unwrap();
        assert!(out.contains("cap = 4"));
    }

    #[test]
    fn nonsense_is_refused() {
        assert!(apply(FILE, &[], Some(&json!(1))).is_err());
        assert!(apply(FILE, &key(&["general", ""]), Some(&json!(1))).is_err());
        assert!(
            apply(
                FILE,
                &key(&["general", "send_key", "deeper"]),
                Some(&json!(1))
            )
            .is_err(),
            "a value is not a table"
        );
        assert!(apply(FILE, &key(&["general", "x"]), Some(&json!({ "a": 1 }))).is_err());
        assert!(apply(FILE, &key(&["general", "x"]), Some(&json!(null))).is_err());
        assert!(apply(FILE, &key(&["general", "x"]), Some(&json!([[1]]))).is_err());
        assert!(apply("this is = not [valid", &key(&["a", "b"]), Some(&json!(1))).is_err());
    }

    fn settings_from(toml_text: &str) -> Settings {
        toml::from_str(toml_text).unwrap()
    }

    #[test]
    fn values_are_checked() {
        assert!(validate(&settings_from("")).is_ok());
        assert!(validate(&settings_from(
            "[general]\nsend_key = \"mod_enter\"\nfollow_up = \"queue\""
        ))
        .is_ok());
        for (bad_toml, key) in [
            ("[general]\nsend_key = \"tab\"", "general.send_key"),
            ("[general]\nfollow_up = \"ignore\"", "general.follow_up"),
            ("[appearance]\ntheme = \"purple\"", "appearance.theme"),
            ("[appearance]\nfont_size = \"huge\"", "appearance.font_size"),
            ("[appearance]\ndensity = \"tight\"", "appearance.density"),
            ("[models]\neffort = \"extreme\"", "models.effort"),
            (
                "[agents]\npermission_mode = \"yolo\"",
                "agents.permission_mode",
            ),
            ("[git]\nmerge_method = \"fast-forward\"", "git.merge_method"),
            ("[advanced]\nlog_level = \"loud\"", "advanced.log_level"),
            (
                "[advanced]\ncheckpoint_days = 0",
                "advanced.checkpoint_days",
            ),
            ("[scripts]\nrun_mode = \"maybe\"", "scripts.run_mode"),
        ] {
            let error = validate(&settings_from(bad_toml)).unwrap_err();
            assert!(error.starts_with(key), "{error}");
        }
        assert!(validate(&settings_from("[models]\nmodel = \"  \"")).is_err());
        assert!(validate(&settings_from(
            "[agents]\nclaude_path = \"/nonexistent/claude\""
        ))
        .unwrap_err()
        .contains("not a file"));
        assert!(validate(&settings_from("[agents]\nextra_args = [\"--ok\", \"\"]")).is_err());
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("claude");
        std::fs::write(&real, "").unwrap();
        assert!(validate(&settings_from(&format!(
            "[agents]\nclaude_path = \"{}\"",
            real.display()
        )))
        .is_ok());
    }

    fn layers(dir: &Path) -> Vec<(Scope, PathBuf)> {
        vec![
            (Scope::User, dir.join("user.toml")),
            (Scope::Project, dir.join("repo/.circles/settings.toml")),
            (Scope::LocalProject, dir.join("local/settings.toml")),
        ]
    }

    #[test]
    fn a_good_edit_is_written_into_the_right_file_only() {
        let tmp = tempfile::tempdir().unwrap();
        let layers = layers(tmp.path());
        std::fs::write(&layers[0].1, FILE).unwrap();
        edit(
            &layers,
            Scope::User,
            &key(&["general", "send_key"]),
            Some(&json!("mod_enter")),
        )
        .unwrap();
        let user = std::fs::read_to_string(&layers[0].1).unwrap();
        assert!(user.contains("send_key = \"mod_enter\" # the default"));
        assert!(!layers[1].1.exists() && !layers[2].1.exists());

        // A level with no file yet gets one, folders included.
        edit(
            &layers,
            Scope::Project,
            &key(&["git", "merge_method"]),
            Some(&json!("rebase")),
        )
        .unwrap();
        edit(
            &layers,
            Scope::LocalProject,
            &key(&["instructions", "pr"]),
            Some(&json!("Be brief.")),
        )
        .unwrap();
        assert!(std::fs::read_to_string(&layers[1].1)
            .unwrap()
            .contains("merge_method = \"rebase\""));
        assert!(std::fs::read_to_string(&layers[2].1)
            .unwrap()
            .contains("pr = \"Be brief.\""));
        assert!(
            !tmp.path().join("user.toml.tmp").exists(),
            "no temporary file is left"
        );
    }

    #[test]
    fn a_bad_value_is_refused_and_the_file_is_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let layers = layers(tmp.path());
        std::fs::write(&layers[0].1, FILE).unwrap();
        let error = edit(
            &layers,
            Scope::User,
            &key(&["general", "send_key"]),
            Some(&json!("tab")),
        )
        .unwrap_err();
        assert_eq!(error.code(), "settings");
        assert!(error
            .to_string()
            .contains("general.send_key must be one of"));
        assert_eq!(std::fs::read_to_string(&layers[0].1).unwrap(), FILE);

        // The wrong kind of value is refused as well.
        let error = edit(
            &layers,
            Scope::User,
            &key(&["general", "notifications"]),
            Some(&json!("yes")),
        )
        .unwrap_err();
        assert_eq!(error.code(), "settings");
        assert_eq!(std::fs::read_to_string(&layers[0].1).unwrap(), FILE);
        assert!(edit(
            &layers,
            Scope::Project,
            &key(&["git", "merge_method"]),
            Some(&json!("nope"))
        )
        .is_err());
        assert!(!layers[1].1.exists(), "no file is made for a refused value");
    }

    #[test]
    fn nothing_is_saved_while_another_level_is_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let layers = layers(tmp.path());
        std::fs::create_dir_all(layers[1].1.parent().unwrap()).unwrap();
        std::fs::write(&layers[1].1, "this is = not [valid").unwrap();
        let error = edit(
            &layers,
            Scope::User,
            &key(&["general", "sound"]),
            Some(&json!(true)),
        )
        .unwrap_err();
        assert_eq!(error.code(), "settings");
        assert!(
            error.to_string().contains("settings.toml"),
            "it names the file: {error}"
        );
        assert!(!layers[0].1.exists());
    }

    #[test]
    fn a_value_set_lower_down_wins_and_the_merge_is_what_is_checked() {
        let tmp = tempfile::tempdir().unwrap();
        let layers = layers(tmp.path());
        edit(
            &layers,
            Scope::User,
            &key(&["git", "merge_method"]),
            Some(&json!("merge")),
        )
        .unwrap();
        edit(
            &layers,
            Scope::Project,
            &key(&["git", "merge_method"]),
            Some(&json!("rebase")),
        )
        .unwrap();
        let merged = merged_with(
            &layers,
            Scope::Project,
            &std::fs::read_to_string(&layers[1].1).unwrap(),
        )
        .unwrap();
        assert_eq!(merged.git.merge_method.as_deref(), Some("rebase"));
        // Removing the override brings the user's value back.
        edit(
            &layers,
            Scope::Project,
            &key(&["git", "merge_method"]),
            None,
        )
        .unwrap();
        let merged = merged_with(
            &layers,
            Scope::Project,
            &std::fs::read_to_string(&layers[1].1).unwrap(),
        )
        .unwrap();
        assert_eq!(merged.git.merge_method.as_deref(), Some("merge"));
    }

    #[test]
    fn saving_the_same_value_again_does_not_touch_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let layers = layers(tmp.path());
        std::fs::write(&layers[0].1, FILE).unwrap();
        let before = std::fs::metadata(&layers[0].1).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        edit(
            &layers,
            Scope::User,
            &key(&["general", "send_key"]),
            Some(&json!("enter")),
        )
        .unwrap();
        assert_eq!(
            std::fs::metadata(&layers[0].1).unwrap().modified().unwrap(),
            before
        );
    }
}
