---
import:
  process: "std:process"
---

# @exec process

## main

**结果 = process.运行("/bin/echo", ["hello"])**

`return [结果.ok, 结果.value.status, 结果.value.stdout, 结果.value.stderr]`
