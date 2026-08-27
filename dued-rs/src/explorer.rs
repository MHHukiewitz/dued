use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::dead::{dead_files, dead_symbols};
use crate::hollow::hollow_symbols;
use crate::inventory::inventory;
use crate::progress::{note, Bar};
use crate::rank::reading_order;

pub fn write_explorer(repo: &Path, conn: &Connection, dest: &Path, extra: Value) -> Value {
    fs::create_dir_all(dest.join("data")).ok();
    note("export complete index tables for the HTML explorer");
    let mut bar = Bar::new("export", 14);
    let languages = query_langs(conn);
    bar.tick("languages");
    let files = query_files(conn);
    bar.tick("files");
    let symbols = query_symbols(conn);
    bar.tick("symbols");
    let issues = query_issues(conn);
    bar.tick("issues");
    let names = query_names(conn);
    bar.tick("names");
    let clones = query_clones(conn);
    bar.tick("clones");
    let coupling = query_coupling(conn);
    bar.tick("coupling");
    let dead = dead_symbols(conn);
    bar.tick("dead symbols");
    let dead_f = dead_files(conn);
    bar.tick("dead files");
    let hollow = hollow_symbols(conn);
    bar.tick("hollow");
    let order = reading_order(conn, 80);
    bar.tick("reading order");
    let inv = inventory(conn, repo);
    bar.tick("inventory");
    let questions = review_questions(&order, &dead, &issues);
    let stamp = dest
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| *n != "latest")
        .unwrap_or("")
        .to_string();
    let counts = json!({
        "files": files.len(),
        "symbols": symbols.len(),
        "issues": issues.len(),
        "names": names.len(),
        "clones": clones.len(),
        "coupling": coupling.len(),
        "dead_symbols": dead.len(),
        "dead_files": dead_f.len(),
        "hollow": hollow.len(),
    });
    let mut catalog = json!({
        "repo": repo.display().to_string(),
        "generated_at": stamp,
        "engine": "dued",
        "languages": languages,
        "inventory": inv,
        "reading_order": order,
        "files": files,
        "symbols": symbols,
        "issues": issues,
        "names": names,
        "clones": clones,
        "coupling": coupling,
        "dead_symbols": dead,
        "dead_files": dead_f,
        "hollow": hollow,
        "questions": questions,
        "counts": counts,
    });
    if let Value::Object(extra_map) = extra {
        if let Value::Object(obj) = &mut catalog {
            obj.extend(extra_map);
        }
    }
    write_json(dest.join("data/files.json"), &catalog["files"]);
    write_json(dest.join("data/symbols.json"), &catalog["symbols"]);
    write_json(dest.join("data/issues.json"), &catalog["issues"]);
    write_json(dest.join("data/names.json"), &catalog["names"]);
    write_json(dest.join("data/clones.json"), &catalog["clones"]);
    write_json(dest.join("data/dead.json"), &json!({
        "symbols": catalog["dead_symbols"],
        "files": catalog["dead_files"],
        "hollow": catalog["hollow"],
    }));
    write_json(dest.join("data/coupling.json"), &catalog["coupling"]);
    write_json(dest.join("data/reading_order.json"), &catalog["reading_order"]);
    write_json(
        dest.join("data/overview.json"),
        &json!({
            "repo": catalog["repo"],
            "generated_at": catalog["generated_at"],
            "languages": catalog["languages"],
            "inventory": catalog["inventory"],
            "questions": catalog["questions"],
            "counts": catalog["counts"],
        }),
    );
    bar.tick("json");
    let html = render_html(&catalog);
    fs::write(dest.join("report.html"), html).ok();
    bar.tick("html");
    bar.finish();
    note(&format!(
        "explorer {} files, {} symbols, {} issues → {}",
        catalog["counts"]["files"],
        catalog["counts"]["symbols"],
        catalog["counts"]["issues"],
        dest.join("report.html").display()
    ));
    catalog
}

