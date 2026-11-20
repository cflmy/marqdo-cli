//! Independent Tool Protocol v1 consumer: reads a small part of Trace v2.

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

fn number_field(json: &str, key: &str) -> Option<i64> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)? + needle.len();
    let colon = json[start..].find(':')? + start;
    let rest = json[colon + 1..].trim_start();
    let end = rest
        .find(|ch: char| !ch.is_ascii_digit() && ch != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .position(|arg| arg == "--trace")
        .and_then(|index| args.get(index + 1))
        .or_else(|| args.first())
        .unwrap_or_else(|| {
            eprintln!("usage: sample-knowledge --trace <trace-v2.json>");
            std::process::exit(2);
        });
    let json = fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("sample-knowledge: cannot read {path}: {error}");
        std::process::exit(1);
    });
    let schema = string_field(&json, "schema").unwrap_or_default();
    if schema != "marqdo.trace" {
        eprintln!("sample-knowledge: expected marqdo.trace, got {schema:?}");
        std::process::exit(1);
    }
    let version = number_field(&json, "schema_version").unwrap_or_default();
    if version != 2 {
        eprintln!("sample-knowledge: unsupported Trace version {version}");
        std::process::exit(1);
    }
    let status = string_field(&json, "status").unwrap_or_default();
    let events = json.matches("\"seq\"").count();
    println!("sample-knowledge schema={schema} version={version} events={events} status={status}");
}
