//! Independent Tool Protocol v1 consumer: binds Analysis v2 and Trace v2.

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
    let mut trace = None;
    let mut index = 0;
    while index + 1 < args.len() {
        match args[index].as_str() {
            "--analysis" => analysis = Some(args[index + 1].clone()),
            "--trace" => trace = Some(args[index + 1].clone()),
            _ => {}
        }
        index += 2;
    }
    let (Some(analysis), Some(trace)) = (analysis, trace) else {
        eprintln!("usage: sample-proof --analysis <analysis-v2.json> --trace <trace-v2.json>");
        std::process::exit(2);
    };
    let analysis = read(&analysis, "analysis");
    let trace = read(&trace, "trace");
    if string_field(&analysis, "schema") != Some("marqdo.analysis") {
        eprintln!("sample-proof: analysis schema mismatch");
        std::process::exit(1);
    }
    if string_field(&trace, "schema") != Some("marqdo.trace") {
        eprintln!("sample-proof: trace schema mismatch");
        std::process::exit(1);
    }
    if string_field(&trace, "status") != Some("ok") {
        eprintln!("sample-proof: trace did not complete successfully");
        std::process::exit(1);
    }
    let digest = string_field(&trace, "analysis_digest").unwrap_or_default();
    if !digest.starts_with("sha256:") {
        eprintln!("sample-proof: trace is not bound to an analysis digest");
        std::process::exit(1);
    }
    println!("sample-proof bound=true digest={digest}");
}
