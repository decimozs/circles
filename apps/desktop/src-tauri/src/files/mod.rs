//! Reading a workspace's files for the file tree, the editor and the diff viewer.
//!
//! Every path from the UI is relative to the workspace folder and is checked to stay inside it,
//! symbolic links included.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use ignore::WalkBuilder;
use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};

/// Text files bigger than this are not opened in the editor.
pub const TEXT_LIMIT: u64 = 2 * 1024 * 1024;
/// Images bigger than this are not sent to the UI.
pub const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;

fn outside() -> AppError {
    AppError::Invalid("That path is outside the workspace.".into())
}

/// The absolute path of `relative` inside `root`. Fails when it would leave the workspace, either
/// through `..`, an absolute path or a symbolic link, or when it does not exist.
pub fn resolve_inside(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(outside());
    }
    let root = fs::canonicalize(root)?;
    let joined = root.join(rel);
    let real = fs::canonicalize(&joined).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!("{relative} does not exist."))
        } else {
            AppError::from(e)
        }
    })?;
    if !real.starts_with(&root) {
        return Err(outside());
    }
    Ok(real)
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FsEntry {
    pub name: String,
    /// Relative to the workspace folder.
    pub path: String,
    pub is_dir: bool,
    #[ts(type = "number")]
    pub size: u64,
    /// Matched by a `.gitignore`. Only listed when ignored files are shown.
    pub ignored: bool,
}

