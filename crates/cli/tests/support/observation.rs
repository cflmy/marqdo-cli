//! Helpers for black-box tests that inspect values or committed program bytes.

use serde_json::Value;

pub fn terminal(snapshot: &Value) -> &Value {
    assert_eq!(snapshot["schema"], "marqdo.observation");
    let event = snapshot["events"].as_array().unwrap().last().unwrap();
    assert!(matches!(
        event["kind"].as_str(),
        Some("run.completed" | "run.failed" | "run.rejected")
    ));
    &event["payload"]
}

pub fn text(snapshot: &Value, channel: &str) -> String {
    let bytes = snapshot["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == channel)
        .flat_map(|event| {
            let payload = &event["payload"];
            let bytes = base64_decode(payload["data_base64"].as_str().unwrap());
            assert_eq!(payload["byte_count"], bytes.len());
            bytes
        })
        .collect();
    String::from_utf8(bytes).unwrap()
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let bytes: Vec<u8> = text
        .bytes()
        .filter(|byte| *byte != b'=')
        .map(|byte| {
            ALPHABET
                .iter()
                .position(|item| *item == byte)
                .unwrap_or_else(|| panic!("invalid base64 byte {byte}")) as u8
        })
        .collect();
    for chunk in bytes.chunks(4) {
        let mut triple = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            triple |= u32::from(*byte) << (18 - 6 * index);
        }
        out.push((triple >> 16) as u8);
        if chunk.len() > 2 {
            out.push((triple >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(triple as u8);
        }
    }
    out
}
