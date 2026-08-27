use std::path::Path;

use serde_json::Value;

pub fn print_value(data: &Value) {
    if let Some(obj) = data.as_object() {
        if obj.contains_key("report") && obj.contains_key("files") {
            print_analyze(data);
            return;
        }
        if obj.contains_key("symbols") && obj.contains_key("files") && obj.contains_key("hollow") && !obj.contains_key("edges") {
            print_dead(data);
            return;
        }
        if obj.contains_key("blast_radius") || obj.contains_key("effects") && obj.contains_key("symbols") {
            print_slice(data);
            return;
        }
        if obj.contains_key("clones") && obj.contains_key("clusters") {
            print_cluster(data);
            return;
        }
        if obj.contains_key("summary") && obj.contains_key("coupling") || obj.contains_key("bus_factor") {
            print_history(data);
            return;
        }
        if obj.contains_key("reading_order") {
            print_brief(data, None);
            return;
        }
    }
    if let Some(arr) = data.as_array() {
        print_list(arr);
        return;
    }
    println!("{}", serde_json::to_string_pretty(data).unwrap());
}

pub fn print_analyze(data: &Value) {
    println!("Scan complete");
    println!();
    kv("files", &data["files"]);
    kv("symbols", &data["symbols"]);
    kv("edges", &data["edges"]);
    kv("parsed this run", &data["parsed"]);
    kv("reused files", &data["reused"]);
    kv("issues", &data["issues"]);
    kv("hollow stubs", &data["hollow"]);
    kv("clones", &data["clones"]);
    kv("mismatches", &data["mismatches"]);
    kv("model", &data["model"]);
    kv("elapsed seconds", &data["elapsed_seconds"]);
    if let Some(inv) = data.get("inventory") {
        if let Some(langs) = inv.get("languages").and_then(|v| v.as_array()) {
            let text = langs
                .iter()
                .filter_map(|l| Some(format!("{} ({} files)", l["language"].as_str()?, l["n"])))
                .collect::<Vec<_>>()
                .join(", ");
            if !text.is_empty() {
                println!("  {:<18} {text}", "languages");
            }
        }
    }
    if data["git"]["enabled"].as_bool() == Some(true) {
        println!("  {:<18} {} commits", "git", data["git"]["commits"]);
    }
    println!();
    if let Some(report) = data["report"].as_str() {
        let html = Path::new(report).join("report.html");
        println!("Reports");
        println!("  HTML     {}", html.display());
        println!("  folder   {report}");
        println!();
        println!("Explore");
        println!("  open the HTML file in a browser");
        println!("  dued report              re-print this summary from disk");
        println!("  dued rank                reading order");
        println!("  dued issues              flagged problems");
        println!("  dued dead                unused symbols and files");
        println!("  dued names               name-health flags");
        println!("  dued cluster             clones");
        println!("  dued slice <symbol>      behavior slice");
    }
}

pub fn print_brief(data: &Value, html: Option<&Path>) {
    println!("dued report");
    println!();
    kv("repo", &data["repo"]);
    kv("files", &data["files"]);
    kv("symbols", &data["symbols"]);
    if let Some(langs) = data["languages"].as_array() {
        let text = langs
            .iter()
            .filter_map(|l| Some(format!("{}={}", l["language"].as_str()?, l["n"])))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {:<18} {text}", "languages");
    }
    println!();
    println!("Reading order");
    if let Some(order) = data["reading_order"].as_array() {
        for (i, item) in order.iter().enumerate() {
            println!(
                "  {:>2}. {}::{}  —  {}",
                i + 1,
                item["relpath"].as_str().unwrap_or(""),
                item["name"].as_str().unwrap_or(""),
                item["why"].as_str().unwrap_or("")
            );
        }
    }
    println!();
    if let Some(issues) = data["issues"].as_array() {
        println!("Issues ({})", issues.len());
        for item in issues.iter().take(15) {
            let loc = match (item["relpath"].as_str(), item["name"].as_str()) {
                (Some(p), Some(n)) if !n.is_empty() => format!("{p}::{n}"),
                (Some(p), _) => p.to_string(),
                _ => String::new(),
            };
            println!(
                "  {:<18} {}  {}",
                item["kind"].as_str().unwrap_or(""),
                loc,
                item["detail"].as_str().unwrap_or("")
            );
        }
        println!();
    }
    if let Some(html) = html {
        println!("HTML report");
        println!("  {}", html.display());
        println!("  open that file in a browser to search and sort the full index");
        println!("  JSON tables are in the data/ folder next to the HTML file");
    }
}

pub fn print_dead(data: &Value) {
    let symbols = data["symbols"].as_array().cloned().unwrap_or_default();
    let files = data["files"].as_array().cloned().unwrap_or_default();
    let hollow = data["hollow"].as_array().cloned().unwrap_or_default();
    println!("Dead code");
    println!("  unused symbols  {}", symbols.len());
    println!("  isolated files  {}", files.len());
    println!("  hollow stubs    {}", hollow.len());
    println!();
    for item in symbols.iter().take(25) {
        println!(
            "  {}::{}  {}",
            item["relpath"].as_str().unwrap_or(""),
            item["name"].as_str().unwrap_or(""),
            item["signature"].as_str().unwrap_or("")
        );
    }
    if !files.is_empty() {
        println!();
        println!("Isolated files");
        for item in files.iter().take(20) {
            println!(
                "  {}  ({} loc)",
                item["relpath"].as_str().unwrap_or(""),
                item["loc"]
            );
        }
    }
}

