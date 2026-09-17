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

fn get_prs(client: &GitClient) -> Result<std::process::Output, std::io::Error> {
    let (bin, args) = match client {
        GitClient::GitLab => ("glab", ["mr", "list", "--author=@me"]),
        GitClient::GitHub => ("gh", ["pr", "list", "--author=@me"]),
    };
    Command::new(bin).args(args).output()
}

impl GitClient {
    fn cli(&self) -> &'static str {
        match self {
            GitClient::GitLab => "glab",
            GitClient::GitHub => "gh",
        }
    }

    fn noun(&self) -> &'static str {
        match self {
            GitClient::GitLab => "MRs",
            GitClient::GitHub => "PRs",
        }
    }
}

/// "owner/repo" from either an SSH or HTTPS remote URL.
fn repo_slug(url: &str) -> String {
    let cleaned = url.trim_end_matches(".git").replace(':', "/");
    let segs: Vec<&str> = cleaned.split('/').filter(|s| !s.is_empty()).collect();
    match segs.len() {
        0 | 1 => cleaned,
        n => format!("{}/{}", segs[n - 2], segs[n - 1]),
    }
}

fn current_branch() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Shown instead of an empty table when you have nothing open.
fn display_repo_summary(client: &GitClient) {
    let slug = get_remote_url().map(|u| repo_slug(&u));
    println!(
        "{}  {}",
        slug.as_deref().unwrap_or("(no origin remote)").blue(),
        current_branch().unwrap_or_default().green()
    );
    println!("no open {} authored by you", client.noun());
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

fn terminal_columns() -> usize {
    if let Ok(c) = std::env::var("COLUMNS") {
        if let Ok(n) = c.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    tty_columns().unwrap_or(80)
}

/// Ask the tty for its size via `TIOCGWINSZ`. Works for the snacks.nvim
/// dashboard PTY (and any real terminal) without an extra crate.
#[cfg(unix)]
fn tty_columns() -> Option<usize> {
    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        _x: u16,
        _y: u16,
    }

    // TIOCGWINSZ: macOS/BSD use the _IOR encoding; Linux uses 0x5413.
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
    const TIOCGWINSZ: std::os::raw::c_ulong = 0x4008_7468;
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd")))]
    const TIOCGWINSZ: std::os::raw::c_ulong = 0x5413;

    extern "C" {
        fn ioctl(fd: std::os::raw::c_int, req: std::os::raw::c_ulong, ...) -> std::os::raw::c_int;
    }

    let mut ws = Winsize {
        ws_row: 0,
        ws_col: 0,
        _x: 0,
        _y: 0,
    };
    // Prefer stdout (the PTY snacks attaches), then stderr, then stdin.
    for fd in [1, 2, 0] {
        let rc = unsafe { ioctl(fd, TIOCGWINSZ, &mut ws as *mut Winsize) };
        if rc == 0 && ws.ws_col > 0 {
            return Some(ws.ws_col as usize);
        }
    }
    None
}

#[cfg(not(unix))]
fn tty_columns() -> Option<usize> {
    None
}

/// Soft-wrap `text` into lines of at most `width` display characters,
/// preferring breaks at spaces. `width == 0` yields the text unchanged.
fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.chars().count() <= width {
        return vec![text.to_string()];
    }

    let chars: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let rest = chars.len() - start;
        if rest <= width {
            lines.push(chars[start..].iter().collect());
            break;
        }

        let end = start + width;
        let break_at = chars[start..end]
            .iter()
            .rposition(|&c| c == ' ')
            .filter(|&i| i > 0)
            .map(|i| start + i)
            .unwrap_or(end);

        let line: String = chars[start..break_at].iter().collect();
        lines.push(line.trim_end().to_string());

        start = break_at;
        while start < chars.len() && chars[start] == ' ' {
            start += 1;
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn display_prs(prs: Vec<PullRequest>) {
    let max_id_width = prs
        .iter()
        .map(|pr| pr.id.chars().count())
        .max()
        .unwrap_or(2)
        .max("ID".len());

    let id_padding = 2;
    let id_column_width = max_id_width + id_padding;
    // gap is the single space between the ID cell and the title cell
    let gap = 1;

    let cols = terminal_columns();
    let title_col_width = cols
        .saturating_sub(id_column_width + gap)
        .max("TITLE".len());

    let content_title_width = prs
        .iter()
        .map(|pr| pr.title.chars().count())
        .max()
        .unwrap_or(5)
        .max("TITLE".len());
    // underline only as far as content, and never past the column edge
    let title_rule_width = content_title_width.min(title_col_width);

    // Format plain text first, then color — ANSI codes must not eat padding width.
    let id_header = format!("{:<width$}", "ID", width = id_column_width);
    println!("{} {}", id_header.white(), "TITLE".white());

    print!("{}", "─".repeat(max_id_width).white());
    print!("{}", " ".repeat(id_padding + gap));
    println!("{}", "─".repeat(title_rule_width).white());

    let hang = " ".repeat(id_column_width + gap);
    for pr in prs {
        let title_lines = wrap_to_width(&pr.title, title_col_width);
        for (i, line) in title_lines.iter().enumerate() {
            if i == 0 {
                let id_cell = format!("{:<width$}", pr.id, width = id_column_width);
                println!("{} {}", id_cell.green(), line.blue());
            } else {
                println!("{}{}", hang, line.blue());
            }
        }
    }
}

fn main() {
    if !is_git_repo() {
        std::process::exit(0);
    }

    let client = get_client();
    let output = match get_prs(&client) {
        Ok(o) => o,
        // Don't exit silently — the usual cause is the CLI not being installed.
        Err(e) => {
            eprintln!("could not run `{}`: {}", client.cli(), e);
            std::process::exit(1);
        }
    };

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "`{}` failed: {}",
            client.cli(),
            err.trim().lines().next().unwrap_or("unknown error")
        );
        std::process::exit(1);
    }

    let output_str = String::from_utf8_lossy(&output.stdout).into_owned();
    let prs = match client {
        GitClient::GitLab => collect_gitlab_prs(&output_str),
        GitClient::GitHub => collect_github_prs(&output_str),
    };

    if prs.is_empty() {
        display_repo_summary(&client);
    } else {
        display_prs(prs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_handles_ssh_and_https() {
        assert_eq!(repo_slug("git@github.com:jellis206/mrprlist.git"), "jellis206/mrprlist");
        assert_eq!(repo_slug("https://github.com/jellis206/carillon.git"), "jellis206/carillon");
        assert_eq!(repo_slug("git@gitlab.com:acme/breeze-front-end.git"), "acme/breeze-front-end");
    }

    #[test]
    fn parses_gh_tsv() {
        // `gh pr list` emits tab-separated columns when not a tty.
        let out = "12\tFix the flaky retry path\tfix-retry\tOPEN\n\
                   7\tAdd tunnel support\ttunnel\tDRAFT\n";
        let prs = collect_github_prs(out);
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].id, "12");
        assert_eq!(prs[0].title, "Fix the flaky retry path");
        assert_eq!(prs[1].id, "7");
    }

    #[test]
    fn parses_glab_list() {
        let out = "Showing 2 open merge requests on breeze-front-end (Page 1)\n\n\
                   !451  feat/new-picker  Add the new date picker (3 days ago)\n";
        let prs = collect_gitlab_prs(out);
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].id, "!451");
        assert_eq!(prs[0].title, "Add the new date picker");
    }

    #[test]
    fn empty_input_yields_no_prs() {
        assert!(collect_github_prs("").is_empty());
        assert!(collect_gitlab_prs("").is_empty());
    }

    #[test]
    fn wrap_keeps_short_text() {
        assert_eq!(wrap_to_width("hello", 10), vec!["hello".to_string()]);
    }

    #[test]
    fn wrap_breaks_on_spaces() {
        let lines = wrap_to_width("fix the flaky retry path now", 12);
        assert_eq!(
            lines,
            vec![
                "fix the".to_string(),
                "flaky retry".to_string(),
                "path now".to_string()
            ]
        );
    }

    #[test]
    fn wrap_hard_breaks_long_tokens() {
        let lines = wrap_to_width("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd".to_string(), "efgh".to_string(), "ij".to_string()]);
    }
}
