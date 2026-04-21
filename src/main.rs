use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use regex::Regex;
use std::env;
use std::path::Path;

mod error;
mod fs;
mod spinner;
mod vcs;

use crate::error::Error;
use crate::fs::make_readonly;
use crate::spinner::Spinner;
use crate::vcs::{ tf, git };

struct Workfold {
    workspace: String,
    collection: String,
    remote: String,
    local: String,
}

fn get_workfold(toplevel: &str) -> Result<Workfold, Error> {
    let tf_workfold = tf(["workfold",  &toplevel])?;
    let mut workfold_lines = tf_workfold.lines();

    let workspace_regex = Regex::new(r"\s*Workspace:\s*([\s\S]*)\s*")?;
    let workspace = workfold_lines
        .find_map(|s| workspace_regex.captures(s))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim())
        .ok_or_else(|| "Missing workspace".to_string())?;

    let collection_regex = Regex::new(r"\s*Collection:\s*([\s\S]*)\s*")?;
    let collection = workfold_lines
        .find_map(|s| collection_regex.captures(s))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim())
        .ok_or_else(|| "Missing collection".to_string())?;

    let mut path_map_expr = String::from(r"\s*(\$[^:]*)\s*:\s*(");
    path_map_expr.push_str(&regex::escape(&toplevel));
    path_map_expr.push_str(r")\s*");
    let path_map_regex = Regex::new(&path_map_expr)?;
    let path_map: (&str, &str) = workfold_lines
        .find_map(|s| path_map_regex.captures(s))
        .and_then(|caps| {
            let remote = caps.get(1)?;
            let local = caps.get(2)?;
            Some((remote.as_str().trim(), local.as_str().trim()))
        })
        .ok_or_else(|| "Missing path_map".to_string())?;

    Ok(Workfold{
        workspace: workspace.to_string(),
        collection: collection.to_string(),
        remote: path_map.0.to_string(),
        local: path_map.1.to_string() })
}

fn to_date(src: &str) -> Result<String, Error> {
    let format = "%b %d, %Y, %I:%M:%S %p";
    let naive_dt = NaiveDateTime::parse_from_str(src, format)?;
    let dt: DateTime<Local> = Local.from_local_datetime(&naive_dt)
        .single()
        .ok_or_else(|| "Invalid local datetime".to_string())?;
    Ok(String::from(dt.to_rfc3339()))
}

#[derive(Default)]
struct Commit<'a> {
    user: &'a str,
    date: &'a str,
    comment: Vec<&'a str>,
    changeset: &'a str,
}

