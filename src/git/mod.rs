pub mod commit;
pub mod diff;
pub mod executor;

pub use commit::{Author, Commit, CommitHash, CommitType, Head, Ref};
pub use diff::{FileChange, FileDiff};

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;

use crate::git::diff::{get_diff_summary, get_initial_commit_additions};
use crate::git::executor::check_git_repository;

#[derive(Debug, Clone, Copy)]
pub enum LogOrder {
    Chronological,
    Topological,
}

type RefMap = HashMap<CommitHash, Vec<Ref>>;
type CommitsMap = HashMap<CommitHash, Vec<CommitHash>>;

#[derive(Debug)]
pub struct Repository {
    path: PathBuf,
    commits: HashMap<CommitHash, Commit>,
    parents_map: CommitsMap,
    children_map: CommitsMap,
    refs: RefMap,
    pub commit_hashes: Vec<CommitHash>,
    pub head: Head,
}

impl Repository {
    pub fn load(order: LogOrder) -> Result<Self> {
        let path = PathBuf::from(".");

        check_git_repository()?;

        let (mut ref_map, head) = load_refs(&path);

        let stashes = load_stashes(&path);
        let commits = load_commits(&path, order, &head, &stashes);

        if commits.is_empty() {
            anyhow::bail!("no commits in the repository");
        }

        let commits = merge_stashes_to_commits(commits, stashes);
        let commit_hashes: Vec<CommitHash> = commits.iter().map(|c| c.hash.clone()).collect();

        let (parents_map, children_map) = build_commits_maps(&commits);
        let commit_map: HashMap<CommitHash, Commit> = commits
            .into_iter()
            .map(|c| (c.hash.clone(), c))
            .collect();

        let stash_ref_map = load_stashes_as_refs(&path);
        merge_ref_maps(&mut ref_map, stash_ref_map);

        Ok(Self {
            path,
            commits: commit_map,
            parents_map,
            children_map,
            refs: ref_map,
            commit_hashes,
            head,
        })
    }

    #[cfg(test)]
    pub fn new(
        commit_hashes: Vec<CommitHash>,
        commits: HashMap<CommitHash, Commit>,
        children_map: HashMap<CommitHash, Vec<CommitHash>>,
    ) -> Self {
        let mut parents_map: CommitsMap = HashMap::new();
        for commit in commits.values() {
            for parent_hash in &commit.parent_hashes {
                parents_map
                    .entry(commit.hash.clone())
                    .or_default()
                    .push(parent_hash.clone());
            }
        }
        Self {
            path: PathBuf::from("."),
            commits,
            parents_map,
            children_map,
            refs: RefMap::default(),
            commit_hashes,
            head: Head::None,
        }
    }

    pub fn commit(&self, hash: &CommitHash) -> Option<&Commit> {
        self.commits.get(hash)
    }

    pub fn all_commits(&self) -> Vec<&Commit> {
        self.commit_hashes
            .iter()
            .filter_map(|h| self.commit(h))
            .collect()
    }

    pub fn parents_hash(&self, hash: &CommitHash) -> Vec<&CommitHash> {
        self.parents_map
            .get(hash)
            .map(|hs| hs.iter().collect())
            .unwrap_or_default()
    }

    pub fn children_hash(&self, hash: &CommitHash) -> Vec<&CommitHash> {
        self.children_map
            .get(hash)
            .map(|hs| hs.iter().collect())
            .unwrap_or_default()
    }

    pub fn refs(&self, hash: &CommitHash) -> Vec<&Ref> {
        self.refs
            .get(hash)
            .map(|refs| refs.iter().collect())
            .unwrap_or_default()
    }

