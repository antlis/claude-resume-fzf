# claude-resume-fzf

> Jump back into any Claude Code session, from anywhere, in two keystrokes.

[![Rust](https://img.shields.io/badge/built%20with-Rust-000000?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`claude-resume-fzf` is a tiny, fast Rust CLI that finds **every** [Claude Code](https://claude.ai/code)
session on your machine and lets you fuzzy-search and resume any of them with
[`fzf`](https://github.com/junegunn/fzf) — no matter which directory you're currently in.

```
session> auth
  2h   ~/work/api        ▸ add JWT refresh + rotate the signing keys
  1d   ~/dotfiles        ▸ why does my zsh prompt lag on git repos
  3d   ~/work/api        ▸ the login handler returns 500 on empty body
┌──────────────────────────────────────────────────────────────────┐
│ dir:    ~/work/api                                                 │
│ branch: feat/jwt                                                   │
│ claude: 2.1.197                                                    │
│ ─────────────────────────────────────                             │
│ you:    add JWT refresh + rotate the signing keys                  │
│ claude: I'll add a /auth/refresh endpoint and a rotation job...    │
└──────────────────────────────────────────────────────────────────┘
```

## Why

Claude Code stores each session under `~/.claude/projects/<encoded-cwd>/<id>.jsonl`,
and `claude --resume` only lists sessions for the folder you're standing in. If you
remember *what* you were working on but not *where*, or you want to hop between
projects, you're stuck `cd`-ing around and squinting at UUIDs.

This tool flips it around: **search by what you said, not where you were.** Pick a
session and it drops you into the right directory and resumes it for you.

## Features

- 🔎 **Global search** — every session across every project, newest first.
- ⌨️ **Fuzzy find** by title and directory — or the *entire transcript* with `-a`.
- 👁️ **Live preview** — read the actual conversation, with your query highlighted.
- 🚀 **Zero-config** — reads Claude Code's own session files; nothing to set up.
- 🦀 **Fast & tiny** — single static-ish Rust binary, one dependency.

## Install

From source (requires a Rust toolchain):

```sh
cargo install --git https://github.com/antlis/claude-resume-fzf
```

Or clone and build:

```sh
git clone https://github.com/antlis/claude-resume-fzf
cd claude-resume-fzf
cargo install --path .
```

Both install two binaries: `claude-resume-fzf` and the short alias **`ccresume`**.

## Usage

```sh
ccresume          # search by title + directory (default)
ccresume -a       # also fuzzy-search the full conversation transcript
```

- **Type** to fuzzy-search. By default this matches each session's title (the one
  Claude generates) and its directory.
- Pass **`-a`** / **`--all`** to also search everything ever said in the session —
  your prompts and Claude's replies. Handy when you remember *what* you discussed
  but not the title (e.g. "that time I was messing with `yazi`").
- The **preview pane** shows the conversation for the highlighted session, with
  your search terms highlighted.
- **`Enter`** — `cd` into the session's directory and run `claude --resume <id>`.
- **`Esc`** — quit, do nothing.

## Requirements

- [`fzf`](https://github.com/junegunn/fzf) on your `PATH`
- [`claude`](https://claude.ai/code) (Claude Code CLI) on your `PATH`
- Linux/macOS (uses `exec` to hand off to `claude`)

## How it works

1. Scans `~/.claude/projects/*/*.jsonl`.
2. For each session file, extracts the working directory, Claude's generated
   title (falling back to the first user prompt), and the last-modified time.
   Sessions with no recorded `cwd` fall back to decoding the folder name, but only
   if that path still exists.
3. Feeds a formatted, tab-delimited list into `fzf`; hidden columns carry the raw
   `cwd`, session id, and file path. With `-a`, a flattened transcript blob is
   appended off-screen so `fzf` searches the whole conversation.
4. The preview pane is rendered by re-invoking the binary
   (`--preview <file> <query>`), which pretty-prints the conversation turns and
   highlights the current search terms.
5. On selection it `chdir`s to the session's directory and `exec`s
   `claude --resume <id>`, replacing itself with Claude Code.

## Contributing

Issues and PRs welcome. It's a small, single-file codebase (`src/lib.rs`) — easy to
read and hack on.

## License

[MIT](LICENSE)
