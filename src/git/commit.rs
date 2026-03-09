use std::fmt;

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitHash(pub String);

impl CommitHash {
    pub fn as_short_hash(&self) -> String {
        self.0.chars().take(7).collect()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
pub struct Author {
    pub name: String,
    pub email: String,
    pub date: String,
}

/// Sentinel hash used for the synthetic uncommitted-changes row.
pub const UNCOMMITTED_HASH: &str = "0000000000000000000000000000000000000000";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum CommitType {
    #[default]
    Commit,
    Stash,
    /// Synthetic row representing working-tree uncommitted changes.
    Uncommitted,
}

#[derive(Debug, Default, Clone)]
pub struct Commit {
    pub hash: CommitHash,
    pub author: Author,
    pub committer: Author,
    pub subject: String,
    pub body: String,
    pub parent_hashes: Vec<CommitHash>,
    pub commit_type: CommitType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ref {
    Tag {
        name: String,
        target: CommitHash,
    },
    Branch {
        name: String,
        target: CommitHash,
    },
    RemoteBranch {
        name: String,
        target: CommitHash,
    },
    Stash {
        name: String,
        target: CommitHash,
        message: String,
    },
}

impl Ref {
    pub fn name(&self) -> &str {
        match self {
            Ref::Tag { name, .. } => name,
            Ref::Branch { name, .. } => name,
            Ref::RemoteBranch { name, .. } => name,
            Ref::Stash { name, .. } => name,
        }
    }

    pub fn target(&self) -> &CommitHash {
        match self {
            Ref::Tag { target, .. } => target,
            Ref::Branch { target, .. } => target,
            Ref::RemoteBranch { target, .. } => target,
            Ref::Stash { target, .. } => target,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Head {
    Branch { name: String },
    Detached { target: CommitHash },
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncommitted_hash_is_40_zeros() {
        assert_eq!(UNCOMMITTED_HASH.len(), 40);
        assert!(UNCOMMITTED_HASH.chars().all(|c| c == '0'));
    }

    #[test]
    fn commit_type_uncommitted_variant() {
        let ct = CommitType::Uncommitted;
        assert_eq!(ct, CommitType::Uncommitted);
        assert_ne!(ct, CommitType::Commit);
        assert_ne!(ct, CommitType::Stash);
    }

    #[test]
    fn commit_hash_from_uncommitted_hash_constant() {
        let hash: CommitHash = UNCOMMITTED_HASH.into();
        assert_eq!(hash.as_str(), UNCOMMITTED_HASH);
        assert_eq!(hash.as_short_hash(), "0000000");
    }
}
