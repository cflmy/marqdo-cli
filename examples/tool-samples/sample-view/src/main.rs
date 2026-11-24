//! Independent Tool Protocol v1 consumer: reads a small part of Analysis.
//!
//! This is a test fixture, not a hard-coded CLI command. Build it with
//! `rustc --edition 2021`, package it with `tool.json`, install it, and invoke
//! `marqdo sample-view --analysis <file>`.

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
        .position(|arg| arg == "--analysis")
        .and_then(|index| args.get(index + 1))
        .or_else(|| args.first())
        .unwrap_or_else(|| {
            eprintln!("usage: sample-view --analysis <analysis.json>");
            std::process::exit(2);
        });
    let json = fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("sample-view: cannot read {path}: {error}");
        std::process::exit(1);
    });
    let schema = string_field(&json, "schema").unwrap_or_default();
    if schema != "marqdo.analysis" {
        eprintln!("sample-view: expected marqdo.analysis, got {schema:?}");
        std::process::exit(1);
    }
    let version = number_field(&json, "schema_version").unwrap_or_default();
    if version != 3 {
        eprintln!("sample-view: unsupported Analysis version {version}");
        std::process::exit(1);
    }
    let modules = json.matches("\"functions\"").count();
    println!("sample-view schema={schema} version={version} modules={modules}");
}
