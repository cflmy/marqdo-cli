---
import:
  http: "std:net/http"
---

# @exec http

## main

**响应文本 = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"**

**响应 = http.解析响应(响应文本)**

**请求 = http.构建请求("POST", "/echo", [["Host", "example.test"], ["Content-Length", "2"]], "ok")**

**响应字节 = http.解析响应字节(http.文本字节("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n6\r\n你好\r\n0\r\n\r\n"))**

**正文 = http.字节文本(响应字节.value.body)**

**请求字节 = http.构建请求字节("POST", "/", [], [65, 66])**

**请求文本 = http.字节文本(请求字节)**

`return [响应.状态, 响应.原因, 响应.头["Content-Type"], 响应.体, 请求, 响应字节.value.status, 正文.value, 请求文本.value]`
