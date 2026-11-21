---
import:
  json: "std:data/json"
---

## main

**`对象 = json.解析("{\"a\": 1, \"b\": [true, null, \"说\\\"好\"]}")`**

**`浮点 = json.解析("[1, 2.5, -3e2, 4E-1, {\"a\": 1.0}, \"x\"]")`**

**`失败 = json.解析("[1 2]")`**

**`尾随 = json.解析("1 2")`**

*[对象.ok, 对象.value.a, 对象.value.b[2], json.格式化(浮点.value), 失败.ok, 失败.code, 尾随.code]*