/// The entries of one folder, folders first, then by name. `.git` is never listed.
pub fn list_dir(root: &Path, relative: &str, show_ignored: bool) -> AppResult<Vec<FsEntry>> {
    let dir = if relative.is_empty() {
        fs::canonicalize(root)?
    } else {
        resolve_inside(root, relative)?
    };
    if !dir.is_dir() {
        return Err(AppError::Invalid(format!("{relative} is not a folder.")));
    }
    let root = fs::canonicalize(root)?;

    let visible: HashSet<String> = WalkBuilder::new(&dir)
        .max_depth(Some(1))
        .hidden(false)
        .build()
        .flatten()
        .filter(|e| e.depth() == 1)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let ignored = !visible.contains(&name);
        if ignored && !show_ignored {
            continue;
        }
        let path = entry.path();
        // A link is listed as what it points at, but only when that stays inside the workspace.
        let meta = match fs::metadata(&path) {
            Ok(m) if fs::canonicalize(&path).is_ok_and(|p| p.starts_with(&root)) => m,
            _ => match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            },
        };
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(FsEntry {
            name,
            path: rel,
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            ignored,
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ContentKind {
    Text,
    Image,
    Binary,
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FileContent {
    pub path: String,
    pub kind: ContentKind,
    #[ts(type = "number")]
    pub size: u64,
    pub text: Option<String>,
    /// Monaco language id.
    pub language: String,
    /// Image data, base64 encoded.
    pub data_base64: Option<String>,
    pub mime: Option<String>,
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The Monaco language id for a file name.
pub fn language_for(path: &str) -> &'static str {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match name.as_str() {
        "dockerfile" => return "dockerfile",
        "makefile" => return "makefile",
        _ => {}
    }
    match extension(path).as_str() {
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescript",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "json" | "jsonc" => "json",
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "sh" | "bash" | "zsh" => "shell",
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "yml" | "yaml" => "yaml",
        "toml" => "ini",
        "xml" | "svg" => "xml",
        "sql" => "sql",
        "graphql" | "gql" => "graphql",
        "lua" => "lua",
        _ => "plaintext",
    }
}

fn image_mime(path: &str) -> Option<&'static str> {
    match extension(path).as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

/// Whether bytes look like text: no NUL bytes in the start and valid UTF-8.
pub fn looks_like_text(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8000)];
    !head.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// Reads a file for the viewer. Big and binary files are described instead of returned.
pub fn read_file(root: &Path, relative: &str) -> AppResult<FileContent> {
    let path = resolve_inside(root, relative)?;
    let meta = fs::metadata(&path)?;
    if meta.is_dir() {
        return Err(AppError::Invalid(format!("{relative} is a folder.")));
    }
    let size = meta.len();
    let language = language_for(relative).to_string();
    let base = FileContent {
        path: relative.to_string(),
        kind: ContentKind::Text,
        size,
        text: None,
        language,
        data_base64: None,
        mime: None,
    };
    if let Some(mime) = image_mime(relative) {
        if size > IMAGE_LIMIT {
            return Ok(FileContent {
                kind: ContentKind::TooLarge,
                ..base
            });
        }
        let bytes = fs::read(&path)?;
        return Ok(FileContent {
            kind: ContentKind::Image,
            data_base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
            mime: Some(mime.into()),
            ..base
        });
    }
    if size > TEXT_LIMIT {
        return Ok(FileContent {
            kind: ContentKind::TooLarge,
            ..base
        });
    }
    let bytes = fs::read(&path)?;
    if !looks_like_text(&bytes) {
        return Ok(FileContent {
            kind: ContentKind::Binary,
            ..base
        });
    }
    Ok(FileContent {
        text: Some(String::from_utf8_lossy(&bytes).into_owned()),
        ..base
    })
}

/// Saves the editor's text over an existing file of the workspace.
pub fn write_file(root: &Path, relative: &str, text: &str) -> AppResult<()> {
    let path = resolve_inside(root, relative)?;
    if path.is_dir() {
        return Err(AppError::Invalid(format!("{relative} is a folder.")));
    }
    fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for file in [
            "src/main.rs",
            "src/lib/a.ts",
            "README.md",
            "target/out.bin",
            ".env",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        }
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "target\n.env\n").unwrap();
        tmp
    }

    fn names(entries: &[FsEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn paths_cannot_leave_the_workspace() {
        let tmp = tree();
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secret.txt"), "secret").unwrap();
        let root = tmp.path();

        for bad in [
            "../x",
            "../../etc/hosts",
            "src/../../x",
            "/etc/hosts",
            "src/../..",
        ] {
            assert_eq!(
                resolve_inside(root, bad).unwrap_err().code(),
                "invalid",
                "{bad}"
            );
            assert!(read_file(root, bad).is_err(), "{bad}");
            assert!(write_file(root, bad, "boom").is_err(), "{bad}");
        }
        assert_eq!(
            resolve_inside(root, "nope.txt").unwrap_err().code(),
            "not_found"
        );
        assert!(resolve_inside(root, "src/main.rs").is_ok());
        assert!(resolve_inside(root, "").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_out_of_the_workspace_is_refused() {
        let tmp = tree();
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(outer.path(), tmp.path().join("escape")).unwrap();
        std::os::unix::fs::symlink(outer.path().join("secret.txt"), tmp.path().join("link.txt"))
            .unwrap();

        assert_eq!(
            read_file(tmp.path(), "escape/secret.txt")
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            read_file(tmp.path(), "link.txt").unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            list_dir(tmp.path(), "escape", true).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            write_file(tmp.path(), "link.txt", "x").unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            fs::read_to_string(outer.path().join("secret.txt")).unwrap(),
            "secret"
        );
        // The link itself is listed, but not followed.
        let top = list_dir(tmp.path(), "", false).unwrap();
        assert!(names(&top).contains(&"escape"));
    }

    #[test]
    fn a_folder_lists_folders_first_and_hides_git() {
        let tmp = tree();
        let top = list_dir(tmp.path(), "", false).unwrap();
        assert_eq!(names(&top), ["src", ".gitignore", "README.md"]);
        assert!(top[0].is_dir);
        assert_eq!(top[0].path, "src");
        let inner = list_dir(tmp.path(), "src", false).unwrap();
        assert_eq!(names(&inner), ["lib", "main.rs"]);
        assert_eq!(inner[0].path, "src/lib");
        assert!(list_dir(tmp.path(), "README.md", false).is_err());
    }

    #[test]
    fn ignored_entries_are_hidden_unless_asked_for() {
        let tmp = tree();
        let shown = list_dir(tmp.path(), "", true).unwrap();
        let ignored: Vec<_> = shown
            .iter()
            .filter(|e| e.ignored)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(ignored, ["target", ".env"]);
        assert!(!names(&shown).contains(&".git"));
        let hidden = list_dir(tmp.path(), "", false).unwrap();
        assert!(hidden.iter().all(|e| !e.ignored));
    }

    #[test]
    fn text_files_are_read_with_their_language() {
        let tmp = tree();
        fs::write(tmp.path().join("src/app.tsx"), "export {}").unwrap();
        let file = read_file(tmp.path(), "src/app.tsx").unwrap();
        assert_eq!(file.kind, ContentKind::Text);
        assert_eq!(file.text.as_deref(), Some("export {}"));
        assert_eq!(file.language, "typescript");
        assert_eq!(language_for("Dockerfile"), "dockerfile");
        assert_eq!(language_for("notes.unknown"), "plaintext");
    }

    #[test]
    fn binary_images_and_huge_files_degrade() {
        let tmp = tree();
        fs::write(tmp.path().join("blob.dat"), [0u8, 159, 146, 150, 0]).unwrap();
        assert_eq!(
            read_file(tmp.path(), "blob.dat").unwrap().kind,
            ContentKind::Binary
        );
        fs::write(tmp.path().join("latin.txt"), [0x66u8, 0xe9]).unwrap();
        assert_eq!(
            read_file(tmp.path(), "latin.txt").unwrap().kind,
            ContentKind::Binary
        );

        fs::write(tmp.path().join("dot.png"), [1u8, 2, 3]).unwrap();
        let image = read_file(tmp.path(), "dot.png").unwrap();
        assert_eq!(image.kind, ContentKind::Image);
        assert_eq!(image.mime.as_deref(), Some("image/png"));
        assert_eq!(image.data_base64.as_deref(), Some("AQID"));

        let big = fs::File::create(tmp.path().join("big.log")).unwrap();
        big.set_len(TEXT_LIMIT + 1).unwrap();
        let file = read_file(tmp.path(), "big.log").unwrap();
        assert_eq!(file.kind, ContentKind::TooLarge);
        assert!(file.text.is_none());
        assert_eq!(file.size, TEXT_LIMIT + 1);
    }

    #[test]
    fn saving_changes_an_existing_file_only() {
        let tmp = tree();
        write_file(tmp.path(), "README.md", "new text").unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("README.md")).unwrap(),
            "new text"
        );
        assert_eq!(
            write_file(tmp.path(), "missing.md", "x")
                .unwrap_err()
                .code(),
            "not_found"
        );
        assert!(write_file(tmp.path(), "src", "x").is_err());
    }
}
