use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// One resumable Claude Code session.
struct Session {
    id: String,
    cwd: String,
    prompt: String,
    mtime: u64,
    file: PathBuf,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME not set"))
}

fn projects_dir() -> PathBuf {
    home().join(".claude/projects")
}

/// Shorten a path by replacing the home prefix with `~`.
fn tilde(path: &str) -> String {
    let h = home();
    let h = h.to_string_lossy();
    match path.strip_prefix(h.as_ref()) {
        Some(rest) => format!("~{rest}"),
        None => path.to_string(),
    }
}

/// Extract plain text from a user message's `content` field.
fn text_from_content(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(blocks) => {
            let mut out = String::new();
            for b in blocks {
                // Skip tool results / non-text blocks.
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = b.get("text").and_then(Value::as_str) {
                        out.push_str(t);
                    }
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}

/// Read a session file, pulling out cwd and the first real user prompt.
fn parse_session(file: PathBuf) -> Option<Session> {
    let id = file.file_stem()?.to_string_lossy().into_owned();
    let mtime = fs::metadata(&file)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let f = fs::File::open(&file).ok()?;
    let reader = BufReader::new(f);

    let mut cwd: Option<String> = None;
    let mut prompt: Option<String> = None;

    for line in reader.lines().map_while(Result::ok) {
        if cwd.is_some() && prompt.is_some() {
            break;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                cwd = Some(c.to_string());
            }
        }
        if prompt.is_none()
            && v.get("type").and_then(Value::as_str) == Some("user")
            && v.get("isSidechain").and_then(Value::as_bool) != Some(true)
        {
            if let Some(content) = v.pointer("/message/content") {
                if let Some(t) = text_from_content(content) {
                    let t = t.trim();
                    // Ignore command wrappers / meta noise.
                    if !t.is_empty() && !t.starts_with("<") && !t.starts_with("Caveat:") {
                        prompt = Some(t.to_string());
                    }
                }
            }
        }
    }

    let cwd = cwd?;
    Some(Session {
        id,
        cwd,
        prompt: prompt.unwrap_or_else(|| "(no prompt)".to_string()),
        mtime,
        file,
    })
}

fn collect_sessions() -> Vec<Session> {
    let mut sessions = Vec::new();
    let dir = projects_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return sessions,
    };
    for proj in entries.flatten() {
        let p = proj.path();
        if !p.is_dir() {
            continue;
        }
        if let Ok(files) = fs::read_dir(&p) {
            for f in files.flatten() {
                let fp = f.path();
                if fp.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(s) = parse_session(fp) {
                        sessions.push(s);
                    }
                }
            }
        }
    }
    sessions.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    sessions
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rel_time(mtime: u64) -> String {
    let now = now_secs();
    let d = now.saturating_sub(mtime);
    if d < 60 {
        format!("{d}s")
    } else if d < 3600 {
        format!("{}m", d / 60)
    } else if d < 86400 {
        format!("{}h", d / 3600)
    } else {
        format!("{}d", d / 86400)
    }
}

fn truncate(s: &str, max: usize) -> String {
    let one_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > max {
        let t: String = one_line.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    } else {
        one_line
    }
}

// ANSI colors for the fzf list.
const DIM: &str = "\x1b[90m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

