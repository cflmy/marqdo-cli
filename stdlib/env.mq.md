## 读取

`(名称)`

读取一个显式授权的环境变量。名称是精确匹配，不支持 glob；未授权是
`permission.denied`，未设置是 `env.not_found`，非 UTF-8 是
`env.not_unicode`。

*resource.request("env.get", [名称])*

## 存在

`(名称)`

**结果 = 读取(名称)**

1. `结果.ok`

   *{ok: true, value: true}*

1. `结果.code == "env.not_found"`

   *{ok: true, value: false}*

*结果*