fn write_json(path: std::path::PathBuf, value: &Value) {
    fs::write(path, serde_json::to_string(value).unwrap()).ok();
}

fn query_langs(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare("SELECT language, COUNT(*) AS n, COALESCE(SUM(loc),0) AS loc FROM files GROUP BY language")
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({"language": r.get::<_, String>(0)?, "n": r.get::<_, i64>(1)?, "loc": r.get::<_, i64>(2)?}))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn query_files(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT id, relpath, language, loc, size, is_test, tokens, ast_nodes,
               pagerank, hotspot, churn, authors, bus_factor, profile_total
        FROM files ORDER BY hotspot DESC, loc DESC
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "relpath": r.get::<_, String>(1)?,
            "language": r.get::<_, String>(2)?,
            "loc": r.get::<_, i64>(3)?,
            "size": r.get::<_, i64>(4)?,
            "is_test": r.get::<_, i64>(5)?,
            "tokens": r.get::<_, i64>(6)?,
            "ast_nodes": r.get::<_, i64>(7)?,
            "pagerank": r.get::<_, f64>(8)?,
            "hotspot": r.get::<_, f64>(9)?,
            "churn": r.get::<_, i64>(10)?,
            "authors": r.get::<_, i64>(11)?,
            "bus_factor": r.get::<_, i64>(12)?,
            "profile_total": r.get::<_, f64>(13)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn query_symbols(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.kind, s.start_line, s.end_line, s.signature,
               s.cyclomatic, s.cognitive, s.nesting, s.nargs,
               s.is_public, s.is_entry, s.is_test, s.fan_in, s.fan_out,
               s.effects, s.risks, s.cost_hint, f.relpath, f.language
        FROM symbols s JOIN files f ON f.id = s.file_id
        ORDER BY s.cognitive DESC, s.fan_in DESC, f.relpath, s.start_line
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "name": r.get::<_, String>(1)?,
            "kind": r.get::<_, String>(2)?,
            "start_line": r.get::<_, i64>(3)?,
            "end_line": r.get::<_, i64>(4)?,
            "signature": r.get::<_, String>(5)?,
            "cyclomatic": r.get::<_, i64>(6)?,
            "cognitive": r.get::<_, i64>(7)?,
            "nesting": r.get::<_, i64>(8)?,
            "nargs": r.get::<_, i64>(9)?,
            "is_public": r.get::<_, i64>(10)?,
            "is_entry": r.get::<_, i64>(11)?,
            "is_test": r.get::<_, i64>(12)?,
            "fan_in": r.get::<_, i64>(13)?,
            "fan_out": r.get::<_, i64>(14)?,
            "effects": r.get::<_, String>(15)?,
            "risks": r.get::<_, String>(16)?,
            "cost_hint": r.get::<_, i64>(17)?,
            "relpath": r.get::<_, String>(18)?,
            "language": r.get::<_, String>(19)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn query_issues(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT i.kind, i.detail, i.score, f.relpath, s.name, s.start_line, f.language, f.is_test
        FROM issues i
        LEFT JOIN files f ON f.id = i.file_id
        LEFT JOIN symbols s ON s.id = i.symbol_id
        ORDER BY i.score DESC
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "kind": r.get::<_, String>(0)?,
            "detail": r.get::<_, String>(1)?,
            "score": r.get::<_, f64>(2)?,
            "relpath": r.get::<_, Option<String>>(3)?,
            "name": r.get::<_, Option<String>>(4)?,
            "start_line": r.get::<_, Option<i64>>(5)?,
            "language": r.get::<_, Option<String>>(6)?,
            "is_test": r.get::<_, Option<i64>>(7)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn query_names(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT n.kind, n.detail, n.score, s.name, f.relpath, s.start_line
        FROM name_flags n
        JOIN symbols s ON s.id = n.symbol_id
        JOIN files f ON f.id = s.file_id
        ORDER BY n.score DESC, f.relpath
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "kind": r.get::<_, String>(0)?,
            "detail": r.get::<_, String>(1)?,
            "score": r.get::<_, f64>(2)?,
            "name": r.get::<_, String>(3)?,
            "relpath": r.get::<_, String>(4)?,
            "start_line": r.get::<_, i64>(5)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn query_clones(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT c.score, c.method, a.name, fa.relpath, b.name, fb.relpath
        FROM clones c
        JOIN symbols a ON a.id = c.symbol_a
        JOIN files fa ON fa.id = a.file_id
        JOIN symbols b ON b.id = c.symbol_b
        JOIN files fb ON fb.id = b.file_id
        ORDER BY c.score DESC
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "score": r.get::<_, f64>(0)?,
            "method": r.get::<_, String>(1)?,
            "a": format!("{}::{}", r.get::<_, String>(3)?, r.get::<_, String>(2)?),
            "b": format!("{}::{}", r.get::<_, String>(5)?, r.get::<_, String>(4)?),
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn query_coupling(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare("SELECT file_a, file_b, shared, strength FROM git_coupling ORDER BY strength DESC, shared DESC")
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "file_a": r.get::<_, String>(0)?,
            "file_b": r.get::<_, String>(1)?,
            "shared": r.get::<_, i64>(2)?,
            "strength": r.get::<_, f64>(3)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn review_questions(order: &[Value], dead: &[Value], issues: &[Value]) -> Vec<String> {
    let mut questions = Vec::new();
    for item in order.iter().take(8) {
        questions.push(format!(
            "What side effects does `{}::{}` have, and are they at a boundary?",
            item["relpath"].as_str().unwrap_or(""),
            item["name"].as_str().unwrap_or("")
        ));
    }
    if !dead.is_empty() {
        questions.push("Which listed dead symbols are public API, and which can be removed?".into());
    }
    for item in issues.iter().take(8) {
        questions.push(format!(
            "Is `{}` in `{}` a real refactor target? {}",
            item["kind"].as_str().unwrap_or(""),
            item["relpath"].as_str().unwrap_or(""),
            item["detail"].as_str().unwrap_or("")
        ));
    }
    questions
}

fn render_html(payload: &Value) -> String {
    let json = serde_json::to_string(payload).unwrap().replace('<', "\\u003c");
    format!("{HEAD}{json}{TAIL}")
}

const HEAD: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>dued explorer</title>
<style>
:root { --bg:#0f1419; --panel:#1a222c; --text:#e8eef4; --muted:#8b9bb0; --acc:#7dd3fc; --line:#2a3644; --hi:#1e3a4c; }
* { box-sizing:border-box; }
body { margin:0; font-family: ui-sans-serif, system-ui, sans-serif; background:var(--bg); color:var(--text); display:flex; min-height:100vh; }
nav { width:230px; padding:22px 14px; background:#0b1015; border-right:1px solid var(--line); position:sticky; top:0; height:100vh; overflow:auto; }
nav h1 { font-size:18px; margin:0 0 6px; color:var(--acc); }
nav .sub { color:var(--muted); font-size:12px; margin-bottom:16px; }
nav a { display:flex; justify-content:space-between; color:var(--muted); text-decoration:none; padding:7px 8px; border-radius:6px; }
nav a:hover, nav a.on { color:var(--text); background:var(--hi); }
nav a span { color:var(--acc); font-size:12px; }
main { flex:1; padding:24px 32px 80px; max-width:1200px; }
h2 { margin:0 0 8px; }
.muted { color:var(--muted); }
.cards { display:flex; gap:10px; flex-wrap:wrap; margin:16px 0 24px; }
.card { background:var(--panel); padding:12px 14px; border-radius:8px; min-width:110px; }
.card b { display:block; font-size:20px; }
.card span { color:var(--muted); font-size:12px; }
.toolbar { display:flex; gap:8px; flex-wrap:wrap; margin:12px 0; align-items:center; }
input, select { background:#0b1015; color:var(--text); border:1px solid var(--line); border-radius:6px; padding:8px 10px; }
input[type=search] { min-width:280px; flex:1; }
table { width:100%; border-collapse:collapse; font-size:13px; }
th, td { text-align:left; padding:7px 8px; border-bottom:1px solid var(--line); vertical-align:top; }
th { color:var(--muted); cursor:pointer; user-select:none; }
th:hover { color:var(--text); }
tr:hover td { background:var(--hi); }
code { font-family: ui-monospace, SFMono-Regular, monospace; font-size:12px; }
.detail { background:var(--panel); padding:12px 14px; border-radius:8px; margin-top:14px; white-space:pre-wrap; font-family: ui-monospace, monospace; font-size:12px; }
img { max-width:100%; background:#111; border-radius:8px; }
pre.cmd { background:var(--panel); padding:12px 16px; border-radius:8px; overflow:auto; }
.section { display:none; }
.section.on { display:block; }
.more { margin-top:10px; color:var(--acc); cursor:pointer; }
.tabs { display:flex; flex-wrap:wrap; gap:6px; margin:10px 0 14px; }
.tab { background:#0b1015; color:var(--muted); border:1px solid var(--line); border-radius:999px; padding:6px 10px; cursor:pointer; font:inherit; }
.tab span { color:var(--acc); margin-left:4px; }
.tab:hover, .tab.on { color:var(--text); background:var(--hi); border-color:var(--acc); }
</style>
</head>
<body>
<nav>
  <h1>dued explorer</h1>
  <div class="sub" id="repo-label"></div>
  <a href="#overview" data-sec="overview" class="on">Overview</a>
  <a href="#read" data-sec="read">Reading order</a>
  <a href="#files" data-sec="files">Files <span id="n-files"></span></a>
  <a href="#symbols" data-sec="symbols">Symbols <span id="n-symbols"></span></a>
  <a href="#issues" data-sec="issues">Issues <span id="n-issues"></span></a>
  <a href="#dead" data-sec="dead">Dead code <span id="n-dead"></span></a>
  <a href="#hollow" data-sec="hollow">Hollow <span id="n-hollow"></span></a>
  <a href="#names" data-sec="names">Names <span id="n-names"></span></a>
  <a href="#clones" data-sec="clones">Clones <span id="n-clones"></span></a>
  <a href="#coupling" data-sec="coupling">Git coupling <span id="n-coupling"></span></a>
  <a href="#heatmap" data-sec="heatmap">Heatmap</a>
  <a href="#questions" data-sec="questions">Questions</a>
  <a href="#explore" data-sec="explore">CLI</a>
</nav>
<main>
  <div class="toolbar">
    <input type="search" id="q" placeholder="Search the index (name, path, kind, detail)">
    <select id="lang"><option value="">all languages</option></select>
    <select id="prod">
      <option value="">prod + test</option>
      <option value="prod">production only</option>
      <option value="test">tests only</option>
    </select>
  </div>
  <section id="overview" class="section on"></section>
  <section id="read" class="section"></section>
  <section id="files" class="section"></section>
  <section id="symbols" class="section"></section>
  <section id="issues" class="section"></section>
  <section id="dead" class="section"></section>
  <section id="hollow" class="section"></section>
  <section id="names" class="section"></section>
  <section id="clones" class="section"></section>
  <section id="coupling" class="section"></section>
  <section id="heatmap" class="section">
    <h2>Heatmap</h2>
    <p class="muted">Larger tiles have more lines. Darker tiles have more hotspot score.</p>
    <img src="heatmap.svg" alt="file heatmap">
  </section>
  <section id="questions" class="section"></section>
  <section id="explore" class="section">
    <h2>Explore from the CLI</h2>
    <p class="muted">These commands read the SQLite index. They do not scan again.</p>
    <pre class="cmd">dued report
dued rank --limit 20
dued issues
dued dead
dued names
dued cluster
dued slice &lt;symbol&gt;
dued history</pre>
    <p class="muted">JSON tables are also in the <code>data/</code> folder next to this file.</p>
  </section>
  <div id="detail" class="detail" hidden></div>
</main>
<script type="application/json" id="dued-data">"##;

const TAIL: &str = r##"</script>
<script>
const D = JSON.parse(document.getElementById("dued-data").textContent);
const PAGE = 200;
const state = { sec: "overview", sort: {}, shown: {}, kind: {} };
const $ = (id) => document.getElementById(id);
$("repo-label").textContent = (D.repo || "") + (D.generated_at ? " · " + D.generated_at : "");
const C = D.counts || {};
$("n-files").textContent = C.files || (D.files||[]).length;
$("n-symbols").textContent = C.symbols || (D.symbols||[]).length;
$("n-issues").textContent = C.issues || (D.issues||[]).length;
$("n-dead").textContent = C.dead_symbols || (D.dead_symbols||[]).length;
$("n-hollow").textContent = C.hollow || (D.hollow||[]).length;
$("n-names").textContent = C.names || (D.names||[]).length;
$("n-clones").textContent = C.clones || (D.clones||[]).length;
$("n-coupling").textContent = C.coupling || (D.coupling||[]).length;
const langs = [...new Set((D.files||[]).map(f => f.language).filter(Boolean))].sort();
langs.forEach(l => { const o=document.createElement("option"); o.value=l; o.textContent=l; $("lang").appendChild(o); });

function esc(s){ return String(s??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;"); }
function match(row, q){
  if(!q) return true;
  q = q.toLowerCase();
  return Object.values(row).some(v => String(v??"").toLowerCase().includes(q));
}
function langOk(row){
  const l = $("lang").value;
  return !l || row.language===l || (row.relpath||"").endsWith({python:".py",rust:".rs",typescript:".ts"}[l]||"");
}
function prodOk(row){
  const p = $("prod").value;
  if(!p) return true;
  const test = row.is_test===1 || row.is_test===true;
  return p==="test" ? test : !test;
}
function rows(list){ return (list||[]).filter(r => match(r, $("q").value) && langOk(r) && prodOk(r)); }
function sortRows(list, key){
  const dir = state.sort[state.sec+"."+key] || 1;
  return list.slice().sort((a,b)=>{
    const av=a[key], bv=b[key];
    if(typeof av==="number" && typeof bv==="number") return (av-bv)*dir;
    return String(av??"").localeCompare(String(bv??""))*dir;
  });
}
function kindCounts(list, field){
  const c = {};
  (list||[]).forEach(r => {
    const k = String(r[field]??"").trim();
    if(!k) return;
    c[k] = (c[k]||0)+1;
  });
  return c;
}
function kindTabs(sec, filtered, field){
  const counts = kindCounts(filtered, field);
  const keys = Object.keys(counts).sort((a,b)=>counts[b]-counts[a] || a.localeCompare(b));
  if(keys.length < 2) return {html:"", list: filtered};
  let sel = state.kind[sec] || "";
  if(sel && !counts[sel]) sel = "";
  state.kind[sec] = sel;
  let h = "<div class='tabs' role='tablist'>";
  h += kindTab(sec, "", "all", filtered.length, !sel);
  keys.forEach(k => { h += kindTab(sec, k, k.replace(/_/g," "), counts[k], sel===k); });
  h += "</div>";
  const shown = sel ? filtered.filter(r => String(r[field]??"").trim()===sel) : filtered;
  return {html:h, list:shown};
}
function kindTab(sec, value, label, n, on){
  return "<button type='button' class='tab"+(on?" on":"")+"' role='tab' aria-selected='"+(on?"true":"false")+"' data-tab-sec='"+sec+"' data-tab='"+encodeURIComponent(value)+"'>"+esc(label)+" <span>"+n+"</span></button>";
}
function table(sec, list, cols, render, kindField){
  let base = rows(list);
  let tabs = "";
  if(kindField){
    const t = kindTabs(sec, base, kindField);
    tabs = t.html;
    base = t.list;
  }
  const key = state.sort.key && state.sort.sec===sec ? state.sort.key : cols[0][0];
  const data = sortRows(base, key);
  const n = state.shown[sec] || PAGE;
  const slice = data.slice(0, n);
  let h = tabs+"<table><thead><tr>"+cols.map(c=>"<th data-k='"+c[0]+"'>"+c[1]+"</th>").join("")+"</tr></thead><tbody>";
  slice.forEach((r,i)=>{ h += "<tr data-i='"+i+"'>"+render(r).map(x=>"<td>"+x+"</td>").join("")+"</tr>"; });
  h += "</tbody></table>";
  if(data.length>slice.length) h += "<div class='more' data-more='"+sec+"'>Show more ("+slice.length+" of "+data.length+")</div>";
  else h += "<p class='muted'>Showing "+data.length+" rows</p>";
  return {html:h, data};
}

function overview(){
  const inv = D.inventory||{};
  const langs = (D.languages||[]).map(l=>l.language+" ("+l.n+" files, "+l.loc+" loc)").join(", ");
  const entries = (inv.entry_points||[]).map(e=>"<code>"+esc(e.relpath)+"::"+esc(e.name)+"</code>").join("<br>");
  $("overview").innerHTML = "<h2>Overview</h2><p class='muted'>Complete export from the SQLite index. Search and sort any table.</p>"
    +"<div class='cards'>"
    +card(C.files||(D.files||[]).length,"files")+card(C.symbols||(D.symbols||[]).length,"symbols")
    +card(C.issues||0,"issues")+card(C.dead_symbols||0,"dead symbols")
    +card(C.clones||0,"clones")+card(C.names||0,"name flags")
    +"</div><p>Languages: "+esc(langs)+"</p><p>Entry points:</p><p>"+(entries||"<span class='muted'>none</span>")+"</p>";
}
function card(n,l){ return "<div class='card'><b>"+n+"</b><span>"+l+"</span></div>"; }
function paint(){
  overview();
  const r = table("read", D.reading_order||[], [["name","symbol"],["why","why"],["cognitive","cognitive"],["fan_in","fan-in"]],
    x=>["<code>"+esc(x.relpath)+"::"+esc(x.name)+"</code>", esc(x.why), x.cognitive, x.fan_in]);
  $("read").innerHTML = "<h2>Reading order</h2><p class='muted'>Start here. Generic names such as new and default are omitted.</p>"+r.html;
  const f = table("files", D.files||[], [["relpath","path"],["language","lang"],["loc","loc"],["hotspot","hotspot"],["churn","churn"],["pagerank","pagerank"],["is_test","test"]],
    x=>["<code>"+esc(x.relpath)+"</code>", esc(x.language), x.loc, Number(x.hotspot||0).toFixed(1), x.churn, Number(x.pagerank||0).toFixed(3), x.is_test], "language");
  $("files").innerHTML = "<h2>Files</h2>"+f.html;
  const s = table("symbols", D.symbols||[], [["name","symbol"],["kind","kind"],["cognitive","cognitive"],["fan_in","fan-in"],["fan_out","fan-out"],["effects","effects"]],
    x=>["<code>"+esc(x.relpath)+":"+x.start_line+" "+esc(x.name)+"</code>", esc(x.kind), x.cognitive, x.fan_in, x.fan_out, esc(x.effects)], "kind");
  $("symbols").innerHTML = "<h2>Symbols</h2><p class='muted'>Bodies stay in SQLite. This table is the complete compact catalog.</p>"+s.html;
  const i = table("issues", D.issues||[], [["kind","kind"],["relpath","where"],["detail","detail"],["score","score"]],
    x=>[esc(x.kind), "<code>"+esc(x.relpath||"")+"::"+esc(x.name||"")+"</code>", esc(x.detail), Number(x.score||0).toFixed(2)], "kind");
  $("issues").innerHTML = "<h2>Issues</h2>"+i.html;
  const d = table("dead", D.dead_symbols||[], [["name","symbol"],["kind","kind"],["signature","signature"]],
    x=>["<code>"+esc(x.relpath)+"::"+esc(x.name)+"</code>", esc(x.kind), "<code>"+esc(x.signature||"")+"</code>"], "kind");
  $("dead").innerHTML = "<h2>Dead code</h2><p class='muted'>"+(D.dead_files||[]).length+" isolated files.</p>"+d.html;
  const h = table("hollow", D.hollow||[], [["name","symbol"],["reason","reason"]],
    x=>["<code>"+esc(x.relpath)+"::"+esc(x.name)+"</code>", esc(x.reason)], "reason");
  $("hollow").innerHTML = "<h2>Hollow stubs</h2>"+h.html;
  const n = table("names", D.names||[], [["kind","kind"],["name","symbol"],["detail","detail"],["score","score"]],
    x=>[esc(x.kind), "<code>"+esc(x.relpath)+"::"+esc(x.name)+"</code>", esc(x.detail), Number(x.score||0).toFixed(2)], "kind");
  $("names").innerHTML = "<h2>Name health</h2>"+n.html;
  const c = table("clones", D.clones||[], [["score","score"],["method","method"],["a","a"],["b","b"]],
    x=>[Number(x.score||0).toFixed(2), esc(x.method), "<code>"+esc(x.a)+"</code>", "<code>"+esc(x.b)+"</code>"], "method");
  $("clones").innerHTML = "<h2>Clones</h2>"+c.html;
  const g = table("coupling", D.coupling||[], [["strength","strength"],["shared","shared"],["file_a","file a"],["file_b","file b"]],
    x=>[Number(x.strength||0).toFixed(2), x.shared, "<code>"+esc(x.file_a)+"</code>", "<code>"+esc(x.file_b)+"</code>"]);
  $("coupling").innerHTML = "<h2>Git coupling</h2>"+g.html;
  $("questions").innerHTML = "<h2>Review questions</h2><ol>"+(D.questions||[]).map(q=>"<li>"+esc(q)+"</li>").join("")+"</ol>";
  state.cache = {read:r.data, files:f.data, symbols:s.data, issues:i.data, dead:d.data, hollow:h.data, names:n.data, clones:c.data, coupling:g.data};
}
function show(sec){
  state.sec = sec;
  document.querySelectorAll("nav a").forEach(a=>a.classList.toggle("on", a.dataset.sec===sec));
  document.querySelectorAll("main .section").forEach(s=>s.classList.toggle("on", s.id===sec));
}
document.querySelectorAll("nav a[data-sec]").forEach(a=>a.addEventListener("click", e=>{ e.preventDefault(); show(a.dataset.sec); }));
$("q").addEventListener("input", paint);
$("lang").addEventListener("change", paint);
$("prod").addEventListener("change", paint);
document.body.addEventListener("click", e=>{
  const tab = e.target.closest("[data-tab-sec]");
  if(tab){
    state.kind[tab.dataset.tabSec] = decodeURIComponent(tab.dataset.tab || "");
    state.shown[tab.dataset.tabSec] = PAGE;
    paint(); return;
  }
  const th = e.target.closest("th[data-k]");
  if(th){
    const k = th.dataset.k;
    const id = state.sec+"."+k;
    state.sort[id] = (state.sort[id]||1)*-1;
    state.sort.key = k; state.sort.sec = state.sec;
    paint(); return;
  }
  const more = e.target.closest("[data-more]");
  if(more){ state.shown[more.dataset.more] = (state.shown[more.dataset.more]||PAGE)+PAGE; paint(); return; }
  const tr = e.target.closest("tbody tr");
  if(!tr) return;
  const pack = state.cache && state.cache[state.sec];
  const row = pack && pack[Number(tr.dataset.i)];
  if(!row) return;
  $("detail").hidden = false;
  $("detail").textContent = JSON.stringify(row, null, 2);
});
paint();
show((location.hash||"#overview").slice(1) || "overview");
</script>
</body>
</html>
"##;
