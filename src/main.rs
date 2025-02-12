use colored::*;
use std::process::Command;

#[derive(Clone, Debug)]
enum GitClient {
    GitLab,
    GitHub,
}

fn is_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map_or(false, |output| output.status.success())
}

fn get_remote_url() -> Option<String> {
    Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
}

fn get_client() -> GitClient {
    if let Some(url) = get_remote_url() {
        if url.contains("gitlab") {
            GitClient::GitLab
        } else {
            GitClient::GitHub
        }
    } else {
        GitClient::GitHub
    }
}

fn get_prs(client: &GitClient) -> Result<Vec<u8>, std::io::Error> {
    match client {
        GitClient::GitLab => Command::new("glab")
            .args(["mr", "list", "--author=@me"])
            .output()
            .map(|output| output.stdout),
        GitClient::GitHub => Command::new("gh")
            .args(["pr", "list", "--author=@me"])
            .output()
            .map(|output| output.stdout),
    }
}

struct PullRequest {
    id: String,
    title: String,
}

fn clean_gitlab_title(title: &str) -> String {
    if let Some(paren_pos) = title.find('(') {
        let clean_title = title[..paren_pos].trim();
        clean_title
            .split_whitespace()
            .skip_while(|&part| part.contains('/') || part.starts_with('!'))
            .collect::<Vec<&str>>()
            .join(" ")
    } else {
        title.to_string()
    }
}

fn collect_gitlab_prs(output_str: &str) -> Vec<PullRequest> {
    let mut prs = Vec::new();
    for line in output_str.lines() {
        if line.is_empty() || line.starts_with("breeze-front-end") || line.starts_with("Showing") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let id = parts[0].to_string();
            let title = clean_gitlab_title(line);

            if !id.is_empty() && !title.is_empty() {
                prs.push(PullRequest { id, title });
            }
        }
    }
    prs
}

fn collect_github_prs(output_str: &str) -> Vec<PullRequest> {
    let mut prs = Vec::new();
    for line in output_str.lines() {
        if line.starts_with("Showing") || line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let id = parts[0].trim_start_matches('#').to_string();
            let title = parts[1].trim().to_string();
            prs.push(PullRequest { id, title });
        }
    }
    prs
}

fn display_prs(prs: Vec<PullRequest>) {
    let max_id_width = prs
        .iter()
        .map(|pr| pr.id.len())
        .max()
        .unwrap_or(2)
        .max("ID".len());

    let max_title_width = prs
        .iter()
        .map(|pr| pr.title.len())
        .max()
        .unwrap_or(5)
        .max("TITLE".len());

    let id_padding = 2;
    let id_column_width = max_id_width + id_padding;

    println!(
        "{:<width$} {}",
        "ID".white(),
        "TITLE".white(),
        width = id_column_width
    );

    print!("{}", "─".repeat(max_id_width).white());
    print!("{}", " ".repeat(id_padding));
    print!(" "); // Added space to align with title column
    println!("{}", "─".repeat(max_title_width).white());

    for pr in prs {
        println!(
            "{:<width$} {}",
            pr.id.green(),
            pr.title.blue(),
            width = id_column_width
        );
    }
}

fn main() {
    if !is_git_repo() {
        std::process::exit(0);
    }

    let client = get_client();
    if let Ok(output) = get_prs(&client) {
        if let Ok(output_str) = String::from_utf8(output) {
            let prs = match client {
                GitClient::GitLab => collect_gitlab_prs(&output_str),
                GitClient::GitHub => collect_github_prs(&output_str),
            };
            display_prs(prs);
        }
    }
}
