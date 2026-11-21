---
import:
  env: "std:env"
---

## main

**存在 = env.存在("MARQDO_TEST_ENV")**

**读取 = env.读取("MARQDO_TEST_ENV")**

**缺失 = env.读取("MARQDO_MISSING_ENV")**

*[存在.ok, 存在.value, 读取.ok, 读取.value, 缺失.ok, 缺失.code]*
