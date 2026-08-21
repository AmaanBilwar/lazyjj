use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug)]
pub struct Revision {
    pub change_id: String,
    pub commit_id: String,
    pub is_working_copy: bool,
    pub bookmarks: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct Bookmark {
    pub name: String,
    pub remote: Option<String>,
    pub tracked: bool,
    pub change_id: String,
}

impl Bookmark {
    pub fn symbol(&self) -> String {
        match &self.remote {
            Some(remote) => format!("{}@{remote}", self.name),
            None => self.name.clone(),
        }
    }

    pub fn is_remote(&self) -> bool {
        self.remote.is_some()
    }
}

#[derive(Debug)]
pub struct RepoSnapshot {
    pub root: PathBuf,
    pub revisions: Vec<Revision>,
    pub bookmarks: Vec<Bookmark>,
}

pub struct Jj;

impl Jj {
    pub fn load() -> Result<RepoSnapshot> {
        let root = PathBuf::from(Self::output(&["root"])?);
        let revisions = Self::revisions()?;
        let bookmarks = Self::bookmarks()?;

        Ok(RepoSnapshot {
            root,
            revisions,
            bookmarks,
        })
    }

    pub fn revisions() -> Result<Vec<Revision>> {
        let template = concat!(
            "change_id.shortest(8) ++ \"|\" ++ ",
            "commit_id.shortest(8) ++ \"|\" ++ ",
            "if(current_working_copy, \"@\", \"\") ++ \"|\" ++ ",
            "bookmarks ++ \"|\" ++ ",
            "description.first_line() ++ \"\\n\""
        );
        let output = Self::output(&[
            "log",
            "--no-graph",
            "--color",
            "never",
            "-n",
            "200",
            "-r",
            "all()",
            "-T",
            template,
        ])?;

        Ok(output
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(5, '|');
                Some(Revision {
                    change_id: fields.next()?.to_owned(),
                    commit_id: fields.next()?.to_owned(),
                    is_working_copy: fields.next()? == "@",
                    bookmarks: fields.next()?.to_owned(),
                    description: fields.next().unwrap_or_default().to_owned(),
                })
            })
            .collect())
    }

    pub fn bookmarks() -> Result<Vec<Bookmark>> {
        let template = concat!(
            "name ++ \"|\" ++ ",
            "if(remote, remote, \"\") ++ \"|\" ++ ",
            "if(tracked, \"tracked\", \"untracked\") ++ \"|\" ++ ",
            "normal_target.change_id().shortest(8) ++ \"\\n\""
        );
        let output = Self::output(&[
            "bookmark",
            "list",
            "--all-remotes",
            "--color",
            "never",
            "-T",
            template,
        ])?;

        Ok(output
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(4, '|');
                let name = fields.next()?.to_owned();
                let remote = fields
                    .next()
                    .filter(|remote| !remote.is_empty())
                    .map(str::to_owned);
                let tracked = fields.next()? == "tracked";
                let change_id = fields.next().unwrap_or_default().to_owned();
                Some(Bookmark {
                    name,
                    remote,
                    tracked,
                    change_id,
                })
            })
            .collect())
    }

    pub fn changed_files(revision: &str) -> Result<Vec<String>> {
        let output = Self::output(&["diff", "--summary", "--color", "never", "-r", revision])?;
        Ok(output.lines().map(str::to_owned).collect())
    }

    pub fn run(args: &[String]) -> Result<String> {
        let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
        Self::output(&borrowed)
    }

    fn output(args: &[&str]) -> Result<String> {
        let output = Command::new("jj")
            .args(args)
            .output()
            .with_context(|| "failed to start jj; is it installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            bail!(if stderr.is_empty() {
                format!("jj exited with {}", output.status)
            } else {
                stderr
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_holds_display_data() {
        let revision = Revision {
            change_id: "abcdefgh".into(),
            commit_id: "12345678".into(),
            is_working_copy: true,
            bookmarks: "main".into(),
            description: "demo".into(),
        };

        assert!(revision.is_working_copy);
        assert_eq!(revision.change_id, "abcdefgh");
    }
}
