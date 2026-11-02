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

### T2 cb-store · T3 cb-net · T4 cb-dom + cb-extract

- T2：sqlite schema（pages / cache_entries / history / logs / settings + 索引），WAL，`user_version` 迁移，仓储 CRUD。测试 4 绿。
- T3：`wreq 0.15.3` + `wreq_util` 预设（Chrome137 / Firefox139 / Safari18），live + replay 双后端，录制落 cassette，缓存键 = blake3(方法, 归一化 URL, profile, js)。
- T4：`scraper` 解析 + 正文容器评分（文本长度 ×(1−链接密度) + 语义标签加成），markdown 序列化（标题/段落/列表/表格/代码围栏/引用/图片/链接），token 预算裁剪。测试 5 绿（3 组 fixture golden + scope + 预算）。
- Ruling: `cb-dom` 用 `scraper` 而不是手写 `selectors::Element` 实现 —— 少约 200 行胶水，解析仍走 html5ever —— 代价：DOM 不可变，M3 的 JS 可变 DOM 要另建（本来也要另建）。
- Ruling: cassette 的 headers 落成 JSON map（`BTreeMap`），响应里仍是有序 Vec —— 人读友好 —— 代价：丢失重复头与顺序。
- Ruling: 需要 `ego-tree` 直依赖（scraper 0.29 只导出 Node，不导出 NodeRef）。
- Ruling: M1 只建 5 张表（pages/cache_entries/history/logs/settings）；FTS5、page_revisions、llm_calls、challenge_events 延到 M2/M3 —— 当前无人读它们 —— 代价：M2 加迁移。
- 证据：`cargo test -p cb-store`、`-p cb-net`、`-p cb-extract` 全绿。

### 安全事件 · 凭证泄露（2026-01-01）

- 会话中有人贴入一枚 GitHub PAT。该令牌已视为**泄露**，不得写入仓库、remote、配置、日志或脚本。
- 处置：不落盘、不入 git、不使用；请到 GitHub 设置里**吊销并重建**。
- 需要推送时，改用不在会话中出现的凭证（本地 credential manager 或交互式 `gh auth login`），并确认命令回显不含密钥。
- 已扫描：工作区与 git 历史中无 `github_pat_` / `ghp_` 模式命中。
- Ruling: 任何密钥只走环境变量或本地未跟踪文件；`.env` 已在 `.gitignore`。代价：用户要多做一步本地配置。
