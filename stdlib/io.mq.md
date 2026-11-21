---
import:
  format: "./format.mq.md"
  stream: "./io/stream.mq.md"
  console: "./io/console.mq.md"
---

# @exec io

## 写文本

`params(文本)`

`return stream.写文本(文本)`

## 写全部

`params(流, 内存, 起点, 长度)`

`return stream.写全部(流, 内存, 起点, 长度)`

## 写值

`params(值)`

按语言值语义格式化后输出，并追加换行。

`return stream.写文本(format.显示(值) + "\n")`

## 读取行

`params()`

`return console.读取行()`

## 打印

`params(值)`

`return console.打印(值)`