    pub fn all_refs(&self) -> Vec<&Ref> {
        self.refs.values().flatten().collect()
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    pub fn commit_detail(&self, hash: &CommitHash) -> (Commit, Vec<FileChange>) {
        let commit = self.commit(hash).unwrap().clone();
        let changes = if commit.parent_hashes.is_empty() {
            get_initial_commit_additions(&self.path, hash)
        } else {
            get_diff_summary(&self.path, hash)
        };
        (commit, changes)
    }
}

fn load_commits_format() -> String {
    ["%H", "%an", "%ae", "%ad", "%cn", "%ce", "%cd", "%s", "%b", "%P"]
        .join("\x1f")
}

fn load_commits(path: &Path, order: LogOrder, head: &Head, stashes: &[Commit]) -> Vec<Commit> {
    let mut cmd = Command::new("git");
    cmd.arg("log");

    cmd.arg(match order {
        LogOrder::Chronological => "--date-order",
        LogOrder::Topological => "--topo-order",
    });

    cmd.arg(format!("--pretty={}", load_commits_format()))
        .arg("--date=iso-strict")
        .arg("-z");

    cmd.arg("--branches").arg("--remotes").arg("--tags");

    for stash in stashes {
        if !stash.parent_hashes.is_empty() {
            cmd.arg(stash.parent_hashes[0].as_str());
        }
    }

    if !matches!(head, Head::None) {
        cmd.arg("HEAD");
    }

    cmd.current_dir(path).stdout(Stdio::piped());

    let mut process = cmd.spawn().unwrap();
    let stdout = process.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut commits = Vec::new();

    for bytes in reader.split(b'\0') {
        let bytes = bytes.unwrap();
        if bytes.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(&bytes);
        let parts: Vec<&str> = s.split('\x1f').collect();
        if parts.len() != 10 {
            continue;
        }

        let commit = Commit {
            hash: parts[0].trim().into(),
            author: Author {
                name: parts[1].into(),
                email: parts[2].into(),
                date: parts[3].into(),
            },
            committer: Author {
                name: parts[4].into(),
                email: parts[5].into(),
                date: parts[6].into(),
            },
            subject: parts[7].into(),
            body: parts[8].into(),
            parent_hashes: parse_parent_hashes(parts[9]),
            commit_type: CommitType::Commit,
        };

        commits.push(commit);
    }

    process.wait().unwrap();

    commits
}

fn load_stashes(path: &Path) -> Vec<Commit> {
    let mut cmd = Command::new("git")
        .arg("stash")
        .arg("list")
        .arg(format!("--pretty={}", load_commits_format()))
        .arg("--date=iso-strict")
        .arg("-z")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut commits = Vec::new();

    for bytes in reader.split(b'\0') {
        let bytes = bytes.unwrap();
        if bytes.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(&bytes);
        let parts: Vec<&str> = s.split('\x1f').collect();
        if parts.len() != 10 {
            continue;
        }

        let commit = Commit {
            hash: parts[0].trim().into(),
            author: Author {
                name: parts[1].into(),
                email: parts[2].into(),
                date: parts[3].into(),
            },
            committer: Author {
                name: parts[4].into(),
                email: parts[5].into(),
                date: parts[6].into(),
            },
            subject: parts[7].into(),
            body: parts[8].into(),
            parent_hashes: parse_parent_hashes(parts[9]),
            commit_type: CommitType::Stash,
        };

        commits.push(commit);
    }

    cmd.wait().unwrap();

    commits
}

fn load_stashes_as_refs(path: &Path) -> RefMap {
    let format = ["%gd", "%H", "%s"].join("\x1f");
    let mut cmd = Command::new("git")
        .arg("stash")
        .arg("list")
        .arg(format!("--format={format}"))
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut ref_map = RefMap::default();

    for line in reader.lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() != 3 {
            continue;
        }

        let r = Ref::Stash {
            name: parts[0].into(),
            target: parts[1].into(),
            message: parts[2].into(),
        };

        ref_map.entry(parts[1].into()).or_default().push(r);
    }

    cmd.wait().unwrap();

    ref_map
}