pub fn print_slice(data: &Value) {
    println!("Behavior slice");
    println!();
    kv("query", &data["query"]);
    if let Some(err) = data["error"].as_str() {
        println!("  {:<18} {}", "error", err);
        if let Some(candidates) = data["candidates"].as_array() {
            if !candidates.is_empty() {
                println!();
                println!("Candidates (use path::name)");
                for item in candidates.iter().take(20) {
                    println!(
                        "  {}::{}",
                        item["relpath"].as_str().unwrap_or(""),
                        item["name"].as_str().unwrap_or("")
                    );
                }
            }
        }
        return;
    }
    kv("blast radius", &data["blast_radius"]);
    if let Some(effects) = data["effects"].as_array() {
        let tags: Vec<&str> = effects.iter().filter_map(|e| e.as_str()).collect();
        if !tags.is_empty() {
            println!("  {:<18} {}", "effects", tags.join(", "));
        }
    }
    if let Some(unresolved) = data["unresolved_callees"].as_array() {
        let tags: Vec<&str> = unresolved.iter().filter_map(|e| e.as_str()).take(12).collect();
        if !tags.is_empty() {
            println!("  {:<18} {}", "unresolved", tags.join(", "));
        }
    }
    println!();
    if let Some(symbols) = data["symbols"].as_array() {
        println!("Symbols");
        for item in symbols {
            if let Some(s) = item.as_str() {
                println!("  {s}");
            } else {
                println!(
                    "  {}::{}",
                    item["relpath"].as_str().unwrap_or(""),
                    item["name"].as_str().unwrap_or("")
                );
            }
        }
    }
    if let Some(files) = data["files"].as_array() {
        println!();
        println!("Files");
        for item in files {
            if let Some(s) = item.as_str() {
                println!("  {s}");
            }
        }
    }
}

pub fn print_cluster(data: &Value) {
    let clones = data["clones"].as_array().cloned().unwrap_or_default();
    println!("Clusters");
    println!("  clone pairs  {}", clones.len());
    println!();
    for item in clones.iter().take(20) {
        println!(
            "  {:.2}  {}  ↔  {}",
            item["score"].as_f64().unwrap_or(0.0),
            item["a"].as_str().or_else(|| item["left"].as_str()).unwrap_or(""),
            item["b"].as_str().or_else(|| item["right"].as_str()).unwrap_or("")
        );
    }
    if let Some(near) = data["similar"].as_array() {
        if !near.is_empty() {
            println!();
            println!("Similar");
            for item in near {
                println!(
                    "  {:.3}  {}::{}",
                    item["score"].as_f64().unwrap_or(0.0),
                    item["relpath"].as_str().unwrap_or(""),
                    item["name"].as_str().unwrap_or("")
                );
            }
        }
    }
}

pub fn print_history(data: &Value) {
    println!("Git history");
    println!();
    let summary = data.get("summary").unwrap_or(data);
    kv("enabled", &summary["enabled"]);
    if summary.get("commits").is_some() {
        kv("commits", &summary["commits"]);
    }
    if let Some(reason) = summary["reason"].as_str() {
        println!("  {:<18} {reason}", "reason");
    }
}

pub fn print_list(items: &[Value]) {
    if items.is_empty() {
        println!("(no rows)");
        return;
    }
    if items[0].get("why").is_some() && items[0].get("name").is_some() {
        println!("Reading order");
        for (i, item) in items.iter().enumerate() {
            println!(
                "  {:>2}. {}::{}  —  {}",
                i + 1,
                item["relpath"].as_str().unwrap_or(""),
                item["name"].as_str().unwrap_or(""),
                item["why"].as_str().unwrap_or("")
            );
        }
        return;
    }
    if items[0].get("kind").is_some() && items[0].get("detail").is_some() {
        println!("Flags ({})", items.len());
        for item in items {
            let loc = match (item["relpath"].as_str(), item["name"].as_str()) {
                (Some(p), Some(n)) if !n.is_empty() => format!("{p}::{n}"),
                (Some(p), _) => p.to_string(),
                _ => String::new(),
            };
            println!(
                "  {:<22} {}  {}",
                item["kind"].as_str().unwrap_or(""),
                loc,
                item["detail"].as_str().unwrap_or("")
            );
        }
        return;
    }
    for item in items {
        if let (Some(p), Some(n)) = (item["relpath"].as_str(), item["name"].as_str()) {
            println!("  {p}::{n}");
        } else {
            println!("  {item}");
        }
    }
}

fn kv(label: &str, value: &Value) {
    if value.is_null() {
        return;
    }
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    };
    println!("  {label:<18} {text}");
}
