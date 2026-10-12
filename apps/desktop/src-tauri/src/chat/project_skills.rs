//! The skills and instruction files a project keeps for its agents.
//!
//! Claude Code loads `.claude/skills` and `CLAUDE.md` by itself. A project may keep its skills in
//! `.agents/skills` or `.agent/skills` as well, which it does not load, and an `AGENTS.md`. The
//! agent is told where they are, so the project's own way of working is followed in every
//! workspace, whoever started the chat.

use std::path::Path;

/// Where a project may keep skills, relative to the workspace.
const SKILL_DIRS: &[&str] = &[".claude/skills", ".agents/skills", ".agent/skills"];
const INSTRUCTION_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md"];
const MAX_SKILLS: usize = 40;

struct Skill {
    name: String,
    path: String,
    description: String,
}

/// The `description:` of a skill's front matter, on one line.
fn description_of(text: &str) -> String {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix("description:") {
            let value = rest.trim().trim_matches(|c| c == '"' || c == '\'');
            let mut out: String = value.chars().take(160).collect();
            if value.chars().count() > 160 {
                out.push('…');
            }
            return out;
        }
    }
    String::new()
}

fn skills_in(root: &Path) -> Vec<Skill> {
    let mut found: Vec<Skill> = Vec::new();
    for dir in SKILL_DIRS {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut names: Vec<_> = entries.flatten().collect();
        names.sort_by_key(|e| e.file_name());
        for entry in names {
            let file = entry.path().join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if found.iter().any(|s| s.name == name) {
                continue;
            }
            found.push(Skill {
                name,
                path: format!("{dir}/{}/SKILL.md", entry.file_name().to_string_lossy()),
                description: description_of(&text),
            });
        }
    }
    found.truncate(MAX_SKILLS);
    found
}

/// What to tell an agent about the project's own skills and instruction files, or nothing when it
/// has none.
pub fn note(root: &Path) -> Option<String> {
    let skills = skills_in(root);
    let files: Vec<&str> = INSTRUCTION_FILES
        .iter()
        .copied()
        .filter(|f| root.join(f).is_file())
        .collect();
    if skills.is_empty() && files.is_empty() {
        return None;
    }
    let mut out = String::from(
        "This project has its own way of working. Follow it before your own habits or any \
         general preference. If it says how branches are named and this branch does not follow \
         it, say so in your final message. Do not rename the branch with git: Circles keeps \
         track of its name.\n",
    );
    if !files.is_empty() {
        out.push_str(&format!(
            "Project instruction files, to read at the start: {}.\n",
            files.join(", ")
        ));
    }
    if !skills.is_empty() {
        out.push_str(
            "Project skills. Read a skill's file, with the path given, when its description \
             fits what you are about to do, and follow it. Skills under .agents/skills and \
             .agent/skills are not loaded for you, so open them yourself:\n",
        );
        for skill in &skills {
            if skill.description.is_empty() {
                out.push_str(&format!("- {} ({})\n", skill.name, skill.path));
            } else {
                out.push_str(&format!(
                    "- {} ({}): {}\n",
                    skill.name, skill.path, skill.description
                ));
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_project_with_nothing_gets_no_note() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(note(dir.path()), None);
    }

    #[test]
    fn skills_in_every_known_folder_are_listed_with_their_path_and_description() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".claude/skills/review/SKILL.md",
            "---\nname: review\ndescription: How we review code\n---\nbody",
        );
        write(
            dir.path(),
            ".agents/skills/release/SKILL.md",
            "---\ndescription: \"Cut a release\"\n---\n",
        );
        write(
            dir.path(),
            ".agent/skills/deploy/SKILL.md",
            "no front matter",
        );
        write(dir.path(), "AGENTS.md", "rules");
        let text = note(dir.path()).unwrap();
        assert!(text.contains("- review (.claude/skills/review/SKILL.md): How we review code"));
        assert!(text.contains("- release (.agents/skills/release/SKILL.md): Cut a release"));
        assert!(text.contains("- deploy (.agent/skills/deploy/SKILL.md)\n"));
        assert!(text.contains("AGENTS.md"));
        assert!(!text.contains("CLAUDE.md"));
    }

    #[test]
    fn the_same_skill_in_two_folders_is_listed_once() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".claude/skills/a/SKILL.md", "x");
        write(dir.path(), ".agents/skills/a/SKILL.md", "x");
        let text = note(dir.path()).unwrap();
        assert_eq!(text.matches("- a (").count(), 1);
        assert!(text.contains(".claude/skills/a/SKILL.md"));
    }

    #[test]
    fn a_folder_without_a_skill_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".agents/skills/empty")).unwrap();
        assert_eq!(note(dir.path()), None);
    }
}
