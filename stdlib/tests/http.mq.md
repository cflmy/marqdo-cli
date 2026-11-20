---
import:
  http: "std:net/http"
---

## main

**响应文本 = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"**

**响应 = http.解析响应(响应文本)**

**请求 = http.构建请求("POST", "/echo", [["Host", "example.test"], ["Content-Length", "2"]], "ok")**

*[响应.状态, 响应.原因, 响应.头["Content-Type"], 响应.体, 请求]*
