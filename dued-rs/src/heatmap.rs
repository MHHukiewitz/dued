use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};

pub fn write_heatmap(conn: &Connection, dest: &Path, slice_files: Option<&[String]>) -> Value {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT f.relpath, f.loc, f.hotspot, f.pagerank, f.profile_total, COALESCE(SUM(s.cognitive), 0)
        FROM files f LEFT JOIN symbols s ON s.file_id = f.id
        GROUP BY f.id ORDER BY f.relpath
        "#,
        )
        .unwrap();
    let mut rows: Vec<(String, i64, f64, f64, f64, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .unwrap()
        .flatten()
        .collect();
    if let Some(slice) = slice_files {
        rows.retain(|r| slice.contains(&r.0));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok();
    }
    let matrix: Value = rows
        .iter()
        .map(|r| {
            json!({
                "relpath": r.0, "loc": r.1, "hotspot": r.2, "pagerank": r.3,
                "cognitive": r.5, "profile_total": r.4
            })
        })
        .collect();
    fs::write(dest.with_extension("json"), serde_json::to_string_pretty(&matrix).unwrap()).ok();
    if rows.is_empty() {
        fs::write(dest, r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="400"></svg>"#).ok();
        return json!({"files": 0});
    }
    let sizes: Vec<f64> = rows.iter().map(|r| (r.1 as f64).max(1.0)).collect();
    let scores: Vec<f64> = rows.iter().map(|r| r.2 + r.5 + 10.0 * r.4).collect();
    let max_score = scores.iter().cloned().fold(1.0, f64::max);
    let rects = squarify(&sizes, 0.0, 0.0, 960.0, 640.0);
    let mut parts = vec![
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="640" viewBox="0 0 960 640">"#.to_string(),
        "<style>text{font-family:sans-serif;font-size:11px;fill:#111}</style>".to_string(),
    ];
    for ((row, rect), score) in rows.iter().zip(rects).zip(scores) {
        let (x, y, w, h) = rect;
        if w < 2.0 || h < 2.0 {
            continue;
        }
        let label = Path::new(&row.0).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let fill = color(score, max_score);
        parts.push(format!(
            "<g><rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{fill}\" stroke=\"#222\" stroke-width=\"0.6\"/>"
        ));
        if w > 48.0 && h > 16.0 {
            parts.push(format!(r#"<text x="{:.1}" y="{:.1}">{label}</text>"#, x + 4.0, y + 14.0));
        }
        parts.push("</g>".into());
    }
    parts.push("</svg>".into());
    fs::write(dest, parts.join("\n")).ok();
    let html = dest.with_extension("html");
    fs::write(
        &html,
        format!(
            "<!doctype html><html><body style='background:#111;color:#eee;font-family:sans-serif'><h1>dued heatmap</h1><img src='{}' style='max-width:100%'/></body></html>",
            dest.file_name().unwrap().to_string_lossy()
        ),
    )
    .ok();
    json!({"files": rows.len(), "svg": dest.display().to_string(), "html": html.display().to_string()})
}

fn squarify(sizes: &[f64], x: f64, y: f64, w: f64, h: f64) -> Vec<(f64, f64, f64, f64)> {
    let mut rects = Vec::new();
    let mut cx = x;
    let mut cy = y;
    let mut cw = w;
    let mut ch = h;
    let mut remaining = sizes.to_vec();
    while !remaining.is_empty() {
        let target = remaining[0] / remaining.iter().sum::<f64>().max(1.0);
        if cw >= ch {
            let rw = cw * target;
            rects.push((cx, cy, rw, ch));
            cx += rw;
            cw -= rw;
        } else {
            let rh = ch * target;
            rects.push((cx, cy, cw, rh));
            cy += rh;
            ch -= rh;
        }
        remaining.remove(0);
    }
    rects
}

fn color(score: f64, max_score: f64) -> String {
    if max_score <= 0.0 {
        return "#4a6fa5".into();
    }
    let t = (score / max_score).clamp(0.0, 1.0);
    let r = (40.0 + 200.0 * t) as u8;
    let g = (90.0 + 40.0 * (1.0 - t)) as u8;
    let b = (80.0 + 20.0 * (1.0 - t)) as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}
