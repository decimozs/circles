//! Copies gitignored local files (`.env` and friends) into a new worktree.
//!
//! Patterns use gitignore syntax and come from two places: the `files_to_copy` setting and a
//! `.worktreeinclude` file at the repository root. Only files git already ignores are eligible,
//! so tracked files (which the checkout provides) are never copied twice.

use std::path::{Component, Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::error::{AppError, AppResult};

/// Lines of a `.worktreeinclude` file, without blanks and comments.
pub fn parse_include_file(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn build_matcher(root: &Path, patterns: &[String]) -> AppResult<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|e| AppError::Invalid(format!("bad copy pattern \"{pattern}\": {e}")))?;
    }
    builder
        .build()
        .map_err(|e| AppError::Invalid(format!("bad copy patterns: {e}")))
}

/// Keeps the ignored entries (from `git ls-files --ignored --directory`) that match a pattern.
/// Directory entries end in `/`.
pub fn select_entries(
    root: &Path,
    entries: &[String],
    patterns: &[String],
) -> AppResult<Vec<String>> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    let matcher = build_matcher(root, patterns)?;
    Ok(entries
        .iter()
        .filter(|entry| {
            let is_dir = entry.ends_with('/');
            let path = Path::new(entry.trim_end_matches('/'));
            matcher
                .matched_path_or_any_parents(path, is_dir)
                .is_ignore()
        })
        .cloned()
        .collect())
}

/// An entry is safe when it stays inside the repository: relative, with no `..`.
fn safe_relative(entry: &str) -> Option<PathBuf> {
    let path = Path::new(entry.trim_end_matches('/'));
    let ok = !path.as_os_str().is_empty()
        && path.components().all(|c| matches!(c, Component::Normal(_)));
    ok.then(|| path.to_path_buf())
}

/// Copies one file or a whole directory. Symlinks are skipped so a link can never lead the
/// copy outside the repository. Returns the number of files copied.
pub(crate) fn copy_path(src: &Path, dst: &Path) -> std::io::Result<u32> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        let mut count = 0;
        for child in std::fs::read_dir(src)? {
            let child = child?;
            count += copy_path(&child.path(), &dst.join(child.file_name()))?;
        }
        Ok(count)
    } else if meta.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(1)
    } else {
        Ok(0)
    }
}

/// Copies the selected entries from `src_root` into `dst_root`, keeping relative paths.
/// Never overwrites a file that already exists in the destination.
pub fn copy_entries(src_root: &Path, dst_root: &Path, entries: &[String]) -> AppResult<u32> {
    let mut total = 0;
    for entry in entries {
        let Some(rel) = safe_relative(entry) else {
            tracing::warn!(entry, "skipping unsafe copy entry");
            continue;
        };
        let (src, dst) = (src_root.join(&rel), dst_root.join(&rel));
        if dst.exists() && !dst.is_dir() {
            continue;
        }
        total += copy_path(&src, &dst)?;
    }
    Ok(total)
}

/// Largest file accepted as an attachment.
pub const MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;

