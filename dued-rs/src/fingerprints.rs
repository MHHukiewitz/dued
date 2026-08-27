use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};

pub fn fingerprint_symbol(
    name: &str,
    effects: &[String],
    fan_in: i64,
    fan_out: i64,
    cyclomatic: i64,
    cognitive: i64,
    callees: &[String],
) -> String {
    let mut eff = effects.to_vec();
    eff.sort();
    let mut cal = callees.to_vec();
    cal.sort();
    cal.truncate(20);
    let payload = json!({
        "callees": cal,
        "cognitive": cognitive,
        "cyclomatic": cyclomatic,
        "effects": eff,
        "fan_in": fan_in,
        "fan_out": fan_out,
        "name_len": name.len(),
    });
    let pretty = pythonish_dumps(&payload);
    let digest = hex::encode(Sha1::digest(pretty.as_bytes()));
    format!("{}|{pretty}", &digest[..16])
}

fn pythonish_dumps(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(pythonish_dumps).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .into_iter()
                .map(|k| format!("\"{k}\": {}", pythonish_dumps(&map[k])))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
    }
}

pub fn apply_fingerprints(conn: &Connection) {
    let mut stmt = conn
        .prepare("SELECT id, name, effects, fan_in, fan_out, cyclomatic, cognitive FROM symbols")
        .unwrap();
    let rows: Vec<(i64, String, String, i64, i64, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    let mut estmt = conn
        .prepare("SELECT src_symbol_id, dst_name FROM edges WHERE kind = 'call'")
        .unwrap();
    let mut callees_by_src: HashMap<i64, Vec<String>> = HashMap::new();
    for row in estmt
        .query_map([], |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .flatten()
    {
        if let Some(sid) = row.0 {
            callees_by_src.entry(sid).or_default().push(row.1);
        }
    }
    drop(estmt);
    let mut update = conn.prepare("UPDATE symbols SET fingerprint = ? WHERE id = ?").unwrap();
    let mut bar = crate::progress::Bar::new("fingerprints", rows.len());
    for (id, name, effects, fan_in, fan_out, cyc, cog) in rows {
        let tags: Vec<String> = serde_json::from_str(&effects).unwrap_or_default();
        let empty = Vec::new();
        let callees = callees_by_src.get(&id).unwrap_or(&empty);
        let fp = fingerprint_symbol(&name, &tags, fan_in, fan_out, cyc, cog, callees);
        update.execute(params![fp, id]).ok();
        bar.tick(&name);
    }
    bar.finish();
}

pub fn fingerprint_overlap(a: &str, b: &str) -> f64 {
    let Some((_, pa)) = a.split_once('|') else {
        return 0.0;
    };
    let Some((_, pb)) = b.split_once('|') else {
        return 0.0;
    };
    let pa: Value = serde_json::from_str(pa).unwrap_or(Value::Null);
    let pb: Value = serde_json::from_str(pb).unwrap_or(Value::Null);
    let ea = as_set(&pa["effects"]);
    let eb = as_set(&pb["effects"]);
    let ca = as_set(&pa["callees"]);
    let cb = as_set(&pb["callees"]);
    let effect_score = if ea.is_empty() && eb.is_empty() {
        1.0
    } else {
        let union = ea.union(&eb).count() as f64;
        if union == 0.0 {
            0.0
        } else {
            ea.intersection(&eb).count() as f64 / union
        }
    };
    let callee_score = if ca.is_empty() && cb.is_empty() {
        1.0
    } else {
        let union = ca.union(&cb).count() as f64;
        if union == 0.0 {
            0.0
        } else {
            ca.intersection(&cb).count() as f64 / union
        }
    };
    let cyc_a = pa["cyclomatic"].as_i64().unwrap_or(0);
    let cyc_b = pb["cyclomatic"].as_i64().unwrap_or(0);
    let cyc = 1.0 - ((cyc_a - cyc_b).abs() as f64 / 20.0).min(1.0);
    0.4 * effect_score + 0.4 * callee_score + 0.2 * cyc
}

fn as_set(v: &Value) -> std::collections::HashSet<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}
