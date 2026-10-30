//! Composers shared in a repository, in `<repo>/.circles/composers/*.toml`.
//!
//! A shared file may name tools and limits. It never says what a Composer may do without asking:
//! a Composer made from one gets the safe defaults, and only the tools the file leaves out are
//! switched off. Whoever imports it decides the rest.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::store::{Composer, ComposerInput};
use crate::error::{AppError, AppResult};
use crate::tools::catalog;
use crate::tools::policy::{Autonomy, Limits};

/// The folder in a repository.
pub fn folder_of(repo: &Path) -> PathBuf {
    repo.join(".circles").join("composers")
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct SharedComposer {
    pub project_id: String,
    pub project_name: String,
    /// The file name, for example `reviewer.toml`.
    pub file: String,
    pub name: String,
    pub instructions: String,
    /// The tools the file keeps. `None` means all of them.
    pub tools: Option<Vec<String>>,
    pub limits: Option<Limits>,
    /// Why the file cannot be used, when it cannot.
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct File {
    name: String,
    #[serde(default)]
    instructions: String,
    tools: Option<Vec<String>>,
    limits: Option<Limits>,
}

/// Reads the shared Composers of one repository. A file that cannot be read is listed with the
/// reason, so it is not silently missing.
pub fn scan(project_id: &str, project_name: &str, repo: &Path) -> Vec<SharedComposer> {
    let Ok(entries) = std::fs::read_dir(folder_of(repo)) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort_by_key(|e| e.file_name());
    files
        .into_iter()
        .map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let parsed = std::fs::read_to_string(entry.path())
                .map_err(|e| e.to_string())
                .and_then(|text| toml::from_str::<File>(&text).map_err(|e| e.to_string()))
                .and_then(|f| {
                    if f.name.trim().is_empty() {
                        return Err("It has no name.".to_string());
                    }
                    if let Some(tools) = &f.tools {
                        if let Some(bad) = tools.iter().find(|t| catalog::get(t).is_none()) {
                            return Err(format!("\"{bad}\" is not a tool."));
                        }
                    }
                    Ok(f)
                });
            match parsed {
                Ok(f) => SharedComposer {
                    project_id: project_id.into(),
                    project_name: project_name.into(),
                    file,
                    name: f.name,
                    instructions: f.instructions,
                    tools: f.tools,
                    limits: f.limits,
                    error: None,
                },
                Err(error) => SharedComposer {
                    project_id: project_id.into(),
                    project_name: project_name.into(),
                    name: file.trim_end_matches(".toml").to_string(),
                    file,
                    instructions: String::new(),
                    tools: None,
                    limits: None,
                    error: Some(error),
                },
            }
        })
        .collect()
}

/// What a Composer made from a shared file starts with. Nothing here lowers a guard: tools the
/// file keeps get their usual default, and the others are off.
pub fn input_from(shared: &SharedComposer) -> ComposerInput {
    let mut autonomy = std::collections::BTreeMap::new();
    if let Some(keep) = &shared.tools {
        for tool in catalog::all() {
            if !keep.iter().any(|k| k == tool.name) {
                autonomy.insert(tool.name.to_string(), Autonomy::Off);
            }
        }
    }
    ComposerInput {
        name: shared.name.clone(),
        instructions: shared.instructions.clone(),
        mode: None,
        autonomy,
        projects: None,
        integrations: vec![],
        limits: shared.limits.clone().unwrap_or_default(),
        allow_override: false,
        notes: String::new(),
        model: None,
        effort: None,
    }
}

#[derive(Serialize)]
struct Out<'a> {
    name: &'a str,
    instructions: &'a str,
    tools: Vec<&'a str>,
    limits: &'a Limits,
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() {
        "composer".into()
    } else {
        s
    }
}

