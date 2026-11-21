//! C10/P5: a `.mq.md` HTTP client over the TCP resource endpoint.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("marqdo-http-tcp-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mqmd_http_client_parses_a_response_from_a_tcp_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 512];
        let count = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..count]).starts_with("GET / HTTP/1.1\r\n"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello",
            )
            .unwrap();
    });

    let dir = TempDir::new("client");
    let program = format!(
        r#"---
import:
  http: "std:net/http"
  编码: "std:text"
---

## @exec main

`流 = resource.open("tcp", "{address}", ["read", "write"])`

`请求 = http.构建请求("GET", "/", [["Host", "example.test"]], "")`

`请求内存 = memory.new(1, 1)`

`请求长度 = 编码.utf8编码(请求, 请求内存, 0)`

`写入 = resource.write(流, 请求内存, 0, 请求长度)`

`接收 = memory.new(1, 1)`

`读出 = resource.read(流, 接收, 0, 4096)`

`数量 = 读出.value.count`

`响应文本 = ""`

`下标 = 0`

`while 下标 < 数量`

`响应文本 = 响应文本 + text.from_codepoint(接收[下标])`

`下标 = 下标 + 1`

`end`

`关闭 = resource.close(流)`

`响应 = http.解析响应(响应文本)`

`return [写入.ok, 读出.ok, 响应.状态, 响应.头["Content-Type"], 响应.体, 关闭.ok]`
"#
    );
    std::fs::write(dir.path().join("index.mq.md"), program).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["run", "index.mq.md", "--allow-net", "127.0.0.1", "--json"])
        .current_dir(dir.path())
        .env(
            "MARQDO_STDLIB_DIR",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("stdlib"),
        )
        .output()
        .expect("run marqdo");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["success"], true);
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], true);
    assert_eq!(items[1]["value"], true);
    assert_eq!(items[2]["decimal"], "200");
    assert_eq!(items[3]["value"], "text/plain");
    assert_eq!(items[4]["value"], "hello");
    assert_eq!(items[5]["value"], true);
    server.join().unwrap();
}
