# console-browser · 设计规格（精简版）

完整设计与实施计划见会话记录中的已批准计划；此文件保留可审计的决策要点。

## 目标

一条命令把任意网页变成 AI 可直接阅读的干净 markdown；Hermes（外部 Python agent）通过 MCP 调用；同一内核长出 TUI / webui / 插件 / AI 自动化。

## 成功判据

- S1 提取：fixture 覆盖学校/科研/GitHub/SERP/社媒/JS 分享页，正文召回 ≥95%（golden 快照）。
- S2 反爬：TLS 指纹经 oracle 校验；挑战率 ≤10%，全部入 `challenge_events`。
- S3 MCP：Hermes 调 `browse_url` 拿 md，冷请求 <3s，缓存命中 <100ms。
- S4 省 token：同页二次读 0 次 LLM 调用；每次模型调用入账 `llm_calls`。
- S5 自动化：`cargo test` 全绿；`cb selftest --json` 全 ok；CI 离线（cassette 回放）。

## 非目标

不嵌浏览器内核、不做像素级 CSS、不做截图、不做截图 OCR。

## 架构

CLI / MCP / TUI / Web → cb-core（ToolSpec 契约）→ cb-session → cb-net → cb-dom → cb-js → cb-extract → cb-store；旁路 cb-ai / cb-plugin。

管道单向：Request → CacheLookup → NetFetch(profile) → ChallengeCheck → Decode → DomBuild → JsRun → Extract → Serialize → Store → Output。

## 契约

- 退出码：0 ok / 1 通用 / 2 用法 / 3 网络 / 4 挑战 / 5 JS / 6 未找到 / 7 超时 / 8 需鉴权 / 9 预算。
- JSON 信封：`{ok, meta{elapsed_ms,cached,profile,engine}, data|error}`；stdout 只放 JSON，日志走 stderr。
- M1 MCP 工具：browse_url / search_web / history_search / selftest_status。

## 默认决定

1. 默认遵守 robots.txt（`--ignore-robots` 覆盖），每域 1 rps。
2. 不做自动登录。
3. 默认不开 AI，显式 `--ai` 才调模型；每次调用必须记账。
4. DB 在 `%LOCALAPPDATA%\console-browser\cb.db`；缓存 7d / 历史 90d / 日志 14d。
5. markdown 为第一等公民；单文件 HTML 为备份格式。
6. Windows 优先，路径/IPC 抽象跨平台。
7. M1 不做 daemon（无 JS 状态需求）。

## 里程碑

M1 无头内核 → M2 搜索与 AI → M3 JS 与会话 → M4 TUI/webui → M5 插件 → M6 打磨。
