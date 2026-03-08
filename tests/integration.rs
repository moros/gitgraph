//! Integration tests for gitgraph core flows.
//!
//! Each test section exercises a distinct subsystem using real git repositories
//! (created in temp dirs) or purely in-memory data structures.
//!
//! Tests that mutate the process working directory are serialised via CWD_MUTEX.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use tempfile::TempDir;

use gitgraph::git::{CommitHash, FileChange, LogOrder, Repository};
use gitgraph::graph::calc_graph;
use gitgraph::tree::FileTreeNode;

// ---------------------------------------------------------------------------
// Mutex to serialise tests that call std::env::set_current_dir.
// Integration tests run in multiple threads by default; changing cwd is a
// process-global operation and must be exclusive.
// ---------------------------------------------------------------------------
static CWD_MUTEX: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Initialise a bare git repo in `dir`, configure user identity, and commit
/// `n_commits` dummy files.  Returns the list of full commit hashes, ordered
/// oldest-first (the order they were created).
fn init_repo_with_commits(dir: &Path, n_commits: usize) -> Vec<String> {
    fn git(args: &[&str], dir: &Path) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_AUTHOR_DATE", "2020-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2020-01-01T00:00:00Z")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git command failed");
        assert!(status.success(), "git {args:?} failed");
    }

    git(&["init", "-b", "main"], dir);
    git(&["config", "user.email", "test@example.com"], dir);
    git(&["config", "user.name", "Test"], dir);

    let mut hashes = Vec::new();
    for i in 1..=n_commits {
        let filename = format!("file{i}.txt");
        std::fs::write(dir.join(&filename), format!("content {i}\n")).unwrap();
        git(&["add", &filename], dir);
        git(&["commit", "-m", &format!("commit {i}")], dir);

        // Capture the hash of the commit we just made.
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        let hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        hashes.push(hash);
    }

    hashes
}

// ---------------------------------------------------------------------------
// 1. Repository::load — commit ordering
// ---------------------------------------------------------------------------

#[test]
fn test_repo_load_commit_ordering() {
    let tmp = TempDir::new().unwrap();
    let hashes = init_repo_with_commits(tmp.path(), 3);
    // hashes[0] = oldest commit, hashes[2] = newest

    let _lock = CWD_MUTEX.lock().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    let result = Repository::load(LogOrder::Chronological);
    std::env::set_current_dir(&original).unwrap();

    let repo = result.expect("Repository::load should succeed");

    // All three commits should be present.
    assert_eq!(repo.commit_hashes.len(), 3, "expected 3 commits");

    // Commits are stored newest-first (git log order).
    let newest = &repo.commit_hashes[0];
    let oldest = &repo.commit_hashes[2];
    assert_eq!(newest.as_str(), hashes[2], "first entry should be newest commit");
    assert_eq!(oldest.as_str(), hashes[0], "last entry should be oldest commit");

    // all_commits() returns them in the same order.
    let all = repo.all_commits();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].subject, "commit 3");
    assert_eq!(all[2].subject, "commit 1");
}

// ---------------------------------------------------------------------------
// 2. Repository::load — ref resolution (branch + tag)
// ---------------------------------------------------------------------------

#[test]
fn test_repo_ref_resolution() {
    let tmp = TempDir::new().unwrap();
    let hashes = init_repo_with_commits(tmp.path(), 2);

    // Create a lightweight tag on the first (oldest) commit.
    Command::new("git")
        .args(["tag", "v0.1", &hashes[0]])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    let _lock = CWD_MUTEX.lock().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    let result = Repository::load(LogOrder::Chronological);
    std::env::set_current_dir(&original).unwrap();

    let repo = result.unwrap();

    // HEAD should be on branch "main".
    match repo.head() {
        gitgraph::git::Head::Branch { name } => assert_eq!(name, "main"),
        other => panic!("expected Head::Branch, got {other:?}"),
    }

    // The tag "v0.1" should be attached to the oldest commit.
    let oldest_hash = CommitHash::from(hashes[0].as_str());
    let refs = repo.refs(&oldest_hash);
    let tag_names: Vec<&str> = refs.iter().filter_map(|r| {
        if let gitgraph::git::Ref::Tag { name, .. } = r {
            Some(name.as_str())
        } else {
            None
        }
    }).collect();
    assert!(tag_names.contains(&"v0.1"), "tag v0.1 should be on oldest commit; got {tag_names:?}");

    // The branch "main" ref should be on the newest commit.
    let newest_hash = CommitHash::from(hashes[1].as_str());
    let newest_refs = repo.refs(&newest_hash);
    let branch_names: Vec<&str> = newest_refs.iter().filter_map(|r| {
        if let gitgraph::git::Ref::Branch { name, .. } = r {
            Some(name.as_str())
        } else {
            None
        }
    }).collect();
    assert!(branch_names.contains(&"main"), "branch main should be on newest commit; got {branch_names:?}");
}