/// Writes a Composer into a repository so others can use it. Autonomy and notes are left out.
pub fn export(composer: &Composer, repo: &Path) -> AppResult<PathBuf> {
    let dir = folder_of(repo);
    std::fs::create_dir_all(&dir)?;
    let tools: Vec<&str> = catalog::all()
        .into_iter()
        .filter(|t| composer.autonomy.get(t.name) != Some(&Autonomy::Off))
        .map(|t| t.name)
        .collect();
    let text = toml::to_string_pretty(&Out {
        name: &composer.name,
        instructions: &composer.instructions,
        tools,
        limits: &composer.limits,
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;
    let path = dir.join(format!("{}.toml", slug(&composer.name)));
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let folder = folder_of(dir.path());
        std::fs::create_dir_all(&folder).unwrap();
        for (name, text) in files {
            std::fs::write(folder.join(name), text).unwrap();
        }
        dir
    }

    #[test]
    fn files_in_the_folder_are_listed_with_their_tools_and_limits() {
        let dir = repo_with(&[
            (
                "reviewer.toml",
                "name = \"Reviewer\"\ninstructions = \"Review PRs.\"\ntools = [\"list_pull_requests\", \"create_workspace\"]\n[limits]\nmax_budget_usd = 7.0\n",
            ),
            ("notes.txt", "not a composer"),
        ]);
        let found = scan("p1", "proj", dir.path());
        assert_eq!(found.len(), 1);
        let f = &found[0];
        assert_eq!(
            (f.name.as_str(), f.file.as_str()),
            ("Reviewer", "reviewer.toml")
        );
        assert_eq!(f.tools.as_ref().unwrap().len(), 2);
        assert_eq!(f.limits.as_ref().unwrap().max_budget_usd, Some(7.0));
        assert!(f.error.is_none());
    }

    #[test]
    fn a_broken_file_is_listed_with_the_reason() {
        let dir = repo_with(&[
            ("bad.toml", "this is [ not toml"),
            ("unknown.toml", "name = \"X\"\ntools = [\"rm_rf\"]\n"),
            ("nameless.toml", "name = \"\"\n"),
        ]);
        let found = scan("p1", "proj", dir.path());
        assert_eq!(found.len(), 3);
        assert!(found.iter().all(|f| f.error.is_some()));
        assert!(found
            .iter()
            .any(|f| f.error.as_deref().unwrap().contains("not a tool")));
    }

    #[test]
    fn a_repository_without_the_folder_has_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan("p", "n", dir.path()).is_empty());
    }

    #[test]
    fn importing_never_lowers_a_guard_and_only_switches_tools_off() {
        // Even a file that tries to say "autonomy" gets nothing from it.
        let dir = repo_with(&[(
            "sneaky.toml",
            "name = \"S\"\ntools = [\"list_projects\", \"merge_pr\"]\n[autonomy]\nmerge_pr = \"allow\"\n",
        )]);
        let shared = &scan("p", "n", dir.path())[0];
        let input = input_from(shared);
        assert_eq!(
            input.autonomy.get("merge_pr"),
            None,
            "kept tools get the default, which asks"
        );
        assert_eq!(input.autonomy.get("list_projects"), None);
        assert_eq!(input.autonomy.get("delete_workspace"), Some(&Autonomy::Off));
        assert!(!input.allow_override);
        assert!(input.autonomy.values().all(|a| *a == Autonomy::Off));
        input.check().unwrap();
    }

    #[test]
    fn a_file_without_tools_keeps_every_tool() {
        let dir = repo_with(&[("all.toml", "name = \"All\"\n")]);
        let input = input_from(&scan("p", "n", dir.path())[0]);
        assert!(input.autonomy.is_empty());
    }

    #[test]
    fn exporting_leaves_out_autonomy_and_notes_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let composer = Composer {
            id: "c".into(),
            name: "Release Captain!".into(),
            instructions: "Keep main green.".into(),
            workspace_id: "w".into(),
            mode: "manual".into(),
            autonomy: [
                ("merge_pr".to_string(), Autonomy::Allow),
                ("delete_workspace".to_string(), Autonomy::Off),
            ]
            .into(),
            projects: Some(vec!["p".into()]),
            integrations: vec![],
            limits: Limits::default(),
            allow_override: true,
            notes: "secret notes".into(),
            model: None,
            effort: None,
            pinned: false,
            created_at: 0,
            updated_at: 0,
        };
        let path = export(&composer, dir.path()).unwrap();
        assert_eq!(path.file_name().unwrap(), "release-captain.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("allow"), "autonomy is not shared: {text}");
        assert!(!text.contains("secret notes"));
        assert!(
            !text.contains("delete_workspace"),
            "a tool that is off is not listed"
        );
        assert!(text.contains("merge_pr"));
        let back = &scan("p", "n", dir.path())[0];
        assert_eq!(back.name, "Release Captain!");
        assert!(back.error.is_none());
    }
}
