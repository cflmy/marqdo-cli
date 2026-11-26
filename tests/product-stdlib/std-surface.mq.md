---
import:
  format: "std:format"
  json: "std:data/json"
  text_lib: "std:text"
  utf8: "std:text/utf8"
  path: "std:path"
  datetime: "std:datetime"
  tables: "std:collections/table"
  gfm: "std:data/table"
---

# @exec surface

## main

**内存 = memory.new(1, 4)**

**长度 = text_lib.utf8编码("é", 内存, 0)**

**解码 = utf8.解码(内存, 0, 长度)**

**对象 = json.解析("[1, 2]")**

**数据 = 表(["名称", "分数"], [["甲", 87], ["乙", 92]])**

**排序 = tables.排序(数据, 1)**

`return [长度, 解码.value, json.格式化(对象.value), format.显示(["ok", 3]), path.连接(["a", "b"]), datetime.格式化(1795248000123), gfm.转GFM(排序)]`
