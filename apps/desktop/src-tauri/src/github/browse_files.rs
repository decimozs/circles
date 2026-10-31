//! The files a pull request changes, and the text of each side of a file, for any repository.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{bad_output, Gh, GhError, GhErrorKind, DEFAULT_TIMEOUT};

/// One file a pull request changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrFile {
    pub path: String,
    /// Where it was before, for a rename.
    pub previous_path: Option<String>,
    /// `added`, `modified`, `removed`, `renamed`, `copied` or `changed`.
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    /// GitHub gives no patch for a binary file or a very large change.
    pub has_patch: bool,
}

/// The files of a pull request and the two commits they are read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrFiles {
    /// Where the branch left the base. The "before" side of every file is read at this commit.
    pub base_sha: String,
    /// The tip of the branch. The "after" side is read at this commit.
    pub head_sha: String,
    pub files: Vec<PrFile>,
}

#[derive(Deserialize)]
struct RawFile {
    filename: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(default)]
    previous_filename: Option<String>,
    #[serde(default)]
    has_patch: bool,
}

/// `owner/name`, and nothing that could change where a request goes.
pub fn valid_repo(repo: &str) -> bool {
    let ok = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    matches!(repo.split_once('/'), Some((owner, name)) if ok(owner) && ok(name))
}

/// A full commit id.
pub fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit())
}

/// A file path as it goes into the address of a request: each segment encoded, the slashes kept.
pub fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Parses the files `gh api .../pulls/n/files --paginate --jq` printed, one object after another.
pub fn parse_files(out: &str) -> Result<Vec<PrFile>, GhError> {
    let mut files = Vec::new();
    for raw in serde_json::Deserializer::from_str(out).into_iter::<RawFile>() {
        let r = raw.map_err(bad_output)?;
        files.push(PrFile {
            path: r.filename,
            previous_path: r.previous_filename.filter(|p| !p.is_empty()),
            status: r.status,
            additions: r.additions,
            deletions: r.deletions,
            has_patch: r.has_patch,
        });
    }
    Ok(files)
}

const FILE_FIELDS: &str = ".[] | {filename, status, additions, deletions, previous_filename, has_patch: (.patch != null)}";

