//! Parsers for git's machine readable output. Pure functions, easy to test.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    /// Short branch name, `None` when detached or bare.
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Parses `git worktree list --porcelain`.
pub fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut wt = Worktree {
            path: String::new(),
            head: None,
            branch: None,
            detached: false,
            bare: false,
            locked: false,
            prunable: false,
        };
        for line in block.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                wt.path = path.to_string();
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                wt.head = Some(head.to_string());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                wt.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            } else if line == "detached" {
                wt.detached = true;
            } else if line == "bare" {
                wt.bare = true;
            } else if line == "locked" || line.starts_with("locked ") {
                wt.locked = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                wt.prunable = true;
            }
        }
        if !wt.path.is_empty() {
            out.push(wt);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRef {
    pub name: String,
    pub local: bool,
    pub remote: bool,
}

/// Parses `git for-each-ref --format=%(refname)` over `refs/heads` and `refs/remotes/origin`.
/// A name found both locally and on origin is one entry with both flags set.
pub fn parse_branch_refs(text: &str) -> Vec<BranchRef> {
    let mut out: Vec<BranchRef> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (name, remote) = if let Some(n) = line.strip_prefix("refs/heads/") {
            (n, false)
        } else if let Some(n) = line.strip_prefix("refs/remotes/origin/") {
            (n, true)
        } else {
            continue;
        };
        if name == "HEAD" {
            continue;
        }
        match out.iter_mut().find(|b| b.name == name) {
            Some(existing) => {
                existing.local |= !remote;
                existing.remote |= remote;
            }
            None => out.push(BranchRef {
                name: name.to_string(),
                local: !remote,
                remote,
            }),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Tracked,
    Renamed,
    Unmerged,
    Untracked,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StatusFile {
    pub path: String,
    pub orig_path: Option<String>,
    /// Index status letter, `.` when unchanged.
    pub index: String,
    /// Working tree status letter, `.` when unchanged.
    pub worktree: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StatusInfo {
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<StatusFile>,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicts: u32,
    pub is_dirty: bool,
}

/// Parses `git status --porcelain=v2 --branch -z`.
pub fn parse_status(text: &str) -> StatusInfo {
    let mut info = StatusInfo::default();
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());

    while let Some(tok) = tokens.next() {
        if let Some(rest) = tok.strip_prefix("# ") {
            let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
            match key {
                "branch.oid" => {
                    info.head_oid = (value != "(initial)").then(|| value.to_string());
                }
                "branch.head" => {
                    info.branch = (value != "(detached)").then(|| value.to_string());
                }
                "branch.upstream" => info.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            info.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            info.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        let kind_char = tok.chars().next().unwrap_or(' ');
        match kind_char {
            '1' => {
                // 1 XY sub mH mI mW hH hI path
                let parts: Vec<&str> = tok.splitn(9, ' ').collect();
                if parts.len() == 9 {
                    info.files
                        .push(file(parts[1], parts[8], None, FileKind::Tracked));
                }
            }
            '2' => {
                // 2 XY sub mH mI mW hH hI Xscore path, then the original path as the next token
                let parts: Vec<&str> = tok.splitn(10, ' ').collect();
                if parts.len() == 10 {
                    let orig = tokens.next().map(str::to_string);
                    info.files
                        .push(file(parts[1], parts[9], orig, FileKind::Renamed));
                }
            }
            'u' => {
                // u XY sub m1 m2 m3 mW h1 h2 h3 path
                let parts: Vec<&str> = tok.splitn(11, ' ').collect();
                if parts.len() == 11 {
                    info.files
                        .push(file(parts[1], parts[10], None, FileKind::Unmerged));
                }
            }
            '?' => info.files.push(StatusFile {
                path: tok[2..].to_string(),
                orig_path: None,
                index: "?".into(),
                worktree: "?".into(),
                kind: FileKind::Untracked,
            }),
            '!' => info.files.push(StatusFile {
                path: tok[2..].to_string(),
                orig_path: None,
                index: "!".into(),
                worktree: "!".into(),
                kind: FileKind::Ignored,
            }),
            _ => {}
        }
    }

    for f in &info.files {
        match f.kind {
            FileKind::Untracked => info.untracked += 1,
            FileKind::Unmerged => info.conflicts += 1,
            FileKind::Ignored => {}
            FileKind::Tracked | FileKind::Renamed => {
                if f.index != "." {
                    info.staged += 1;
                }
                if f.worktree != "." {
                    info.unstaged += 1;
                }
            }
        }
    }
    info.is_dirty = info.files.iter().any(|f| f.kind != FileKind::Ignored);
    info
}

fn file(xy: &str, path: &str, orig: Option<String>, kind: FileKind) -> StatusFile {
    let mut chars = xy.chars();
    StatusFile {
        path: path.to_string(),
        orig_path: orig,
        index: chars.next().unwrap_or('.').to_string(),
        worktree: chars.next().unwrap_or('.').to_string(),
        kind,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FileStat {
    pub path: String,
    pub old_path: Option<String>,
    pub added: u32,
    pub deleted: u32,
    pub binary: bool,
}

/// Parses `git diff --numstat -z`.
pub fn parse_numstat(text: &str) -> Vec<FileStat> {
    let mut out = Vec::new();
    let mut tokens = text.split('\0');
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        let mut fields = tok.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let binary = added == "-" && deleted == "-";
        let (path, old_path) = if path.is_empty() {
            // Rename: the old and new paths follow as separate tokens.
            let old = tokens.next().unwrap_or("").to_string();
            let new = tokens.next().unwrap_or("").to_string();
            (new, Some(old))
        } else {
            (path.to_string(), None)
        };
        out.push(FileStat {
            path,
            old_path,
            added: added.parse().unwrap_or(0),
            deleted: deleted.parse().unwrap_or(0),
            binary,
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub author: String,
    /// Author time, unix seconds.
    #[ts(type = "number")]
    pub time: i64,
}

pub const LOG_FORMAT: &str = "%H%x1f%s%x1f%an%x1f%at%x1e";

/// Parses `git log --format=LOG_FORMAT`.
pub fn parse_log(text: &str) -> Vec<Commit> {
    text.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_matches(|c| c == '\n' || c == '\r');
            if rec.is_empty() {
                return None;
            }
            let mut f = rec.split('\x1f');
            Some(Commit {
                sha: f.next()?.to_string(),
                subject: f.next()?.to_string(),
                author: f.next()?.to_string(),
                time: f.next()?.parse().ok()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_refs_merge_local_and_remote() {
        let text = "refs/heads/main\nrefs/remotes/origin/main\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/feature/x\nrefs/heads/local-only\n";
        let refs = parse_branch_refs(text);
        assert_eq!(refs.len(), 3);
        assert_eq!(
            (refs[0].name.as_str(), refs[0].local, refs[0].remote),
            ("main", true, true)
        );
        assert_eq!(
            (refs[1].name.as_str(), refs[1].local, refs[1].remote),
            ("feature/x", false, true)
        );
        assert_eq!(
            (refs[2].name.as_str(), refs[2].local, refs[2].remote),
            ("local-only", true, false)
        );
    }

    #[test]
    fn worktree_list() {
        let text = "worktree /repo\nHEAD abc123\nbranch refs/heads/main\n\nworktree /w/oslo\nHEAD def456\nbranch refs/heads/me/x1\n\nworktree /w/det\nHEAD 999\ndetached\nlocked reason\n\nworktree /w/gone\nHEAD 111\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n";
        let list = parse_worktrees(text);
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert_eq!(list[1].branch.as_deref(), Some("me/x1"));
        assert!(list[2].detached && list[2].locked);
        assert!(list[3].prunable);
    }

    #[test]
    fn status_v2() {
        let text = "# branch.oid abc\0# branch.head feat\0# branch.upstream origin/feat\0# branch.ab +2 -1\0\
1 M. N... 100644 100644 100644 h1 h2 staged.txt\0\
1 .M N... 100644 100644 100644 h1 h2 dir/unstaged.txt\0\
1 MM N... 100644 100644 100644 h1 h2 both.txt\0\
2 R. N... 100644 100644 100644 h1 h2 R100 new name.txt\0old name.txt\0\
u UU N... 100644 100644 100644 100644 h1 h2 h3 conflict.txt\0\
? new.txt\0! ignored.log\0";
        let s = parse_status(text);
        assert_eq!(s.branch.as_deref(), Some("feat"));
        assert_eq!(s.upstream.as_deref(), Some("origin/feat"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!(s.files.len(), 7);
        assert_eq!(s.staged, 3); // staged.txt, both.txt, renamed
        assert_eq!(s.unstaged, 2); // unstaged.txt, both.txt
        assert_eq!(s.untracked, 1);
        assert_eq!(s.conflicts, 1);
        assert!(s.is_dirty);
        let renamed = s
            .files
            .iter()
            .find(|f| f.kind == FileKind::Renamed)
            .unwrap();
        assert_eq!(renamed.path, "new name.txt");
        assert_eq!(renamed.orig_path.as_deref(), Some("old name.txt"));
    }

    #[test]
    fn clean_repo_is_not_dirty() {
        let s = parse_status("# branch.oid abc\0# branch.head main\0");
        assert!(!s.is_dirty);
        assert_eq!(s.branch.as_deref(), Some("main"));
    }

    #[test]
    fn ignored_files_do_not_make_it_dirty() {
        assert!(!parse_status("# branch.head main\0! target/\0").is_dirty);
    }

    #[test]
    fn initial_and_detached_head() {
        let s = parse_status("# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!(s.head_oid, None);
        assert_eq!(s.branch, None);
    }

    #[test]
    fn numstat_with_binary_and_rename() {
        let text = "3\t1\tsrc/a.rs\0-\t-\timg.png\x005\t0\t\0old.txt\0new.txt\0";
        let stats = parse_numstat(text);
        assert_eq!(stats.len(), 3);
        assert_eq!((stats[0].added, stats[0].deleted), (3, 1));
        assert!(stats[1].binary);
        assert_eq!(stats[2].path, "new.txt");
        assert_eq!(stats[2].old_path.as_deref(), Some("old.txt"));
    }

    #[test]
    fn log_records() {
        let text = "aaa\x1ffirst commit\x1fAda\x1f1700000000\x1e\nbbb\x1fsecond, with | pipes\x1fBob\x1f1700000100\x1e";
        let commits = parse_log(text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[1].subject, "second, with | pipes");
        assert_eq!(commits[1].time, 1_700_000_100);
    }
}

/// A branch, remote branch or tag that points at a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct GraphRef {
    pub name: String,
    /// `head` (the checked out branch), `branch`, `remote` or `tag`.
    pub kind: String,
}

/// One commit of the graph, with the commits it grew from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct GraphCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author: String,
    /// Unix seconds.
    #[ts(type = "number")]
    pub time: i64,
    pub refs: Vec<GraphRef>,
}

pub const GRAPH_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%at%x1f%s%x1f%D%x1e";

fn parse_decorations(text: &str) -> Vec<GraphRef> {
    text.split(", ")
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .filter_map(|d| {
            if let Some(branch) = d.strip_prefix("HEAD -> ") {
                Some(GraphRef {
                    name: branch.to_string(),
                    kind: "head".into(),
                })
            } else if d == "HEAD" || d.ends_with("/HEAD") {
                // A detached HEAD and `origin/HEAD` say nothing a branch name does not.
                None
            } else if let Some(tag) = d.strip_prefix("tag: ") {
                Some(GraphRef {
                    name: tag.to_string(),
                    kind: "tag".into(),
                })
            } else if d.contains('/') && d.starts_with("origin/") {
                Some(GraphRef {
                    name: d.to_string(),
                    kind: "remote".into(),
                })
            } else {
                Some(GraphRef {
                    name: d.to_string(),
                    kind: "branch".into(),
                })
            }
        })
        .collect()
}

/// Parses `git log --format=GRAPH_FORMAT`.
pub fn parse_graph(text: &str) -> Vec<GraphCommit> {
    text.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_matches(|c| c == '\n' || c == '\r');
            if rec.is_empty() {
                return None;
            }
            let mut fields = rec.split('\x1f');
            let sha = fields.next()?.to_string();
            let parents = fields
                .next()?
                .split_whitespace()
                .map(str::to_string)
                .collect();
            let author = fields.next()?.to_string();
            let time = fields.next()?.parse().ok()?;
            let subject = fields.next()?.to_string();
            let refs = parse_decorations(fields.next().unwrap_or(""));
            Some(GraphCommit {
                sha,
                parents,
                subject,
                author,
                time,
                refs,
            })
        })
        .collect()
}
