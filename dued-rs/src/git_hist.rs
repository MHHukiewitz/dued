use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub fn analyze_history(repo: &Path, conn: &Connection) -> Value {
    if !repo.join(".git").exists() {
        return json!({"enabled": false, "reason": "not a git repository"});
    }
    let output = Command::new("git")
        .args(["log", "--numstat", "--pretty=format:---%n%H%n%an%n%at", "--no-renames"])
        .current_dir(repo)
        .output();
    let Ok(output) = output else {
        return json!({"enabled": false, "reason": "git log failed"});
    };
    if !output.status.success() {
        return json!({"enabled": false, "reason": String::from_utf8_lossy(&output.stderr).trim().to_string()});
    }
    let log = String::from_utf8_lossy(&output.stdout);
    let mut churn: HashMap<String, i64> = HashMap::new();
    let mut authors: HashMap<String, HashSet<String>> = HashMap::new();
    let mut coupling: HashMap<(String, String), i64> = HashMap::new();
    let mut revisions: HashMap<String, i64> = HashMap::new();
    let mut times: HashMap<String, Vec<i64>> = HashMap::new();
    let mut commits = 0;
    let mut current_author = String::new();
    let mut current_time = 0i64;
    let mut files_in_commit: Vec<String> = Vec::new();
    let mut state = "sep";
    let mut log_bar = crate::progress::Bar::new("git log", log.lines().count());
    for line in log.lines() {
        log_bar.tick("");
        if line == "---" {
            flush_commit(&mut commits, &files_in_commit, &mut coupling);
            files_in_commit.clear();
            current_author.clear();
            current_time = 0;
            state = "hash";
            continue;
        }
        if state == "hash" {
            state = "author";
            continue;
        }
        if state == "author" {
            current_author = line.to_string();
            state = "time";
            continue;
        }
        if state == "time" {
            current_time = line.parse().unwrap_or(0);
            state = "stats";
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 || parts[0] == "-" || parts[1] == "-" {
            continue;
        }
        let added: i64 = parts[0].parse().unwrap_or(0);
        let deleted: i64 = parts[1].parse().unwrap_or(0);
        let path = parts[2].to_string();
        *churn.entry(path.clone()).or_insert(0) += added + deleted;
        *revisions.entry(path.clone()).or_insert(0) += 1;
        if !current_author.is_empty() {
            authors.entry(path.clone()).or_default().insert(current_author.clone());
        }
        if current_time != 0 {
            times.entry(path.clone()).or_default().push(current_time);
        }
        files_in_commit.push(path);
    }
    log_bar.finish();
    flush_commit(&mut commits, &files_in_commit, &mut coupling);
    conn.execute("DELETE FROM git_coupling", []).ok();
    let mut couple_bar = crate::progress::Bar::new("git coupling", coupling.len());
    for ((a, b), shared) in &coupling {
        couple_bar.tick("");
        let ra = *revisions.get(a).unwrap_or(&0);
        let rb = *revisions.get(b).unwrap_or(&0);
        if ra < 3 || rb < 3 || *shared < 2 {
            continue;
        }
        let strength = *shared as f64 / ra.min(rb).max(1) as f64;
        if strength < 0.3 {
            continue;
        }
        conn.execute(
            "INSERT INTO git_coupling(file_a, file_b, shared, strength) VALUES (?,?,?,?)",
            params![a, b, shared, strength],
        )
        .ok();
    }
    couple_bar.finish();
    let now = times.values().filter_map(|v| v.iter().max().copied()).max().unwrap_or(0);
    let mut stmt = conn.prepare("SELECT id, relpath FROM files").unwrap();
    let file_rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    let mut file_bar = crate::progress::Bar::new("git files", file_rows.len());
    for row in file_rows {
        let people = authors.get(&row.1).map(|s| s.len()).unwrap_or(0);
        let mut stamps = times.get(&row.1).cloned().unwrap_or_default();
        stamps.sort();
        let first = stamps.first().copied().unwrap_or(0);
        let last = stamps.last().copied().unwrap_or(0);
        let age_days = if first != 0 && now != 0 { (now - first) / 86400 } else { 0 };
        let mut by_day: HashMap<i64, i64> = HashMap::new();
        for ts in &stamps {
            *by_day.entry(ts / 86400).or_insert(0) += 1;
        }
        let bursts = by_day.values().copied().max().unwrap_or(0);
        conn.execute(
            "UPDATE files SET churn = ?, authors = ?, bus_factor = ?, bursts = ?, age_days = ?, first_seen = ?, last_seen = ? WHERE id = ?",
            params![
                churn.get(&row.1).copied().unwrap_or(0),
                people as i64,
                (people.min(9)) as i64,
                bursts,
                age_days,
                if first == 0 { String::new() } else { first.to_string() },
                if last == 0 { String::new() } else { last.to_string() },
                row.0
            ],
        )
        .ok();
        file_bar.tick(&row.1);
    }
    file_bar.finish();
    let pairs: i64 = conn.query_row("SELECT COUNT(*) FROM git_coupling", [], |r| r.get(0)).unwrap_or(0);
    json!({"enabled": true, "commits": commits, "coupled_pairs": pairs})
}

fn flush_commit(commits: &mut i64, files: &[String], coupling: &mut HashMap<(String, String), i64>) {
    if files.is_empty() {
        return;
    }
    *commits += 1;
    let mut unique = files.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() > 1 && unique.len() <= 50 {
        for i in 0..unique.len() {
            for b in unique.iter().skip(i + 1) {
                *coupling.entry((unique[i].clone(), b.clone())).or_insert(0) += 1;
            }
        }
    }
}

pub fn history_report(conn: &Connection) -> Value {
    let mut stmt = conn
        .prepare("SELECT relpath, churn, authors, bus_factor, hotspot, bursts, age_days FROM files ORDER BY churn DESC LIMIT 20")
        .unwrap();
    let hot: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "relpath": r.get::<_, String>(0)?,
                "churn": r.get::<_, i64>(1)?,
                "authors": r.get::<_, i64>(2)?,
                "bus_factor": r.get::<_, i64>(3)?,
                "hotspot": r.get::<_, f64>(4)?,
                "bursts": r.get::<_, i64>(5)?,
                "age_days": r.get::<_, i64>(6)?,
            }))
        })
        .unwrap()
        .flatten()
        .collect();
    let mut stmt = conn
        .prepare("SELECT file_a, file_b, shared, strength FROM git_coupling ORDER BY strength DESC LIMIT 20")
        .unwrap();
    let couples: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "file_a": r.get::<_, String>(0)?,
                "file_b": r.get::<_, String>(1)?,
                "shared": r.get::<_, i64>(2)?,
                "strength": r.get::<_, f64>(3)?,
            }))
        })
        .unwrap()
        .flatten()
        .collect();
    json!({"hot_files": hot, "temporal_coupling": couples})
}
