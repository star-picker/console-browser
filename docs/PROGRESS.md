# console-browser · 开发进度台账

> 只追加不改写。每条记录：时间 / 任务 / 做了什么 / 证据 / 裁决(Ruling)。

## 规则

- 所有提交进本地 git，提交信息写清用途；发布/推送前再次检查无敏感数据。
- **禁止入仓**：API key、token、cookie、会话凭证、真实用户数据、带凭证的 URL。密钥只从环境变量或本地未跟踪文件读取。
- 每个任务结束：跑测试 → 记台账 → 提交。

## 台账

### 2026-01-01 · T1 工作区骨架

- 建 workspace（7 crate：cb-core/cb-store/cb-net/cb-dom/cb-extract/cb-mcp/cb-cli），`git init`。
- 依赖落地：`wreq 0.15.3`（0.16.3 要求 rustc 1.98，本机 1.96，锁 0.15.3）。
- Ruling: M1 只建 7 个有代码的 crate，其余（cb-js/cb-session/cb-ai/cb-plugin/cb-tui/cb-web）延到对应里程碑 —— 避免空壳噪音 —— 代价：后续里程碑要各自加 crate，属常规操作。
- Ruling: MCP 不用 `rmcp`，手写 JSON-RPC 2.0 over stdio —— rmcp 版本/API 变动风险高且只需 4 个工具 —— 代价：协议细节自己实现，约 200 行。
- Ruling: `docs/PROGRESS.md` 用台账形式记录裁决，保证开发历史可审计。
