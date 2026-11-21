---
import:
  csv: "std:data/csv"
---

## main

**`行 = csv.解析行("甲,\"乙,丙\",\"说\"\"好\"")`**

**`空 = csv.解析行("")`**

**`坏 = csv.解析行("\"a")`**

**`框 = csv.解析行("a,\"b\"")`**

*[行.value, csv.格式化行(行.value), 空.value, 坏.code, 框.value]*