fn fetch(version: &str) -> Result<String, Error> {
    let toplevel = git(["rev-parse", "--show-toplevel"])?;

    let spinner = Spinner::new("Get workfold");
    let workfold = get_workfold(&toplevel)?;
    spinner.stop();

    let mut workspace = String::from("-workspace:");
    workspace.push_str(&workfold.workspace);

    let mut collection = String::from("-collection:");
    collection.push_str(&workfold.collection);

    let spinner = Spinner::new("Undo workfold");
    tf(["workfold", "undo", "-recursive", &workfold.local])?;
    tf(["workfold", "uu", "-recursive", &workfold.local])?;
    spinner.stop();

    let spinner = Spinner::new("Get versions");
    let tf_vers: String;
    let vers: &str;
    if version.is_empty() {
        let vers_regex = Regex::new(r"(^[0-9]+)")?;
        tf_vers = tf(["history", &collection, "-recursive", "-stopafter:1", &workfold.remote])?;
        vers = tf_vers
            .lines()
            .find_map(|s| vers_regex.captures(s))
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().trim())
            .ok_or_else(|| "Missing version".to_string())?;
    } else {
        vers = version
    }

    let last_regex = Regex::new(r"\s*git-tf-id:\s*([0-9]+)\s*")?;
    let git_last = git(["log", "-n", "1", "tfs", "--pretty=format:%B", &workfold.local])?;
    let last: &str = git_last
        .lines()
        .find_map(|s| last_regex.captures(s))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim())
        .ok_or_else(|| "Missing last version".to_string())?;

    let rng_vers = format!("-version:{}~{}", if last.is_empty() { vers } else { last }, vers);
    let tf_history = tf(
        ["history", &workfold.remote, &rng_vers, "-noprompt", "-format:detailed", "-recursive"])?;
    spinner.stop();

    let spinner = Spinner::new("Get changesets");
    let changeset_regex = Regex::new(r"\s*Changeset:\s*([\S\s]*)\s*")?;
    let user_regex = Regex::new(r"\s*User:\s*([\S\s]*)\s*")?;
    let date_regex = Regex::new(r"\s*Date:\s*([\S\s]*)\s*")?;
    let comment_regex = Regex::new(r"\s*Comment:\s*")?;
    let items_regex = Regex::new(r"\s*Items:\s*")?;
    let no_ci_regex = Regex::new(r"\s***NO_CI***\s")?;

    let mut is_comment = false;
    let mut commit = Commit::default();
    let mut commits: Vec<Commit> = Vec::new();
    for line in tf_history.lines() {
        if is_comment {
            if let Some(_) = items_regex.find(line) {
                commits.push(commit);
                commit = Commit::default();
                is_comment = false
            }
            else
            {
                if let Some(_) = no_ci_regex.find(line) {
                    continue;
                }
                commit.comment.push(line.trim());
            }
            continue;
        }

        if let Some(caps) = changeset_regex.captures(line) {
            commit.changeset = caps.get(1)
                .ok_or_else(|| "Missing changeset".to_string())?
                .as_str().trim();
            continue;
        }

        if let Some(caps) = user_regex.captures(line) {
            commit.user = caps.get(1)
                .ok_or_else(|| "Missing user".to_string())?
                .as_str().trim();
            continue;
        }

        if let Some(caps) = date_regex.captures(line) {
            commit.date = caps.get(1)
                .ok_or_else(|| "Missing date".to_string())?
                .as_str().trim();
            continue;
        }

        if let Some(_) = comment_regex.find(line) {
            is_comment = true;
            continue;
        }
    }
    spinner.stop();

    let git_ls_files = git(["ls-files", "--full-name", &toplevel])?;
    for commit in commits.iter().rev() {
        let spinner = Spinner::new(&(String::from("Commit changesets: ") + &commit.changeset));
        for line in git_ls_files.lines() {
            let path = Path::new(&toplevel).join(&line);
            make_readonly(&path)?;
        }

        let mut vers = String::from("-version:");
        vers.push_str(commit.changeset);
        tf(["get", "-recursive", "-overwrite", &vers, "-force", &toplevel])?;

        git(["add", &toplevel])?;

        let mut comment = String::from(&commit.comment.join("\n"));
        comment.push_str("\n\ngit-tf-id: ");
        comment.push_str(commit.changeset);

        let mut date = String::from("--date=");
        date.push_str(&to_date(commit.date)?);

        let mut author = String::from("--author=");
        author.push_str(commit.user);
        author.push_str(" <noreply@topsystems.ru>");
        git(["commit", "-n", "-m", &comment, &date, &author])?;
        spinner.stop();
    }

    Ok("Changes fetched!".to_string())
}

fn fetch_tfs(version: &str) -> Result<String, Error> {
    let spinner = Spinner::new("Get current branch");
    let branch = git(["branch", "--show-current"])?;
    spinner.stop();

    let spinner = Spinner::new("Switch tfs branch");
    let stash = git(["stash", "push", "-u"])?;
    if branch != "tfs" {
        git(["switch", "tfs"])?;
    }
    spinner.stop();

    let result = fetch(version);

    let spinner = Spinner::new("Switch current branch");
    if branch != "tfs" {
        git(["switch", &branch])?;
    }
    if stash != "No local changes to save" {
        git(["stash", "pop"])?;
    }
    spinner.stop();

    result
}

