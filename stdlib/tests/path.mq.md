---
import:
  path: "std:path"
---

# @exec path

## main

`return [path.连接(["a", "", "b", "c"]), path.文件名("a/b/c.txt"), path.目录名("a/b/c.txt"), path.扩展名("a/b/c.txt"), path.规范("./a//b/../c"), path.规范("/a/./b/../c"), path.规范("../a/b"), path.是否绝对("/a"), path.是否绝对("a")]`
