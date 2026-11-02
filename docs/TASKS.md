# console-browser · 任务清单

## M1 无头内核（当前）

| # | 任务 | 状态 | 验收 |
|---|---|---|---|
| T1 | 工作区骨架 + 依赖落地验证（wreq 指纹） | 进行中 | `cargo build` 全绿；ADR 写出 TLS 方案结论 |
| T2 | cb-store：schema + 迁移 + 仓储 API | 待办 | 建库/写读往返/重复 migrate 幂等 |
| T3 | cb-net：客户端 + 指纹 profile + 录制/回放 | 待办 | 回放返回录制状态/头/体 |
| T4 | cb-dom + cb-extract → markdown | 待办 | 3 组 fixture golden 快照 |
| T5 | fetch 管道 + `cb fetch --json` | 待办 | JSON 契约 + 退出码 + 二次读走缓存 |
| T6 | ToolSpec 注册表 + `cb selftest` | 待办 | 8 项 check；契约文档 |
| T7 | cb-mcp stdio（4 工具） | 待办 | 握手 + list_tools + browse_url 对打通过 |
| T8 | 测试台收口 + CI + README | 待办 | fmt/clippy/test 全绿，e2e 脚本可用 |

## M2 搜索与 AI
搜索器（ddg/bing/gh/arxiv）、cb-ai（provider/ledger/预算）、challenge LLM 升级、agent 最小循环、export。

## M3 JS 与会话
QuickJS + DOM/BOM 绑定、fetch 桥、session/daemon、句柄 @eN 与 delta。

## M4 界面
TUI（科幻风、可配键位、命令面板）、webui（axum + SSE，单文件前端）。

## M5 插件
plugin.json + QuickJS 模块、权限门控、站点适配器/挑战求解 hook。

## M6 打磨
历史/缓存 UI、导出增强、文档、性能守门。