// ---------------------------------------------------------------------------
// 3. calc_graph — commit positions and edge count on a linear repo
// ---------------------------------------------------------------------------

#[test]
fn test_calc_graph_linear_repo() {
    let tmp = TempDir::new().unwrap();
    init_repo_with_commits(tmp.path(), 3);

    let _lock = CWD_MUTEX.lock().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    let repo = Repository::load(LogOrder::Chronological).unwrap();
    std::env::set_current_dir(&original).unwrap();

    let graph = calc_graph(&repo);

    // Linear history → single lane (x = 0 for all commits).
    assert_eq!(graph.max_pos_x, 0, "linear repo should use only lane 0");
    assert_eq!(graph.commits.len(), 3);

    for hash in &graph.commits {
        let pos = &graph.commit_pos_map[hash];
        assert_eq!(pos.x, 0, "all commits should be in lane 0");
    }

    // Row assignments: y == display index.
    for (y, hash) in graph.commits.iter().enumerate() {
        assert_eq!(graph.commit_pos_map[hash].y, y);
    }

    // There should be edges (vertical connectors) between commits.
    let total_edges: usize = graph.edges.iter().map(|row| row.len()).sum();
    assert!(total_edges > 0, "a 3-commit linear graph should have edges");
}

// ---------------------------------------------------------------------------
// 4. diff output — get_diff_summary on a real repo
// ---------------------------------------------------------------------------

#[test]
fn test_diff_summary_real_repo() {
    use gitgraph::git::diff::get_diff_summary;

    let tmp = TempDir::new().unwrap();
    let hashes = init_repo_with_commits(tmp.path(), 2);

    // Second commit should show one added file.
    let commit2 = CommitHash::from(hashes[1].as_str());
    let changes = get_diff_summary(tmp.path(), &commit2);

    assert_eq!(changes.len(), 1, "second commit adds exactly one file");
    match &changes[0] {
        FileChange::Add(path) => assert_eq!(path, "file2.txt"),
        other => panic!("expected Add, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 5. file_diff — initial commit synthesised diff
// ---------------------------------------------------------------------------

#[test]
fn test_file_diff_initial_commit() {
    use gitgraph::git::diff::file_diff;

    let tmp = TempDir::new().unwrap();
    let hashes = init_repo_with_commits(tmp.path(), 1);

    let commit = CommitHash::from(hashes[0].as_str());
    let parent = CommitHash::from(""); // empty = initial commit

    let diff = file_diff(tmp.path(), &parent, &commit, "file1.txt");

    assert_eq!(diff.filename, "file1.txt");
    assert!(diff.added_lines >= 1, "initial commit should have added lines");
    assert_eq!(diff.removed_lines, 0, "initial commit removes nothing");
    assert!(diff.content.contains("+content 1"), "diff should contain added content");
}

// ---------------------------------------------------------------------------
// 6. FileTreeNode::build — hierarchical tree from FileChange lists
// ---------------------------------------------------------------------------

#[test]
fn test_file_tree_build_flat() {
    let changes = vec![
        FileChange::Add("main.rs".into()),
        FileChange::Modify("lib.rs".into()),
        FileChange::Delete("old.rs".into()),
    ];
    let nodes = FileTreeNode::build(&changes);

    // All flat files, sorted alpha.
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].name, "lib.rs");
    assert_eq!(nodes[1].name, "main.rs");
    assert_eq!(nodes[2].name, "old.rs");
    assert!(nodes.iter().all(|n| !n.is_dir));
}

#[test]
fn test_file_tree_build_nested() {
    let changes = vec![
        FileChange::Add("src/main.rs".into()),
        FileChange::Add("src/lib.rs".into()),
        FileChange::Modify("Cargo.toml".into()),
        FileChange::Add("tests/integration.rs".into()),
    ];
    let nodes = FileTreeNode::build(&changes);

    // Dirs first (src, tests), then files (Cargo.toml).
    assert_eq!(nodes.len(), 3, "should have src/, tests/, Cargo.toml");
    assert!(nodes[0].is_dir);
    assert!(nodes[1].is_dir);
    assert!(!nodes[2].is_dir);

    let src = &nodes[0];
    assert_eq!(src.name, "src");
    assert!(src.expanded, "dirs start expanded");
    assert_eq!(src.children.len(), 2);
    // Sorted: lib.rs, main.rs
    assert_eq!(src.children[0].name, "lib.rs");
    assert_eq!(src.children[1].name, "main.rs");
}

#[test]
fn test_file_tree_build_move_variant() {
    let changes = vec![FileChange::Move("old/path.rs".into(), "new/path.rs".into())];
    let nodes = FileTreeNode::build(&changes);

    // Move uses the destination path (new/).
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].is_dir);
    assert_eq!(nodes[0].name, "new");
    assert_eq!(nodes[0].children[0].name, "path.rs");
}

