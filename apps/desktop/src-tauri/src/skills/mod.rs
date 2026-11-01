//! The skills on this Mac and in the projects: a folder with a `SKILL.md`.
//!
//! Read only. Where they are looked for: every Claude config folder's `skills` (`~/.claude*` and the
//! `CLAUDE_CONFIG_DIR` of Settings), `~/.agents/skills`, the `skills` of each installed plugin, and
//! `.claude/skills`, `.agents/skills` and `.agent/skills` of each project. A skill that several of them
//! link to (a symlink) is listed once, by its real folder. Nothing in a skill is run.

pub mod frontmatter;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::files::looks_like_text;

/// Most skills listed.
const MAX_SKILLS: usize = 500;
/// A `SKILL.md` bigger than this is not shown.
const SKILL_MD_LIMIT: u64 = 256 * 1024;
/// A file of a skill bigger than this is not shown.
const FILE_LIMIT: u64 = 1024 * 1024;
/// Most entries in a skill's file tree.
const TREE_LIMIT: usize = 300;
const PROJECT_DIRS: &[&str] = &[".claude/skills", ".agents/skills", ".agent/skills"];

#[derive(Debug, Clone, Copy, Serialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SkillGroup {
    /// In a Claude config folder: `~/.claude/skills`.
    Yours,
    /// In `~/.agents/skills`, shared with other agents.
    Shared,
    /// Brought by an installed plugin.
    Plugin,
    /// In a project's own folders.
    Project,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SkillRow {
    /// The skill's real folder. Links to it are not separate skills.
    pub id: String,
    pub name: String,
    pub description: String,
    pub group: SkillGroup,
    /// Where it was found: `~/.claude`, a project's name, a plugin's name.
    pub source: String,
    pub project_id: Option<String>,
    pub path: String,
    /// Other places that link to the same folder.
    pub also_in: Vec<String>,
    #[ts(type = "number")]
    pub modified_ms: i64,
    /// Files in its folder besides `SKILL.md`.
    pub files: u32,
    /// False when its front matter says the model may not call it by itself.
    pub model_invocable: bool,
}

/// A place that was looked in, with how many skills it held. For the empty state.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SkillRoot {
    pub label: String,
    pub path: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SkillList {
    pub skills: Vec<SkillRow>,
    pub roots: Vec<SkillRoot>,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SkillFront {
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub model: Option<String>,
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SkillFile {
    /// Relative to the skill's folder.
    pub path: String,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SkillDetail {
    pub row: SkillRow,
    pub front: SkillFront,
    /// `None` when it is too big to show.
    pub skill_md: Option<String>,
    pub files: Vec<SkillFile>,
    /// There are more files than the tree shows.
    pub truncated: bool,
}

/// What to look in.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub home: Option<PathBuf>,
    /// `CLAUDE_CONFIG_DIR` of Settings > Environment.
    pub configured: Option<String>,
    /// `(id, name, folder)` of each project.
    pub projects: Vec<(String, String, PathBuf)>,
}

struct Root {
    group: SkillGroup,
    label: String,
    dir: PathBuf,
    project_id: Option<String>,
}

fn shown(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn claude_dirs(src: &Sources) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = src.configured.as_deref().filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(d));
    }
    if let Some(home) = &src.home {
        let mut found: Vec<PathBuf> = std::fs::read_dir(home)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".claude"))
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        found.sort();
        dirs.extend(found);
    }
    let mut unique: Vec<PathBuf> = Vec::new();
    for d in dirs {
        if !unique.contains(&d) {
            unique.push(d);
        }
    }
    unique
}

/// `plugins` of one `installed_plugins.json`: the plugin's name and where it is installed.
fn installed_plugins(config_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(config_dir.join("plugins/installed_plugins.json"))
    else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(plugins) = json.get("plugins").and_then(|p| p.as_object()) {
        for (key, installs) in plugins {
            let name = key.split('@').next().unwrap_or(key).to_string();
            for install in installs.as_array().into_iter().flatten() {
                if let Some(path) = install.get("installPath").and_then(|p| p.as_str()) {
                    out.push((name.clone(), PathBuf::from(path)));
                }
            }
        }
    }
    out
}

