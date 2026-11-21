## 压缩

`(文本)`

把 UTF-8 文本压缩为 gzip 字节列表（`List<Int>`，0–255）。输入上限
8 MiB；结果确定（不写入时间戳/文件名）。

*resource.request("compress.gzip", [文本])*

## 解压

`(字节列表)`

解压 gzip 字节列表并校验 UTF-8。输出上限 8 MiB；坏 gzip 返回
`compress.invalid`，非 UTF-8 返回 `compress.not_utf8`。

*resource.request("compress.gunzip", [字节列表])*
