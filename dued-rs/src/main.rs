use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dued_rs::clones::{find_clones, find_embed_clones, label_clusters};
use dued_rs::dead::dead_report;
use dued_rs::display::{print_analyze, print_brief, print_value};
use dued_rs::embed::{export_label_csv, similar_to, DEFAULT_MODEL};
use dued_rs::git_hist::{analyze_history, history_report};
use dued_rs::heatmap::write_heatmap;
use dued_rs::issues::list_issues;
use dued_rs::names::analyze_names;
use dued_rs::paths::{db_path, report_root};
use dued_rs::profile::{ingest_profile, launch_or_attach};
use dued_rs::progress::{banner, note, set_quiet, stage};
use dued_rs::rank::{compute_rank, reading_order};
use dued_rs::reports::{refresh_report, write_report_dir};
use dued_rs::review::review_pack;
use dued_rs::scan::run_scan;
use dued_rs::slice::slice_symbol;
use dued_rs::store::connect;
use dued_rs::VERSION;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(
    name = "dued",
    version = VERSION,
    about = "Local due-diligence for Python, TypeScript, and Rust repos",
    long_about = "dued walks a repository, builds a local SQLite index, and ranks what to read first.\n\
It does not send source code to a third-party analysis API. Embeddings run on this machine.\n\
The published CLI is pip install dued.\n\n\
Typical first pass:\n  dued analyze\n  open dued-reports/latest/report.html\n  dued report\n\n\
After analyze, rebuild the HTML explorer or query the index without scanning again:\n  dued report | rank | issues | dead | names | cluster | slice <symbol>",
    after_help = "Examples:\n  dued analyze\n  dued analyze --no-git --no-embed\n  dued report\n  dued slice get_user\n  dued --repo /path/to/repo rank --limit 20"
)]
struct Cli {
    #[arg(long, global = true, help = "Hide progress on stderr")]
    quiet: bool,
    #[arg(long = "json", global = true, help = "Write JSON to stdout instead of text")]
    as_json: bool,
    #[arg(long, global = true, help = "Repository root. Default: current directory")]
    repo: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the dued version.
    Version,
    /// Walk, parse, rank, and write the SQLite index. Does not write the full report pack.
    Scan {
        #[arg(long, help = "Stop after this many source files")]
        max_files: Option<usize>,
        #[arg(long, help = "Stop parsing after this many seconds")]
        budget_seconds: Option<f64>,
        #[arg(long, default_value = DEFAULT_MODEL, help = "Embed model name, or stub")]
        model: String,
        #[arg(long, help = "Overlay git churn and coupling")]
        git: bool,
        #[arg(long, help = "Skip embeddings")]
        no_embed: bool,
    },
    /// Print the reading order from the current index.
    Rank {
        #[arg(long, default_value_t = 15)]
        limit: i64,
    },
    /// Show the behavior slice, effects, and blast radius for a symbol.
    Slice {
        #[arg(help = "Symbol name, or path::name")]
        symbol: String,
        #[arg(long, default_value_t = 4)]
        depth: i64,
    },
    /// List unused symbols, isolated files, and hollow stubs.
    Dead,
    /// List god functions, I/O in core, and shotgun-surgery flags.
    Issues,
    /// Report symbol name-health flags.
    Names,
    /// Token clones and optional similar-to query.
    Cluster {
        #[arg(long = "similar-to", help = "Symbol name or path::name")]
        similar: Option<String>,
    },
    /// Git churn, coupling, and bus factor. Refines rank.
    History,
    /// Write SVG and HTML treemap heatmaps into the latest report folder.
    Heatmap,
    /// Overlay an existing speedscope or CPU profile on the index.
    IngestProfile {
        profile: PathBuf,
    },
    /// Launch or attach a profiler, then ingest the result.
    Profile {
        #[arg(long, help = "python | ts | rust")]
        lang: String,
        #[arg(long)]
        pid: Option<i32>,
        #[arg(long, default_value_t = 15)]
        duration: i32,
        command: Vec<String>,
    },
    /// Full due-diligence pack: scan, reports, HTML, review brief.
    Analyze {
        #[arg(long, help = "Stop after this many source files")]
        max_files: Option<usize>,
        #[arg(long, help = "Stop parsing after this many seconds")]
        budget_seconds: Option<f64>,
        #[arg(long, default_value = DEFAULT_MODEL, help = "Embed model name, or stub")]
        model: String,
        #[arg(long, default_value_t = true, hide = true)]
        git: bool,
        #[arg(long = "no-git", help = "Skip git history")]
        no_git: bool,
        #[arg(long, help = "Skip embeddings")]
        no_embed: bool,
    },
    /// Rebuild the HTML explorer from the index and print a short brief.
    Report,
    /// Write a human review pack from the current index.
    Review {
        #[arg(long = "slice")]
        symbol: Option<String>,
    },
    /// Export mismatch flags as a CSV for later human scoring.
    Label {
        #[arg(long = "out")]
        dest: Option<PathBuf>,
    },
    /// Print the SQLite index path.
    IndexPath,
}