fn roots(src: &Sources) -> Vec<Root> {
    let mut roots = Vec::new();
    let dirs = claude_dirs(src);
    for d in &dirs {
        roots.push(Root {
            group: SkillGroup::Yours,
            label: shown(d, src.home.as_deref()),
            dir: d.join("skills"),
            project_id: None,
        });
    }
    if let Some(home) = &src.home {
        roots.push(Root {
            group: SkillGroup::Shared,
            label: "~/.agents".to_string(),
            dir: home.join(".agents/skills"),
            project_id: None,
        });
    }
    let mut seen_installs: Vec<PathBuf> = Vec::new();
    for d in &dirs {
        for (name, install) in installed_plugins(d) {
            if seen_installs.contains(&install) {
                continue;
            }
            seen_installs.push(install.clone());
            roots.push(Root {
                group: SkillGroup::Plugin,
                label: name,
                dir: install.join("skills"),
                project_id: None,
            });
        }
    }
    for (id, name, folder) in &src.projects {
        for rel in PROJECT_DIRS {
            roots.push(Root {
                group: SkillGroup::Project,
                label: name.clone(),
                dir: folder.join(rel),
                project_id: Some(id.clone()),
            });
        }
    }
    roots
}

fn mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

/// The files of a skill's folder, not following links, without `SKILL.md` at the top.
fn walk(folder: &Path) -> (Vec<SkillFile>, bool) {
    let mut files = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    let mut truncated = false;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            // `file_type` does not follow links, so a link cannot lead out of the folder.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let Ok(rel) = path.strip_prefix(folder) else {
                    continue;
                };
                if rel == Path::new("SKILL.md") {
                    continue;
                }
                if files.len() >= TREE_LIMIT {
                    truncated = true;
                    return (files, truncated);
                }
                let size = entry
                    .metadata()
                    .map_or(0, |m| m.len().min(u32::MAX as u64) as u32);
                files.push(SkillFile {
                    path: rel.to_string_lossy().into_owned(),
                    size,
                });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    (files, truncated)
}

/// Every skill, and where it was looked for.
pub fn list(src: &Sources) -> SkillList {
    let mut rows: Vec<SkillRow> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut report = Vec::new();
    for root in roots(src) {
        let mut count = 0u32;
        let mut entries: Vec<_> = std::fs::read_dir(&root.dir)
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            if folder_name.starts_with('.') {
                continue;
            }
            // `metadata` follows a link: a link to a skill's folder is that skill.
            let path = entry.path();
            let skill_md = path.join("SKILL.md");
            if !std::fs::metadata(&path).is_ok_and(|m| m.is_dir())
                || !std::fs::metadata(&skill_md).is_ok_and(|m| m.is_file())
            {
                continue;
            }
            let Ok(real) = std::fs::canonicalize(&path) else {
                continue;
            };
            count += 1;
            let id = real.to_string_lossy().into_owned();
            if let Some(&at) = by_id.get(&id) {
                let row = &mut rows[at];
                if row.source != root.label && !row.also_in.contains(&root.label) {
                    row.also_in.push(root.label.clone());
                }
                continue;
            }
            if rows.len() >= MAX_SKILLS {
                continue;
            }
            let text = std::fs::read_to_string(real.join("SKILL.md")).unwrap_or_default();
            let front = frontmatter::parse(&text);
            let (files, _) = walk(&real);
            by_id.insert(id.clone(), rows.len());
            rows.push(SkillRow {
                id,
                name: front.text("name").unwrap_or(folder_name),
                description: front.text("description").unwrap_or_default(),
                group: root.group,
                source: root.label.clone(),
                project_id: root.project_id.clone(),
                path: real.to_string_lossy().into_owned(),
                also_in: Vec::new(),
                modified_ms: mtime_ms(&real.join("SKILL.md")),
                files: files.len() as u32,
                model_invocable: !front.flag("disable-model-invocation"),
            });
        }
        report.push(SkillRoot {
            label: root.label,
            path: shown(&root.dir, src.home.as_deref()),
            count,
        });
    }
    SkillList {
        skills: rows,
        roots: report,
    }
}

/// The row for `id`, which must be one the scan found: nothing else is read.
fn known(src: &Sources, id: &str) -> AppResult<SkillRow> {
    list(src)
        .skills
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound("That skill is not in any folder Circles reads.".into()))
}

pub fn detail(src: &Sources, id: &str) -> AppResult<SkillDetail> {
    let row = known(src, id)?;
    let folder = PathBuf::from(&row.path);
    let md = folder.join("SKILL.md");
    let too_big = std::fs::metadata(&md).map_or(true, |m| m.len() > SKILL_MD_LIMIT);
    let text = if too_big {
        None
    } else {
        std::fs::read_to_string(&md).ok()
    };
    let front = text.as_deref().map(frontmatter::parse).unwrap_or_default();
    let (files, truncated) = walk(&folder);
    Ok(SkillDetail {
        front: SkillFront {
            allowed_tools: front.list("allowed-tools"),
            argument_hint: front.text("argument-hint"),
            model: front.text("model"),
            disable_model_invocation: front.flag("disable-model-invocation"),
        },
        row,
        skill_md: text,
        files,
        truncated,
    })
}

