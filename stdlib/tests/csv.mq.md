---
import:
  csv: "std:data/csv"
---

# @exec csv

## main

**`行 = csv.解析行("甲,\"乙,丙\",\"说\"\"好\"")`**

**`空 = csv.解析行("")`**

**`坏 = csv.解析行("\"a")`**

**`框 = csv.解析行("a,\"b\"")`**

**`多行 = csv.解析("甲,\"乙,丙\"\n\"说\"\"好\",2")`**

**格式化 = csv.格式化(多行.value)**

`return [行.value, csv.格式化行(行.value), 空.value, 坏.code, 框.value, 多行.ok, 多行.value, 格式化]`