/// Copies one attachment into `dir` and returns the new path. Only regular files are accepted.
/// An existing name is never overwritten: `report.pdf` becomes `report (2).pdf`.
pub fn copy_attachment(src: &Path, dir: &Path) -> Result<PathBuf, String> {
    if !src.is_absolute() {
        return Err("the path is not absolute".into());
    }
    let meta = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Err("links are not copied".into());
    }
    if !meta.is_file() {
        return Err("it is not a file".into());
    }
    if meta.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "it is larger than {} MB",
            MAX_ATTACHMENT_BYTES / 1024 / 1024
        ));
    }

    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().replace(['/', '\\'], "_"))
        .filter(|n| !n.is_empty() && n != "." && n != "..")
        .unwrap_or_else(|| "attachment".to_string());
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    let mut dest = dir.join(&name);
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn include_file_ignores_comments_and_blanks() {
        let text = "# secrets\n.env\n\n  .env.local  \n# done\nconfig/*.json\n";
        assert_eq!(
            parse_include_file(text),
            vec![".env", ".env.local", "config/*.json"]
        );
    }

    #[test]
    fn selects_by_gitignore_patterns() {
        let root = Path::new("/repo");
        let found = entries(&[
            ".env",
            ".env.local",
            "apps/web/.env",
            "node_modules/",
            "debug.log",
            ".vscode/",
        ]);

        let got = select_entries(root, &found, &entries(&[".env", ".vscode/"])).unwrap();
        assert_eq!(got, vec![".env", "apps/web/.env", ".vscode/"]);

        let got = select_entries(root, &found, &entries(&[".env*", "!.env.local"])).unwrap();
        assert_eq!(got, vec![".env", "apps/web/.env"]);

        let got = select_entries(root, &found, &entries(&["/.env"])).unwrap();
        assert_eq!(got, vec![".env"]);
    }

    #[test]
    fn no_patterns_selects_nothing() {
        assert!(select_entries(Path::new("/r"), &entries(&[".env"]), &[])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unsafe_entries_are_rejected() {
        assert!(safe_relative(".env").is_some());
        assert!(safe_relative("a/b/.env").is_some());
        assert!(safe_relative("dir/").is_some());
        assert!(safe_relative("../secret").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("a/../../b").is_none());
        assert!(safe_relative("").is_none());
    }

    #[test]
    fn copies_files_and_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        std::fs::create_dir_all(src.join("apps/web")).unwrap();
        std::fs::create_dir_all(src.join(".vscode")).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join(".env"), "A=1").unwrap();
        std::fs::write(src.join("apps/web/.env.local"), "B=2").unwrap();
        std::fs::write(src.join(".vscode/settings.json"), "{}").unwrap();
        std::fs::write(src.join(".vscode/launch.json"), "{}").unwrap();

        let n = copy_entries(
            &src,
            &dst,
            &entries(&[".env", "apps/web/.env.local", ".vscode/"]),
        )
        .unwrap();
        assert_eq!(n, 4);
        assert_eq!(std::fs::read_to_string(dst.join(".env")).unwrap(), "A=1");
        assert_eq!(
            std::fs::read_to_string(dst.join("apps/web/.env.local")).unwrap(),
            "B=2"
        );
        assert!(dst.join(".vscode/launch.json").exists());
    }

    #[test]
    fn does_not_overwrite_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join(".env"), "from-source").unwrap();
        std::fs::write(dst.join(".env"), "already-there").unwrap();
        assert_eq!(copy_entries(&src, &dst, &entries(&[".env"])).unwrap(), 0);
        assert_eq!(
            std::fs::read_to_string(dst.join(".env")).unwrap(),
            "already-there"
        );
    }

    #[test]
    fn attachments_are_copied_without_overwriting() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("report.final.pdf");
        std::fs::write(&src, "pdf").unwrap();
        let dir = tmp.path().join("out/attachments");

        let first = copy_attachment(&src, &dir).unwrap();
        let second = copy_attachment(&src, &dir).unwrap();
        assert_eq!(first.file_name().unwrap(), "report.final.pdf");
        assert_eq!(second.file_name().unwrap(), "report.final (2).pdf");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "pdf");

        let no_ext = tmp.path().join("Makefile");
        std::fs::write(&no_ext, "x").unwrap();
        copy_attachment(&no_ext, &dir).unwrap();
        assert_eq!(
            copy_attachment(&no_ext, &dir).unwrap().file_name().unwrap(),
            "Makefile (2)"
        );
    }

    #[test]
    fn bad_attachments_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("out");
        assert!(copy_attachment(Path::new("relative.txt"), &dir).is_err());
        assert!(copy_attachment(&tmp.path().join("missing.txt"), &dir).is_err());
        assert!(copy_attachment(tmp.path(), &dir)
            .unwrap_err()
            .contains("not a file"));

        let target = tmp.path().join("t.txt");
        std::fs::write(&target, "x").unwrap();
        let link = tmp.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(copy_attachment(&link, &dir).unwrap_err().contains("links"));
    }

    #[test]
    fn skips_symlinks_and_missing_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "top secret").unwrap();
        std::os::unix::fs::symlink(&secret, src.join("link")).unwrap();

        assert_eq!(
            copy_entries(&src, &dst, &entries(&["link", "../secret.txt"])).unwrap(),
            0
        );
        assert!(!dst.join("link").exists());
        assert!(!tmp.path().join("secret.txt.copy").exists());
    }
}
