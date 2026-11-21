---
import:
  regex: "std:regex"
---

# @exec regex

## main

**`星号 = regex.匹配("a*b", "aaab")`**

**`加号 = regex.匹配("a+b", "b")`**

**`锚点 = regex.匹配("^a.*z$", "abcz")`**

**`类 = regex.匹配("[a-c]+", "abc")`**

**`否定类 = regex.匹配("[^0-9]", "a")`**

**`任意 = regex.匹配("a.a", "aba")`**

**`可选 = regex.匹配("a?b", "b")`**

**`转义 = regex.匹配("\\.", ".")`**

**`搜索 = regex.搜索("b+c", "xxbbcyy")`**

**`无效 = regex.匹配("[a", "a")`**

**`不支持 = regex.匹配("(a)", "a")`**

**扩展 = regex.扩展匹配("^(a+)(b|c)$", "aab")**

**捕获 = regex.捕获("^(a+)(b|c)$", "aab")**

`return [星号.value, 加号.value, 锚点.value, 类.value, 否定类.value, 任意.value, 可选.value, 转义.value, 搜索.value, 无效.ok, 无效.code, 不支持.code, 扩展.value, 捕获.value.matched, 捕获.value.groups]`
