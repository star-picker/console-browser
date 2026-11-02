# console-browser (`cb`)

给 AI 和终端用的浏览器：一条命令把网页变成干净的 markdown，可挂成 MCP 服务端，本地 sqlite 存一切。

不依赖任何现有浏览器内核。自己的 HTML 解析、正文提取、TLS/HTTP2 指纹冒充。

## 现在能用什么（M1）

```
cb fetch <url> [--to md|html|text|json] [--max-tokens N] [--scope CSS]
               [--profile chrome|firefox|safari] [--fresh]
               [--replay FILE] [--record FILE] [--out FILE] [--json]
cb history [--query Q] [--limit N]
cb tools            # 能力表（CLI / MCP / agent 共用一份 schema）
cb selftest         # 八项自检，机器可读
cb mcp serve        # MCP stdio 服务端（--replay 可离线回放）
```

三个例子：

```bash
cb fetch https://example.com/a                      # 打印 markdown
cb fetch https://example.com/a --json --max-tokens 800   # 机器可读 + 省 token
cb mcp serve                                       # 给 Hermes / 任意 MCP 客户端用
```

## 退出码（契约）

| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 其他错误 |
| 2 | 用法错误 |
| 3 | 网络错误 |
| 4 | 挑战未过（反爬） |
| 5 | JS 错误 |
| 6 | 未找到 |
| 7 | 超时 |
| 8 | 需要鉴权 |
| 9 | 预算用尽 |

## JSON 信封

```json
{"ok":true,"meta":{"engine":"cb/0.1.0","elapsed_ms":12,"cached":false,"profile":"chrome"},
 "data":{"title":"...","status":200,"content":"# ...","tokens_estimate":42}}
```

失败时 `ok:false` 且 `error:{code,message,retryable,needs_user?}`。stdout 只有这个；日志走 stderr。

## Hermes 接入

见 [docs/hermes-setup.md](docs/hermes-setup.md)。

## 开发

需要 Rust stable + MSVC 构建工具 + NASM（BoringSSL 汇编要用）。

```powershell
cargo build --workspace
cargo test --workspace
scripts/cb-e2e.ps1        # 端到端冒烟，给外部自动化用
```

文档：任务清单 [docs/TASKS.md](docs/TASKS.md) · 进度台账 [docs/PROGRESS.md](docs/PROGRESS.md) · 设计规格 [docs/superpowers/specs/console-browser-design.md](docs/superpowers/specs/console-browser-design.md) · ADR [docs/adr/0001-tls-impersonation.md](docs/adr/0001-tls-impersonation.md)

## 安全与数据

- 仓库内**不含任何密钥**。API key / cookie 只从环境变量或本地未跟踪文件读（`.env` 已忽略）。
- 浏览历史、缓存、日志全部存在本机 sqlite；`cb db prune` 按保留期清理（缓存 7 天 / 历史 90 天 / 日志 14 天）。
- 默认遵守 robots.txt（将来加 `--ignore-robots` 显式覆盖），每域限速 1 rps。
