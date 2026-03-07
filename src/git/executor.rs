use anyhow::{bail, Result};
use std::process::Command;

pub fn check_git_repository() -> Result<()> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--git-dir")
        .output()?;

    if !output.status.success() {
        bail!("not a git repository (or any of the parent directories)");
    }

    Ok(())
}
