
# @exec env
## 读取

`params(名称)`

读取一个显式授权的环境变量。名称是精确匹配，不支持 glob；未授权是
permission.denied，未设置是 env.not_found，非 UTF-8 是
env.not_unicode。

`return resource.request("env.get", [名称])`

## 存在

`params(名称)`

**结果 = 读取(名称)**

`if 结果.ok`

`return {ok: true, value: true}`

`else`
`if 结果.code == "env.not_found"`

`return {ok: true, value: false}`

`end`
`end`
`return 结果`