fn load_refs(path: &Path) -> (RefMap, Head) {
    let mut cmd = Command::new("git")
        .arg("show-ref")
        .arg("--head")
        .arg("--dereference")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut ref_map = RefMap::default();
    let mut tag_map: HashMap<String, Ref> = HashMap::new();
    let mut head = Head::None;

    for line in reader.lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            continue;
        }

        let hash = parts[0];
        let refname = parts[1];

        if refname == "HEAD" {
            head = if let Some(branch) = get_current_branch(path) {
                Head::Branch { name: branch }
            } else {
                Head::Detached {
                    target: hash.into(),
                }
            };
        } else if let Some(r) = parse_branch_ref(hash, refname) {
            ref_map.entry(hash.into()).or_default().push(r);
        } else if let Some(r) = parse_tag_ref(hash, refname) {
            tag_map.insert(r.name().to_string(), r);
        }
    }

    for tag in tag_map.into_values() {
        ref_map.entry(tag.target().clone()).or_default().push(tag);
    }

    ref_map.values_mut().for_each(|refs| refs.sort());

    cmd.wait().unwrap();

    (ref_map, head)
}

fn merge_stashes_to_commits(commits: Vec<Commit>, stashes: Vec<Commit>) -> Vec<Commit> {
    let mut stash_map: HashMap<CommitHash, Vec<Commit>> =
        stashes
            .into_iter()
            .fold(HashMap::new(), |mut acc, commit| {
                if !commit.parent_hashes.is_empty() {
                    let parent = commit.parent_hashes[0].clone();
                    acc.entry(parent).or_default().push(commit);
                }
                acc
            });

    let mut ret = Vec::new();
    for commit in commits {
        if let Some(stash_list) = stash_map.remove(&commit.hash) {
            for stash in stash_list {
                ret.push(stash);
            }
        }
        ret.push(commit);
    }
    ret
}

fn build_commits_maps(commits: &[Commit]) -> (CommitsMap, CommitsMap) {
    let mut parents_map = CommitsMap::new();
    let mut children_map = CommitsMap::new();

    for commit in commits {
        for parent_hash in &commit.parent_hashes {
            parents_map
                .entry(commit.hash.clone())
                .or_default()
                .push(parent_hash.clone());
            children_map
                .entry(parent_hash.clone())
                .or_default()
                .push(commit.hash.clone());
        }
    }

    (parents_map, children_map)
}

fn merge_ref_maps(m1: &mut RefMap, m2: RefMap) {
    for (k, v) in m2 {
        m1.entry(k).or_default().extend(v);
    }
}

fn parse_parent_hashes(s: &str) -> Vec<CommitHash> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    s.split(' ').map(|h| h.into()).collect()
}

fn parse_branch_ref(hash: &str, refname: &str) -> Option<Ref> {
    if refname.starts_with("refs/heads/") {
        let name = refname.trim_start_matches("refs/heads/");
        Some(Ref::Branch {
            name: name.into(),
            target: hash.into(),
        })
    } else if refname.starts_with("refs/remotes/") {
        let name = refname.trim_start_matches("refs/remotes/");
        Some(Ref::RemoteBranch {
            name: name.into(),
            target: hash.into(),
        })
    } else {
        None
    }
}

fn parse_tag_ref(hash: &str, refname: &str) -> Option<Ref> {
    if refname.starts_with("refs/tags/") {
        let name = refname.trim_start_matches("refs/tags/");
        let name = name.trim_end_matches("^{}");
        Some(Ref::Tag {
            name: name.into(),
            target: hash.into(),
        })
    } else {
        None
    }
}

fn get_current_branch(path: &Path) -> Option<String> {
    let mut cmd = Command::new("git")
        .arg("branch")
        .arg("--show-current")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let branch = reader.lines().next().and_then(|l| l.ok()).filter(|s| !s.is_empty());

    cmd.wait().unwrap();

    branch
}
