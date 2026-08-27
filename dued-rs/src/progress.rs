use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::embed::use_stub;
use crate::paths::{db_path, report_root};
use crate::VERSION;

const BAR_WIDTH: usize = 22;

pub fn quiet() -> bool {
    matches!(std::env::var("DUED_QUIET").ok().as_deref(), Some("1") | Some("true"))
}

pub fn set_quiet(on: bool) {
    if on {
        std::env::set_var("DUED_QUIET", "1");
    }
}

pub fn stage(msg: &str) {
    if quiet() {
        return;
    }
    eprintln!("==> {msg}");
}

pub fn note(msg: &str) {
    if quiet() {
        return;
    }
    eprintln!("    {msg}");
}

pub fn progress_line(msg: &str) {
    if quiet() {
        return;
    }
    eprint!("\r    {msg}   ");
    let _ = io::stderr().flush();
}

pub fn progress_done() {
    if quiet() {
        return;
    }
    eprintln!();
}

pub struct Bar {
    title: String,
    total: usize,
    done: usize,
    last: Instant,
    drawn: bool,
    closed: bool,
}

impl Bar {
    pub fn new(title: impl Into<String>, total: usize) -> Self {
        let mut bar = Self {
            title: title.into(),
            total,
            done: 0,
            last: Instant::now() - Duration::from_secs(1),
            drawn: false,
            closed: false,
        };
        if total > 0 {
            bar.draw("");
        }
        bar
    }

    pub fn tick(&mut self, label: &str) {
        self.done = self.done.saturating_add(1);
        self.maybe_draw(label);
    }

    pub fn set(&mut self, done: usize, label: &str) {
        self.done = done;
        self.maybe_draw(label);
    }

    fn maybe_draw(&mut self, label: &str) {
        if self.total == 0 {
            return;
        }
        let force = self.done >= self.total || !self.drawn;
        if force || self.last.elapsed() >= Duration::from_millis(80) {
            self.draw(label);
        }
    }

    fn draw(&mut self, label: &str) {
        if quiet() {
            return;
        }
        let total = self.total.max(1);
        let done = self.done.min(self.total);
        let filled = (done * BAR_WIDTH) / total;
        let mut bar = String::new();
        for i in 0..BAR_WIDTH {
            bar.push(if i < filled { '#' } else { '-' });
        }
        let pct = (done * 100) / total;
        let label = sanitize(label, 48);
        eprint!(
            "\r    {:<12} [{bar}] {done:>5}/{:<5} {pct:>3}%  {label}    ",
            self.title,
            self.total
        );
        let _ = io::stderr().flush();
        self.last = Instant::now();
        self.drawn = true;
    }

    pub fn finish(mut self) {
        if !quiet() && self.total > 0 && self.done < self.total {
            self.done = self.total;
            self.draw("");
        }
        self.close();
    }

    fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if quiet() || !self.drawn {
            return;
        }
        eprintln!();
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        self.close();
    }
}

fn sanitize(label: &str, max: usize) -> String {
    let flat: String = label.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    if flat.chars().count() <= max {
        return flat;
    }
    flat.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

pub fn banner(repo: &Path, command: &str, with_embed: bool, with_git: bool, model: &str) {
    if quiet() {
        return;
    }
    eprintln!("dued {VERSION}");
    eprintln!("repo     {}", repo.display());
    eprintln!("index    {}", db_path(repo).display());
    eprintln!("reports  {}", report_root(repo).display());
    eprintln!("command  {command}");
    let mut steps = vec!["walk source files", "parse + metrics", "build call graph"];
    if with_git {
        steps.push("git history");
    }
    steps.push("rank + name health");
    if with_embed {
        if use_stub(model) {
            steps.push("embed symbols (stub vectors)");
        } else {
            steps.push("embed symbols (Jina, local ONNX)");
        }
    } else {
        steps.push("skip embeddings");
    }
    steps.push("write reports");
    eprintln!("plan     {}", steps.join(" → "));
    if with_embed && !use_stub(model) {
        eprintln!("note     first Jina load downloads ONNX weights (can take several minutes)");
        eprintln!("note     later runs reuse the Hugging Face cache");
    }
    eprintln!("note     use --json for machine output; dued report to re-read results");
    eprintln!();
}