impl Gh {
    /// The files `number` changes in `repo` (`owner/name`), with the commits to read them at.
    pub async fn browse_pr_files(
        &self,
        dir: &Path,
        repo: &str,
        number: u32,
    ) -> Result<PrFiles, GhError> {
        if !valid_repo(repo) {
            return Err(GhError::new(
                GhErrorKind::InvalidRepo,
                "That is not a repository name.",
                None,
            ));
        }
        let pull = format!("repos/{repo}/pulls/{number}");
        let shas = self
            .run_in(
                Some(dir),
                &[
                    "api",
                    &pull,
                    "--jq",
                    r#"[.base.sha, .head.sha] | join(" ")"#,
                ],
                DEFAULT_TIMEOUT,
            )
            .await?;
        let mut parts = shas.split_whitespace();
        let (tip, head) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if !valid_sha(tip) || !valid_sha(head) {
            return Err(bad_output_text("unexpected commits for the pull request"));
        }
        // The side of the base the branch left, so a change on the base since is not shown as
        // something this pull request did.
        let compare = format!("repos/{repo}/compare/{tip}...{head}");
        let merge_base = self
            .run_in(
                Some(dir),
                &["api", &compare, "--jq", ".merge_base_commit.sha"],
                DEFAULT_TIMEOUT,
            )
            .await
            .map(|s| s.trim().to_string())
            .ok()
            .filter(|s| valid_sha(s))
            .unwrap_or_else(|| tip.to_string());
        let list = format!("{pull}/files");
        let out = self
            .run_in(
                Some(dir),
                &["api", &list, "--paginate", "--jq", FILE_FIELDS],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(PrFiles {
            base_sha: merge_base,
            head_sha: head.to_string(),
            files: parse_files(&out)?,
        })
    }

    /// The text of `path` at commit `sha` in `repo`, or `None` when it is not there.
    pub async fn browse_file_text(
        &self,
        dir: &Path,
        repo: &str,
        sha: &str,
        path: &str,
    ) -> Result<Option<String>, GhError> {
        if !valid_repo(repo) || !valid_sha(sha) {
            return Err(GhError::new(
                GhErrorKind::InvalidRepo,
                "That is not a repository or commit.",
                None,
            ));
        }
        let address = format!("repos/{repo}/contents/{}?ref={sha}", encode_path(path));
        match self
            .run_in(
                Some(dir),
                &["api", "-H", "Accept: application/vnd.github.raw", &address],
                DEFAULT_TIMEOUT,
            )
            .await
        {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind == GhErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn bad_output_text(message: &str) -> GhError {
    GhError::new(GhErrorKind::Other, message, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::ResolvedEnv;
    use std::sync::Arc;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn only_a_plain_repository_name_and_a_full_commit_id_are_accepted() {
        assert!(valid_repo("decimozs/circles"));
        assert!(valid_repo("Anervea-AI/sethi_enterprise_backend"));
        assert!(valid_repo("o/r.js"));
        for bad in [
            "", "a", "a/b/c", "../x", "a/..", "a b/c", "a/b?x=1", "a/b#c", "/b", "a/",
        ] {
            assert!(!valid_repo(bad), "{bad}");
        }
        assert!(valid_sha(SHA_A));
        assert!(!valid_sha("main"));
        assert!(!valid_sha(&SHA_A[..39]));
        assert!(!valid_sha(&format!("{}g", &SHA_A[..39])));
    }

    #[test]
    fn a_path_keeps_its_slashes_and_encodes_everything_else() {
        assert_eq!(encode_path("src/main.rs"), "src/main.rs");
        assert_eq!(encode_path("docs/a b#c?.md"), "docs/a%20b%23c%3F.md");
        assert_eq!(encode_path("ünï/ß.txt"), "%C3%BCn%C3%AF/%C3%9F.txt");
    }

    #[test]
    fn files_are_read_from_objects_printed_one_after_another() {
        let out = r#"{"filename":"a.rs","status":"modified","additions":3,"deletions":1,"previous_filename":null,"has_patch":true}
{"filename":"new.rs","status":"renamed","additions":0,"deletions":0,"previous_filename":"old.rs","has_patch":false}
{"filename":"logo.png","status":"added","additions":0,"deletions":0,"previous_filename":null,"has_patch":false}"#;
        let files = parse_files(out).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].previous_path, None);
        assert_eq!(
            (files[1].previous_path.as_deref(), files[1].status.as_str()),
            (Some("old.rs"), "renamed")
        );
        assert!(files[0].has_patch && !files[2].has_patch);
        assert_eq!(parse_files("").unwrap(), vec![]);
        assert!(parse_files("not json").is_err());
    }

    fn fake_gh(dir: &Path) -> Gh {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("gh");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
for a in "$@"; do last="$a"; done
case "$*" in
  *compare*) echo {SHA_C} ;;
  *"/files"*) echo '{{"filename":"a.rs","status":"modified","additions":1,"deletions":1,"previous_filename":null,"has_patch":true}}' ;;
  *contents/missing.rs*) echo 'HTTP 404: Not Found' >&2; exit 1 ;;
  *contents/*) echo "text of $last" ;;
  *pulls/*) echo "{SHA_A} {SHA_B}" ;;
esac
"#
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!("{}:/usr/bin:/bin", bin.display());
        Gh::new(Arc::new(ResolvedEnv::for_test(&[
            ("PATH", &path),
            ("GH_TOKEN", "x"),
        ])))
    }

    #[tokio::test]
    async fn the_files_come_with_the_merge_base_and_the_tip() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = fake_gh(tmp.path());
        let files = gh.browse_pr_files(tmp.path(), "o/r", 5).await.unwrap();
        assert_eq!(
            (files.base_sha.as_str(), files.head_sha.as_str()),
            (SHA_C, SHA_B)
        );
        assert_eq!(files.files.len(), 1);
        let err = gh.browse_pr_files(tmp.path(), "../x", 5).await.unwrap_err();
        assert_eq!(err.kind, GhErrorKind::InvalidRepo);
    }

    #[tokio::test]
    async fn a_side_of_a_file_is_read_or_reported_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = fake_gh(tmp.path());
        let text = gh
            .browse_file_text(tmp.path(), "o/r", SHA_A, "src/a b.rs")
            .await
            .unwrap()
            .unwrap();
        assert!(
            text.contains(&format!("contents/src/a%20b.rs?ref={SHA_A}")),
            "{text}"
        );
        assert_eq!(
            gh.browse_file_text(tmp.path(), "o/r", SHA_A, "missing.rs")
                .await
                .unwrap(),
            None
        );
        assert!(gh
            .browse_file_text(tmp.path(), "o/r", "main", "a.rs")
            .await
            .is_err());
    }

    /// Uses the real GitHub CLI on pull request 27 of this repository:
    /// `cargo test --lib -- --ignored --nocapture real_gh_reads`
    #[tokio::test]
    #[ignore = "uses the real GitHub CLI and the network"]
    async fn real_gh_reads_the_files_and_text_of_a_pull_request() {
        let gh = Gh::new(Arc::new(crate::env::resolve().await));
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let files = gh
            .browse_pr_files(&folder, "decimozs/circles", 27)
            .await
            .unwrap();
        println!(
            "base {} head {} files {}",
            files.base_sha,
            files.head_sha,
            files.files.len()
        );
        for f in files.files.iter().take(4) {
            println!(
                "{} {} +{} -{} patch {}",
                f.status, f.path, f.additions, f.deletions, f.has_patch
            );
        }
        let first = files
            .files
            .iter()
            .find(|f| f.status == "modified")
            .expect("a modified file");
        let after = gh
            .browse_file_text(&folder, "decimozs/circles", &files.head_sha, &first.path)
            .await
            .unwrap();
        let before = gh
            .browse_file_text(&folder, "decimozs/circles", &files.base_sha, &first.path)
            .await
            .unwrap();
        println!(
            "{}: before {:?} bytes, after {:?} bytes",
            first.path,
            before.as_ref().map(String::len),
            after.as_ref().map(String::len)
        );
        assert!(before.is_some() && after.is_some());
        assert_ne!(before, after);
        let missing = gh
            .browse_file_text(
                &folder,
                "decimozs/circles",
                &files.head_sha,
                "no/such/file.rs",
            )
            .await
            .unwrap();
        assert_eq!(missing, None);
    }
}
