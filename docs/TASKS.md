# console-browser · 任务清单

做完打钩。分类看：建基 / 内核 / 接口 / 界面 / 质量。

## 建基（Foundation）

- [x] T1 工作区骨架：workspace + 7 crate + 依赖落地（wreq 0.15.3 指纹栈可编译）
- [x] NASM 工具链就位（BoringSSL 汇编编译所需）
- [x] 本地 git 库 + 台账策略（`docs/PROGRESS.md`）
- [x] 设计规格 + ADR 0001（TLS 指纹选型）
- [x] `.gitattributes` 统一换行，消掉 CRLF 噪音

## 内核（Kernel）

- [x] T2 cb-store：schema + 迁移 + 页面/缓存/历史/日志仓储（4 测试绿）
- [x] T3 cb-net：指纹 profile + 回放/录制 + 缓存键归一化（2 测试绿）
- [x] T4 cb-dom：解析 + CSS 选择 + 正文容器评分
- [x] T4 cb-extract：正文 → markdown（标题/列表/表格/代码/引用/图片/链接）+ token 预算（5 测试绿）
- [x] T5 fetch 管道：缓存 → 网络 → DOM → 提取 → 存库 → 历史 → 日志

## 接口（Interfaces）

- [x] T5 `cb fetch <url> --to md --json`：JSON 信封 + 退出码
- [x] T6 ToolSpec 注册表（CLI / MCP / agent 共用一份 schema）
- [x] T6 `cb selftest --json`：8 项机器可读检查
- [x] T7 cb-mcp stdio：browse_url / history_search / selftest_status（search_web 延到 M2）
- [x] T7 Hermes 接入文档 `docs/hermes-setup.md`

## 质量（Quality）

- [x] T8 测试台：cassette 回放 + e2e 脚本 + CI（离线）
- [x] T8 README：安装、Hermes 配置、三个 `cb fetch` 例子
- [x] T8 `cargo fmt --check` / `clippy -D warnings` 全绿

## M2 搜索与 AI

- [x] 搜索引擎适配（ddg / bing / gh / arxiv）+ `cb search` + MCP `search_web`（9 测试）
- [x] cb-ai：provider（deepseek / openai-compatible / ollama）+ ledger + 预算闸（`cb ai config/usage/ask`）
- [x] challenge 探测与策略阶梯：403 → 换指纹重试（最多 3 轮）+ `challenge_events` 记账 + `cb challenge stats`（LLM/sampling 升级留 M3）
- [x] agent 最小循环：`cb agent run`（模型出 JSON 动作 → 搜索/抓取 → 观察 → 答；步数/token 上限）
- [x] `cb export --to html|md`（单文件 HTML：剥脚本/外链样式/内联事件，注入阅读样式；7 个测试）

## M3 JS 与会话

- [ ] cb-js：QuickJS + DOM/BOM 绑定子集 + fetch 桥回 cb-net
- [ ] cb-session + daemon（命名管道、句柄 @eN、delta 快照）
- [ ] JS 站验收：deepseek 分享页 / GitHub / 一个 SERP

## M4 界面

- [ ] TUI（科幻风、可配键位、命令面板、渲染快照测试）
- [ ] webui（axum + SSE，单文件前端，同状态镜像）

## M5 插件

- [ ] plugin.json + QuickJS 模块 + 权限门控
- [ ] 站点适配器 / 挑战求解 hook 示例

## M6 打磨

- [ ] 历史/缓存 UI、导出增强、性能守门、文档补完