#[test]
fn test_file_tree_flatten_respects_collapse() {
    let changes = vec![
        FileChange::Add("src/main.rs".into()),
        FileChange::Add("README.md".into()),
    ];
    let mut nodes = FileTreeNode::build(&changes);

    // Collapsed src/ should hide main.rs.
    nodes[0].expanded = false;
    let flat = FileTreeNode::flatten(&nodes);
    assert_eq!(flat.len(), 2, "collapsed dir hides children");
    assert_eq!(flat[0].0.name, "src");
    assert_eq!(flat[1].0.name, "README.md");

    // Expanded: src/, src/main.rs, README.md
    nodes[0].expanded = true;
    let flat = FileTreeNode::flatten(&nodes);
    assert_eq!(flat.len(), 3);
    assert_eq!(flat[1].0.name, "main.rs");
    assert_eq!(flat[1].1, 1, "child depth should be 1");
}

// ---------------------------------------------------------------------------
// 7. Config::load — partial TOML via env var
// ---------------------------------------------------------------------------

#[test]
fn test_config_load_partial_toml() {
    use gitgraph::config::{Config, GraphStyle, GraphWidth, OrderType};

    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("test_config.toml");

    // Only override a subset of fields; rest must use defaults.
    std::fs::write(
        &config_path,
        r#"
[core]
order = "topo"
graph_width = "double"
graph_style = "angular"

[ui.list]
subject_min_width = 42
"#,
    )
    .unwrap();

    // Serialise the env var access — set_var is not thread-safe.
    let _lock = CWD_MUTEX.lock().unwrap();
    std::env::set_var("GITGRAPH_CONFIG_FILE", config_path.to_str().unwrap());
    let result = Config::load();
    std::env::remove_var("GITGRAPH_CONFIG_FILE");
    drop(_lock);

    let config = result.expect("Config::load should succeed with partial TOML");

    assert_eq!(config.core.order, OrderType::Topo);
    assert_eq!(config.core.graph_width, GraphWidth::Double);
    assert_eq!(config.core.graph_style, GraphStyle::Angular);
    // Override field.
    assert_eq!(config.ui.list.subject_min_width, 42);
    // Untouched fields keep their defaults.
    assert_eq!(config.ui.list.date_format, "%Y-%m-%d");
    assert_eq!(config.ui.detail.metadata_height, 8);
    assert_eq!(config.ui.refs.width, 26);
}

#[test]
fn test_config_load_defaults_when_no_file() {
    use gitgraph::config::{Config, GraphStyle, GraphWidth, OrderType};

    // Point the env var at a path that doesn't exist.
    let _lock = CWD_MUTEX.lock().unwrap();
    std::env::set_var("GITGRAPH_CONFIG_FILE", "/tmp/__gitgraph_nonexistent_config__.toml");
    let result = Config::load();
    std::env::remove_var("GITGRAPH_CONFIG_FILE");
    drop(_lock);

    let config = result.expect("Config::load should succeed with missing file (use defaults)");

    assert_eq!(config.core.order, OrderType::Chrono);
    assert_eq!(config.core.graph_width, GraphWidth::Auto);
    assert_eq!(config.core.graph_style, GraphStyle::Rounded);
    assert_eq!(config.ui.list.subject_min_width, 20);
    assert_eq!(config.ui.detail.metadata_height, 8);
}