fn emit(data: &Value, as_json: bool) {
    if as_json {
        println!("{}", serde_json::to_string_pretty(data).unwrap());
    } else {
        print_value(data);
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let repo = cli.repo.unwrap_or_else(|| std::env::current_dir().unwrap());
    let as_json = cli.as_json;
    set_quiet(cli.quiet || as_json);
    match cli.command {
        Commands::Version => {
            println!("{VERSION}");
        }
        Commands::Scan {
            max_files,
            budget_seconds,
            model,
            git,
            no_embed,
        } => {
            banner(&repo, "scan", !no_embed, git, &model);
            let summary = run_scan(&repo, max_files, budget_seconds, git, !no_embed, &model);
            emit(&summary, as_json);
        }
        Commands::Rank { limit } => {
            let conn = connect(&repo);
            compute_rank(&conn);
            emit(&Value::Array(reading_order(&conn, limit)), as_json);
        }
        Commands::Slice { symbol, depth } => {
            let conn = connect(&repo);
            let data = slice_symbol(&conn, &symbol, depth);
            let dest = report_root(&repo).join("latest");
            std::fs::create_dir_all(&dest).ok();
            if let Some(files) = data["files"].as_array() {
                let names: Vec<String> = files.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                if !names.is_empty() {
                    write_heatmap(&conn, &dest.join("slice-heatmap.svg"), Some(&names));
                }
            }
            emit(&data, as_json);
        }
        Commands::Dead => {
            let conn = connect(&repo);
            emit(&dead_report(&conn), as_json);
        }
        Commands::Issues => {
            let conn = connect(&repo);
            emit(&Value::Array(list_issues(&conn, 40)), as_json);
        }
        Commands::Names => {
            let conn = connect(&repo);
            emit(&Value::Array(analyze_names(&conn)), as_json);
        }
        Commands::Cluster { similar } => {
            let conn = connect(&repo);
            let mut clones = find_clones(&conn);
            clones.extend(find_embed_clones(&conn));
            let clusters = label_clusters(&conn);
            let near = similar.map(|q| similar_to(&conn, &q)).unwrap_or_default();
            emit(&json!({"clones": clones, "clusters": clusters, "similar": near}), as_json);
        }
        Commands::History => {
            let conn = connect(&repo);
            let info = analyze_history(&repo, &conn);
            compute_rank(&conn);
            let mut report = history_report(&conn);
            if let Value::Object(obj) = &mut report {
                obj.insert("summary".into(), info);
            }
            emit(&report, as_json);
        }
        Commands::Heatmap => {
            let dest = report_root(&repo).join("latest");
            std::fs::create_dir_all(&dest).ok();
            let conn = connect(&repo);
            emit(&write_heatmap(&conn, &dest.join("heatmap.svg"), None), as_json);
        }
        Commands::IngestProfile { profile } => {
            let conn = connect(&repo);
            let data = ingest_profile(&conn, &profile);
            compute_rank(&conn);
            emit(&data, as_json);
        }
        Commands::Profile {
            lang,
            pid,
            duration,
            command,
        } => {
            let dest = report_root(&repo).join("latest");
            std::fs::create_dir_all(&dest).ok();
            let out = dest.join("profile.speedscope.json");
            match launch_or_attach(&repo, &lang, pid, &command, &out, duration) {
                Ok(path) => {
                    let conn = connect(&repo);
                    let data = ingest_profile(&conn, &path);
                    compute_rank(&conn);
                    emit(&data, as_json);
                }
                Err(msg) => {
                    eprintln!("{msg}");
                    return ExitCode::FAILURE;
                }
            }
        }
        Commands::Analyze {
            max_files,
            budget_seconds,
            model,
            git,
            no_git,
            no_embed,
        } => {
            let with_git = git && !no_git;
            banner(&repo, "analyze", !no_embed, with_git, &model);
            let summary = run_scan(&repo, max_files, budget_seconds, with_git, !no_embed, &model);
            stage("write reports");
            let conn = connect(&repo);
            let dest = write_report_dir(&repo, &conn, json!({"scan": summary}));
            review_pack(&conn, &dest, None);
            note(&format!("report directory {}", dest.display()));
            let mut out = summary;
            out["report"] = json!(dest.display().to_string());
            if as_json {
                emit(&out, true);
            } else {
                print_analyze(&out);
            }
        }
        Commands::Report => {
            if !db_path(&repo).is_file() {
                eprintln!("no index yet. run: dued analyze");
                eprintln!("expected {}", db_path(&repo).display());
                return ExitCode::FAILURE;
            }
            stage("rebuild HTML explorer");
            let conn = connect(&repo);
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
            if n == 0 {
                eprintln!("empty index. run: dued analyze");
                return ExitCode::FAILURE;
            }
            let dest = refresh_report(&repo, &conn);
            let html = dest.join("report.html");
            let index = dest.join("index.json");
            let text = std::fs::read_to_string(&index).expect("read report index.json");
            let data: Value = serde_json::from_str(&text).expect("parse report index.json");
            if as_json {
                emit(&data, true);
            } else {
                print_brief(&data, Some(&html));
            }
        }
        Commands::Review { symbol } => {
            let dest = report_root(&repo).join("latest");
            std::fs::create_dir_all(&dest).ok();
            let conn = connect(&repo);
            review_pack(&conn, &dest, symbol.as_deref());
            emit(&json!({"report": dest.display().to_string()}), as_json);
        }
        Commands::Label { dest } => {
            let out = dest.unwrap_or_else(|| report_root(&repo).join("latest").join("labels.csv"));
            let conn = connect(&repo);
            let count = export_label_csv(&conn, &out);
            emit(&json!({"rows": count, "path": out.display().to_string()}), as_json);
        }
        Commands::IndexPath => {
            println!("{}", db_path(&repo).display());
        }
    }
    ExitCode::SUCCESS
}
