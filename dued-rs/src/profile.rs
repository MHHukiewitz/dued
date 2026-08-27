use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub fn ingest_profile(conn: &Connection, profile_path: &Path) -> Value {
    let text = fs::read_to_string(profile_path).unwrap_or_else(|_| "{}".into());
    let data: Value = serde_json::from_str(&text).unwrap_or(json!({}));
    let mut weights: HashMap<String, f64> = HashMap::new();
    if data.get("shared").is_some() && data.get("profiles").is_some() {
        let frames = data["shared"]["frames"].as_array().cloned().unwrap_or_default();
        let samples = data["profiles"][0]["samples"].as_array().cloned().unwrap_or_default();
        let wlist = data["profiles"][0]["weights"].as_array().cloned().unwrap_or_default();
        for (i, sample) in samples.iter().enumerate() {
            let weight = wlist.get(i).and_then(|v| v.as_f64()).unwrap_or(1.0);
            if let Some(frame_id) = sample.as_array().and_then(|a| a.last()).and_then(|v| v.as_u64()) {
                if let Some(name) = frames.get(frame_id as usize).and_then(|f| f["name"].as_str()) {
                    *weights.entry(name.to_string()).or_insert(0.0) += weight;
                }
            }
        }
    } else if data.get("nodes").is_some() {
        if let Some(nodes) = data["nodes"].as_array() {
            for node in nodes {
                let name = node["callFrame"]["functionName"]
                    .as_str()
                    .or_else(|| node["name"].as_str())
                    .unwrap_or("");
                let hit = node["hitCount"].as_f64().unwrap_or(0.0);
                if !name.is_empty() {
                    *weights.entry(name.to_string()).or_insert(0.0) += hit;
                }
            }
        }
    }
    let mut applied = 0;
    for (name, weight) in &weights {
        let short = name.rsplit('.').next().unwrap_or(name).rsplit("::").next().unwrap_or(name);
        let mut stmt = conn.prepare("SELECT id, file_id FROM symbols WHERE name = ?").unwrap();
        for row in stmt.query_map([short], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).unwrap().flatten() {
            conn.execute(
                "UPDATE files SET profile_self = profile_self + ?, profile_total = profile_total + ? WHERE id = ?",
                params![weight, weight, row.1],
            )
            .ok();
            applied += 1;
        }
    }
    json!({"frames": weights.len(), "mapped": applied, "path": profile_path.display().to_string()})
}

pub fn launch_or_attach(
    repo: &Path,
    lang: &str,
    pid: Option<i32>,
    command: &[String],
    dest: &Path,
    duration: i32,
) -> Result<PathBuf, String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok();
    }
    match lang {
        "python" => {
            let spy = which("py-spy").ok_or_else(|| {
                "py-spy is not on PATH. Install py-spy, then: py-spy record --format speedscope -o profile.json -- python app.py".to_string()
            })?;
            let mut args = vec![
                "record".into(),
                "--format".into(),
                "speedscope".into(),
                "-o".into(),
                dest.display().to_string(),
                "-d".into(),
                duration.to_string(),
            ];
            if let Some(pid) = pid {
                args.push("--pid".into());
                args.push(pid.to_string());
            } else {
                if command.is_empty() {
                    return Err("pass a command after -- or use --pid".into());
                }
                args.push("--".into());
                args.extend(command.iter().cloned());
            }
            let status = Command::new(spy).args(args).current_dir(repo).status();
            if !status.map(|s| s.success()).unwrap_or(false) {
                return Err("py-spy failed".into());
            }
        }
        "ts" | "typescript" | "js" | "node" => {
            if pid.is_some() {
                return Err("Node attach by pid is not wired. Launch with: dued profile --lang ts -- node app.js".into());
            }
            let node = which("node").ok_or_else(|| "node is not on PATH. Capture with: node --cpu-prof app.js".to_string())?;
            if command.is_empty() {
                return Err("pass a node command after --".into());
            }
            let inspector = dest.with_extension("cpuprofile");
            let mut args = vec![
                "--cpu-prof".into(),
                "--cpu-prof-name".into(),
                inspector.file_name().unwrap().to_string_lossy().into_owned(),
            ];
            if command[0] == "node" {
                args.extend(command[1..].iter().cloned());
            } else {
                args.extend(command.iter().cloned());
            }
            let status = Command::new(node).args(args).current_dir(dest.parent().unwrap_or(repo)).status();
            if !status.map(|s| s.success()).unwrap_or(false) {
                return Err("node profiler failed".into());
            }
            if inspector.is_file() {
                fs::copy(&inspector, dest).ok();
            } else {
                fs::write(dest, r#"{"type":"node-cpu","frames":[]}"#).ok();
            }
        }
        "rust" => {
            return Err("install samply or cargo-flamegraph. Capture with: samply record cargo run".into());
        }
        other => return Err(format!("unsupported profile language: {other}")),
    }
    Ok(dest.to_path_buf())
}

fn which(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}