/// A text file inside a skill's folder. The path must stay inside it, links included.
pub fn read_file(src: &Sources, id: &str, rel: &str) -> AppResult<String> {
    let row = known(src, id)?;
    let folder = PathBuf::from(&row.path);
    let rel_path = Path::new(rel);
    if rel.is_empty()
        || !rel_path
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(AppError::Invalid(
            "That path is not inside the skill.".into(),
        ));
    }
    let target = std::fs::canonicalize(folder.join(rel_path))
        .map_err(|_| AppError::NotFound("That file is not in the skill.".into()))?;
    if !target.starts_with(&folder) || !target.is_file() {
        return Err(AppError::Invalid(
            "That path is not inside the skill.".into(),
        ));
    }
    let size = std::fs::metadata(&target)?.len();
    if size > FILE_LIMIT {
        return Err(AppError::Invalid("That file is too big to show.".into()));
    }
    let bytes = std::fs::read(&target)?;
    if !looks_like_text(&bytes) {
        return Err(AppError::Invalid("That file is not text.".into()));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A home with a skill in two config folders (one a link), a shared one, a plugin, and a project.
    fn fixture() -> (tempfile::TempDir, Sources) {
        let tmp = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(tmp.path()).unwrap();
        write(
            &home.join(".claude/skills/caveman/SKILL.md"),
            "---\nname: caveman\ndescription: >-\n  Talk short.\n  Keep the facts.\nallowed-tools: Read, Bash(ls:*)\n---\nBody\n",
        );
        write(
            &home.join(".claude/skills/caveman/scripts/run.sh"),
            "echo hi\n",
        );
        write(
            &home.join(".claude/skills/.trash/old/SKILL.md"),
            "---\nname: old\n---\n",
        );
        write(&home.join(".claude/skills/no-skill-file/README.md"), "x");
        write(
            &home.join(".claude/skills/quiet/SKILL.md"),
            "---\ndescription: Only by hand.\ndisable-model-invocation: true\n---\n",
        );
        std::fs::create_dir_all(home.join(".claude-work/skills")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            home.join(".claude/skills/caveman"),
            home.join(".claude-work/skills/caveman"),
        )
        .unwrap();
        write(
            &home.join(".agents/skills/herdr/SKILL.md"),
            "---\nname: herdr\ndescription: Terminals.\n---\n",
        );
        let plugin = home.join(".claude/plugins/cache/m/p/1.0");
        write(
            &plugin.join("skills/review/SKILL.md"),
            "---\nname: review\ndescription: Review code.\n---\n",
        );
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            &format!(
                "{{\"version\":2,\"plugins\":{{\"pretty@market\":[{{\"installPath\":\"{}\"}}]}}}}",
                plugin.display()
            ),
        );
        let project = home.join("work/circles");
        write(
            &project.join(".claude/skills/circles-pr-title/SKILL.md"),
            "---\nname: circles-pr-title\ndescription: Titles.\n---\n",
        );
        let src = Sources {
            home: Some(home),
            configured: None,
            projects: vec![("p1".into(), "circles".into(), project)],
        };
        (tmp, src)
    }

    #[test]
    fn every_place_is_read_and_a_linked_skill_is_listed_once() {
        let (_tmp, src) = fixture();
        let found = list(&src);
        let by_name = |n: &str| found.skills.iter().find(|s| s.name == n).unwrap();
        let names: Vec<_> = found.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names.iter().filter(|n| **n == "caveman").count(),
            1,
            "{names:?}"
        );
        for n in ["herdr", "review", "circles-pr-title", "quiet"] {
            assert!(names.contains(&n), "{n} in {names:?}");
        }
        // Not skills: a hidden folder, and one with no SKILL.md.
        assert!(!names.contains(&"old") && !names.contains(&"no-skill-file"));
        let caveman = by_name("caveman");
        assert_eq!(caveman.group, SkillGroup::Yours);
        assert_eq!(caveman.source, "~/.claude");
        assert_eq!(caveman.also_in, ["~/.claude-work"]);
        assert_eq!(caveman.description, "Talk short. Keep the facts.");
        assert_eq!(caveman.files, 1);
        assert_eq!(by_name("herdr").group, SkillGroup::Shared);
        assert_eq!(by_name("review").group, SkillGroup::Plugin);
        assert_eq!(by_name("review").source, "pretty");
        let project = by_name("circles-pr-title");
        assert_eq!(project.group, SkillGroup::Project);
        assert_eq!(project.project_id.as_deref(), Some("p1"));
        // A folder name stands in for a missing name, and the model switch is read.
        assert!(!by_name("quiet").model_invocable);
        assert!(by_name("caveman").model_invocable);
        // The places are reported, with their counts.
        let claude = found.roots.iter().find(|r| r.label == "~/.claude").unwrap();
        assert_eq!(claude.count, 2);
    }

    #[test]
    fn the_configured_folder_is_read_too() {
        let (tmp, mut src) = fixture();
        let other = std::fs::canonicalize(tmp.path()).unwrap().join("elsewhere");
        write(
            &other.join("skills/mine/SKILL.md"),
            "---\nname: mine\n---\n",
        );
        src.configured = Some(other.to_string_lossy().into_owned());
        assert!(list(&src).skills.iter().any(|s| s.name == "mine"));
    }

    #[test]
    fn a_detail_has_the_text_the_front_matter_and_the_files() {
        let (_tmp, src) = fixture();
        let id = list(&src)
            .skills
            .into_iter()
            .find(|s| s.name == "caveman")
            .unwrap()
            .id;
        let d = detail(&src, &id).unwrap();
        assert!(d.skill_md.unwrap().contains("Body"));
        assert_eq!(d.front.allowed_tools, ["Read", "Bash(ls:*)"]);
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "scripts/run.sh");
        assert_eq!(read_file(&src, &id, "scripts/run.sh").unwrap(), "echo hi\n");
    }

    #[test]
    fn nothing_outside_a_listed_skill_can_be_read() {
        let (tmp, src) = fixture();
        let home = std::fs::canonicalize(tmp.path()).unwrap();
        write(&home.join("secret.txt"), "private");
        let id = list(&src)
            .skills
            .into_iter()
            .find(|s| s.name == "caveman")
            .unwrap()
            .id;
        // A path that is not a listed skill, a way out, an absolute path, and a link that leads out.
        assert!(detail(&src, &home.join("work").to_string_lossy()).is_err());
        assert!(read_file(&src, &id, "../../../secret.txt").is_err());
        assert!(read_file(&src, &id, "/etc/hosts").is_err());
        assert!(read_file(&src, &id, "").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                home.join("secret.txt"),
                home.join(".claude/skills/caveman/leak.txt"),
            )
            .unwrap();
            assert!(read_file(&src, &id, "leak.txt").is_err());
            // The tree does not list a link either.
            assert!(!detail(&src, &id)
                .unwrap()
                .files
                .iter()
                .any(|f| f.path == "leak.txt"));
        }
    }

    #[test]
    fn a_big_file_and_a_binary_file_are_not_shown() {
        let (tmp, src) = fixture();
        let home = std::fs::canonicalize(tmp.path()).unwrap();
        let skill = home.join(".claude/skills/caveman");
        std::fs::write(skill.join("big.txt"), vec![b'a'; (FILE_LIMIT + 1) as usize]).unwrap();
        std::fs::write(skill.join("bin.dat"), [0u8, 159, 146, 150, 0, 1]).unwrap();
        let id = list(&src)
            .skills
            .into_iter()
            .find(|s| s.name == "caveman")
            .unwrap()
            .id;
        assert!(read_file(&src, &id, "big.txt").is_err());
        assert!(read_file(&src, &id, "bin.dat").is_err());
    }

    #[test]
    fn a_machine_with_nothing_gives_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        let src = Sources {
            home: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        assert!(list(&src).skills.is_empty());
        assert!(list(&Sources::default()).skills.is_empty());
    }

    /// Reads the real folders of this Mac: `cargo test --lib -- --ignored --nocapture real_skills`
    #[test]
    #[ignore = "reads the real skill folders of this Mac"]
    fn real_skills_are_found() {
        let src = Sources {
            home: dirs::home_dir(),
            ..Default::default()
        };
        let found = list(&src);
        for r in &found.roots {
            println!("{:<16} {:<40} {}", r.label, r.path, r.count);
        }
        println!("{} skills", found.skills.len());
        for s in found.skills.iter().take(60) {
            println!(
                "{:?} {} [{}] +{:?} :: {}",
                s.group, s.name, s.source, s.also_in, s.description
            );
        }
    }
}
