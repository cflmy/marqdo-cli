//! Independent Tool Protocol v1 consumer: reads an Analysis and Observation source binding.

use std::fs;

fn string_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)? + needle.len();
    let colon = json[start..].find(':')? + start;
    let quote = json[colon..].find('"')? + colon;
    let rest = &json[quote + 1..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn read(path: &str, label: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("sample-proof: cannot read {label} {path}: {error}");
        std::process::exit(1);
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut analysis = None;
    let mut snapshot = None;
    let mut index = 0;
    while index + 1 < args.len() {
        match args[index].as_str() {
            "--analysis" => analysis = Some(args[index + 1].clone()),
            "--observation" => snapshot = Some(args[index + 1].clone()),
            _ => {}
        }
        index += 2;
    }
    let (Some(analysis), Some(snapshot)) = (analysis, snapshot) else {
        eprintln!("usage: sample-proof --analysis <analysis.json> --observation <observation.json>");
        std::process::exit(2);
    };
    let analysis = read(&analysis, "analysis");
    let snapshot = read(&snapshot, "snapshot");
    if string_field(&analysis, "schema") != Some("marqdo.analysis") {
        eprintln!("sample-proof: analysis schema mismatch");
        std::process::exit(1);
    }
    if string_field(&snapshot, "schema") != Some("marqdo.observation") {
        eprintln!("sample-proof: snapshot schema mismatch");
        std::process::exit(1);
    }
    let run = snapshot.rsplit_once("\"run\"").map(|(_, run)| run).unwrap_or_default();
    if string_field(run, "status") != Some("ok") {
        eprintln!("sample-proof: snapshot did not complete successfully");
        std::process::exit(1);
    }
    let digest = string_field(&snapshot, "analysis_digest").unwrap_or_default();
    if !digest.starts_with("sha256:") {
        eprintln!("sample-proof: snapshot is not bound to an analysis digest");
        std::process::exit(1);
    }
    println!("sample-proof claimed_digest={digest}");
}
