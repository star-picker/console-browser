# 把 cb 挂到 Hermes 上（MCP）

cb 是一个 stdio MCP 服务端。Hermes 侧只需登记一个命令，不用写代码。

## 命令

```
cb mcp serve
```

## 工具

| 工具 | 用途 |
|---|---|
| `browse_url` | 取网页 → 干净 markdown（可选 html/text/json、max_tokens、scope、fresh） |
| `history_search` | 搜本机浏览历史（URL/标题） |
| `selftest_status` | 八项自检，机器可读 |

## 配置片段（放到 Hermes 的 MCP servers 配置里）

```json
{
  "mcpServers": {
    "console-browser": {
      "command": "cb",
      "args": ["mcp", "serve"]
    }
  }
}
```

Windows 上如果 `cb` 不在 PATH，用绝对路径，例如
`"command": "D:\\tools\\cb.exe"`。

## 离线/复盘模式

`cb mcp serve --replay <cassette.json>`：所有 `browse_url` 从录制文件回放，不联网。
自动化测试与事故复盘用这个。

## 为什么比 curl 强

- 指纹：TLS ClientHello 与 HTTP/2 特征按 Chrome/Firefox/Safari 预设发出，不是 curl 的显式指纹。
- 内容：HTML → 正文提取 → markdown，AI 拿到的是能直接吃的东西，不是标签汤。
- 省钱：同一页第二次读命中本地缓存，零网络、零模型调用。
- 可诊断：失败有稳定退出码与结构化错误（network / challenge / needs_auth / timeout）。
