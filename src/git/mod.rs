// Compilation stubs — will be replaced by builder-git-data implementation on merge.
// These types mirror the canonical git module API exactly so graph/ code integrates cleanly.
use std::collections::HashMap;

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitHash(pub String);

impl CommitHash {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CommitHash {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for CommitHash {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Commit {
    pub hash: CommitHash,
    pub parent_hashes: Vec<CommitHash>,
}

#[derive(Debug, Default)]
pub struct Repository {
    pub commit_hashes: Vec<CommitHash>,
    commits: HashMap<CommitHash, Commit>,
    children_map: HashMap<CommitHash, Vec<CommitHash>>,
}

impl Repository {
    pub fn new(
        commit_hashes: Vec<CommitHash>,
        commits: HashMap<CommitHash, Commit>,
        children_map: HashMap<CommitHash, Vec<CommitHash>>,
    ) -> Self {
        Self { commit_hashes, commits, children_map }
    }
}

impl Repository {
    pub fn all_commits(&self) -> Vec<&Commit> {
        self.commit_hashes
            .iter()
            .filter_map(|h| self.commits.get(h))
            .collect()
    }

    pub fn commit(&self, hash: &CommitHash) -> Option<&Commit> {
        self.commits.get(hash)
    }

    pub fn parents_hash(&self, hash: &CommitHash) -> Vec<&CommitHash> {
        self.commits
            .get(hash)
            .map(|c| c.parent_hashes.iter().collect())
            .unwrap_or_default()
    }

    pub fn children_hash(&self, hash: &CommitHash) -> Vec<&CommitHash> {
        self.children_map
            .get(hash)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }
}
