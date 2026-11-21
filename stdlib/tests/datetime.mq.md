---
import:
  datetime: "std:datetime"
---

# @exec datetime

## main

`return [datetime.格式化(0), datetime.格式化(1700000000123), datetime.解析("1970-01-01T00:00:00Z").value, datetime.解析("2026-11-21T08:00:00.123Z").value, datetime.解析("2026-13-01T00:00:00Z").ok, datetime.解析("2026-02-29T00:00:00Z").ok, datetime.格式化(-1)]`
