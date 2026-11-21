---
import:
  env: "std:env"
  process: "std:process"
---

# @exec process

## main

**程序 = env.读取("MARQDO_TEST_PROCESS_PROGRAM")**

**结果 = process.运行(程序.value, ["--version"])**

`return [结果.ok, 结果.value.status, 结果.value.stdout, 结果.value.stderr]`