fn push(msg: &str) -> Result<String, Error> {
    let spinner = Spinner::new("Get current branch");
    let branch = git(["branch", "--show-current"])?;
    let toplevel = git(["rev-parse", "--show-toplevel"])?;
    spinner.stop();

    let spinner = Spinner::new("Undo workfold");
    let workfold = get_workfold(&toplevel)?;
    spinner.stop();

    let spinner = Spinner::new("Undo workfold");
    tf(["workfold", "undo", "-recursive", &workfold.local])?;
    spinner.stop();

    let spinner = Spinner::new("Checking the possibility of making changes");
    let last_regex = Regex::new(r".([a-z0-9]*).")?;
    let git_last = git(["log", "-n", "1", "tfs", "--pretty=format:%H", &workfold.local])?;
    let last: &str = git_last
        .lines()
        .find_map(|s| last_regex.captures(s))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim())
        .ok_or_else(|| "Missing hash".to_string())?;

    let commitbranch_regex = Regex::new(&format!("\\s*({})\\s*", branch))?;
    let git_commitbranch = git(["branch", "--contains", &last])?;
    let commitbranch: &str = git_commitbranch
        .lines()
        .find_map(|s| commitbranch_regex.captures(s))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim())
        .ok_or_else(|| "Missing branch".to_string())?;

    if commitbranch.is_empty() {
        return Err(Error::new("The current branch has not merged changes from the tfs branch."));
    }
    spinner.stop();

    let spinner = Spinner::new("Get changes");
    let git_diff = git(["diff", "tfs", &branch, "-b", "--name-status"])?;
    spinner.stop();

    let modify_regex = Regex::new(r"\s*M\s*([\S\s]*)\s*")?;
    let add_regex = Regex::new(r"\s*A\s*([\S\s]*)\s*")?;
    let del_regex = Regex::new(r"\s*D\s*([\S\s]*)\s*")?;
    let move_regex = Regex::new(r"\s*R\s*[0-9]*\s*([\S\s]*)\s*->\s*([\S\s]*)\s*")?;
    let copy_regex = Regex::new(r"\s*C\s*[0-9]*\s*([\S\s]*)\s*->\s*([\S\s]*)\s*")?;
    for line in git_diff.lines() {
        if let Some(caps) = modify_regex.captures(line) {
            let repo_file = caps.get(1)
                .ok_or_else(|| "Missing change".to_string())?
                .as_str().trim();

            let spinner = Spinner::new(&(String::from("M: ") + repo_file));
            let mut file = String::from(&toplevel);
            file.push_str("/");
            file.push_str(&repo_file);
            tf(["checkout", &file])?;
            spinner.stop();
            continue;
        }

        if let Some(caps) = add_regex.captures(line) {
            let repo_file = caps.get(1)
                .ok_or_else(|| "Missing change".to_string())?
                .as_str().trim();

            let spinner = Spinner::new(&(String::from("A: ") + repo_file));
            let mut file = String::from(&toplevel);
            file.push_str("/");
            file.push_str(&repo_file);
            tf(["add", &file])?;
            spinner.stop();
            continue;
        }

        if let Some(caps) = del_regex.captures(line) {
            let repo_file = caps.get(1)
                .ok_or_else(|| "Missing change".to_string())?
                .as_str().trim();

            let spinner = Spinner::new(&(String::from("D: ") + repo_file));
            let mut file = String::from(&toplevel);
            file.push_str("/");
            file.push_str(&repo_file);
            tf(["delete", &file])?;
            spinner.stop();
            continue;
        }

        if let Some(caps) = copy_regex.captures(line) {
            let repo_file = caps.get(1)
                .ok_or_else(|| "Missing change".to_string())?
                .as_str().trim();

            let spinner = Spinner::new(&(String::from("A: ") + repo_file));
            let mut file = String::from(&toplevel);
            file.push_str("/");
            file.push_str(&repo_file);
            tf(["add", &file])?;
            spinner.stop();
            continue;
        }

        if let Some(caps) = move_regex.captures(line) {
            let repo_file1 = caps.get(1)
                .ok_or_else(|| "Missing change".to_string())?
                .as_str().trim();
            let repo_file2 = caps.get(2)
                .ok_or_else(|| "Missing change".to_string())?
                .as_str().trim();

            let spinner = Spinner::new(&(String::from("A: ") + repo_file2));
            let mut file2 = String::from(&toplevel);
            file2.push_str("/");
            file2.push_str(repo_file1);
            tf(["add", &file2])?;
            spinner.stop();

            let spinner = Spinner::new(&(String::from("D: ") + repo_file1));
            let mut file1 = String::from(&toplevel);
            file1.push_str("/");
            file1.push_str(repo_file1);
            tf(["delete", &file1])?;
            spinner.stop();

            continue;
        }
    }

    let spinner = Spinner::new("Checkin changeset");
    let mut comment = String::from("-comment:");
    if msg.is_empty() {
        comment.push_str(&git(["log", "-n", "1", "--pretty=format:%B", &workfold.local])?);
    } else {
        comment.push_str(msg);
    }
    tf(["checkin", &comment, &workfold.local])?;
    spinner.stop();

    Ok("Changes pushed!".to_string())
}

fn main() {
    let mut args = env::args();
    args.next().unwrap();

    let command = args.next().unwrap_or("".to_string());
    let arg = args.next().unwrap_or("".to_string());
    let result: Result<String, Error> = match command.as_str() {
        "fetch" => { fetch_tfs(&arg) }
        "push" => { push(&arg) }
        _ => { Err(Error::new("No command!")) }
    };

    match result {
        Ok(msg) => println!("{}", &msg),
        Err(msg) => println!("{}", &msg),
    }
}