/// Interactive picker: list sessions in fzf, then resume the chosen one.
pub fn run() -> ! {
    let sessions = collect_sessions();
    if sessions.is_empty() {
        eprintln!("No Claude sessions found under {}", projects_dir().display());
        std::process::exit(1);
    }

    let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("claude-resume-fzf"));

    // Build tab-delimited input: display col + hidden cwd/id/file cols.
    let mut input = String::new();
    for s in &sessions {
        let display = format!(
            "{DIM}{:>4}{RESET}  {CYAN}{}{RESET}  {}",
            rel_time(s.mtime),
            tilde(&s.cwd),
            truncate(&s.prompt, 80),
        );
        input.push_str(&format!(
            "{display}\t{}\t{}\t{}\n",
            s.cwd,
            s.id,
            s.file.display()
        ));
    }

    let preview_cmd = format!("{} --preview {{4}}", self_exe.display());

    let mut child = Command::new("fzf")
        .args([
            "--ansi",
            "--delimiter=\t",
            "--with-nth=1",
            "--no-hscroll",
            "--prompt=session> ",
            "--height=100%",
            "--layout=reverse",
            "--preview-window=down:60%:wrap",
        ])
        .arg("--preview")
        .arg(&preview_cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("failed to launch fzf (is it installed?): {e}");
            std::process::exit(1);
        });

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("write to fzf");

    let out = child.wait_with_output().expect("wait fzf");
    if !out.status.success() {
        // User pressed ESC / no selection.
        std::process::exit(130);
    }

    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.trim_end_matches('\n');
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 3 {
        eprintln!("unexpected fzf output");
        std::process::exit(1);
    }
    let cwd = cols[1];
    let id = cols[2];

    if !Path::new(cwd).is_dir() {
        eprintln!("session cwd no longer exists: {cwd}");
        std::process::exit(1);
    }

    eprintln!("→ cd {cwd} && claude --resume {id}");
    let err = Command::new("claude")
        .arg("--resume")
        .arg(id)
        .current_dir(cwd)
        .exec();
    // exec only returns on failure.
    eprintln!("failed to launch claude: {err}");
    std::process::exit(1);
}

/// Render a readable preview of a session file for fzf's preview pane.
pub fn preview(file: &str) {
    let f = match fs::File::open(file) {
        Ok(f) => f,
        Err(e) => {
            println!("cannot read session: {e}");
            return;
        }
    };
    let reader = BufReader::new(f);

    let mut header_printed = false;
    let mut turns = 0usize;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in reader.lines().map_while(Result::ok) {
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if !header_printed {
            if let Some(cwd) = v.get("cwd").and_then(Value::as_str) {
                let _ = writeln!(out, "{CYAN}dir:{RESET}    {}", tilde(cwd));
                if let Some(b) = v.get("gitBranch").and_then(Value::as_str) {
                    if !b.is_empty() {
                        let _ = writeln!(out, "{CYAN}branch:{RESET} {b}");
                    }
                }
                if let Some(ver) = v.get("version").and_then(Value::as_str) {
                    let _ = writeln!(out, "{CYAN}claude:{RESET} {ver}");
                }
                let _ = writeln!(out, "{DIM}{}{RESET}", "─".repeat(40));
                header_printed = true;
            }
        }

        let ty = v.get("type").and_then(Value::as_str);
        let role_label = match ty {
            Some("user") if v.get("isSidechain").and_then(Value::as_bool) != Some(true) => {
                Some(("you", "\x1b[33m"))
            }
            Some("assistant") => Some(("claude", "\x1b[32m")),
            _ => None,
        };
        if let Some((label, color)) = role_label {
            if let Some(content) = v.pointer("/message/content") {
                if let Some(t) = text_from_content(content) {
                    let t = t.trim();
                    if !t.is_empty() && !t.starts_with('<') {
                        let _ = writeln!(out, "{color}{label}:{RESET} {}", truncate(t, 300));
                        turns += 1;
                        if turns >= 40 {
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Shared entry point used by both binaries. `args` excludes the program name.
pub fn main_with_args(args: Vec<String>) -> ! {
    match args.first().map(String::as_str) {
        Some("--preview") => {
            if let Some(file) = args.get(1) {
                preview(file);
            }
            std::process::exit(0);
        }
        Some("-h") | Some("--help") => {
            println!(
                "claude-resume-fzf — fuzzy-find and resume Claude Code sessions\n\n\
                 Usage: claude-resume-fzf   (alias: ccresume)\n\n\
                 Lists every session under ~/.claude/projects in fzf, newest first.\n\
                 Select one to cd into its directory and run `claude --resume`."
            );
            std::process::exit(0);
        }
        _ => run(),
    }
}
