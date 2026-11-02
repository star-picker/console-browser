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
- [x] challenge 探测与策略阶梯：403 → 换指纹重试（最多 3 轮）+ `challenge_events` 记账 + `cb challenge stats`（403 → 换指纹 3 轮 → 可选 `--ai-escalate` 问模型要下一步；sampling 回调留 M3）
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
## M3-UI 全屏 TUI（已批准计划，进行中）

- [x] T0 依赖与骨架：cb-render / cb-tui / cb-assist + ratatui/crossterm/unicode-width/tokio-tungstenite
- [x] 版本号 xx.yy.zz.YYYYMMDD.githash（build.rs 注入，含形状测试）
- [x] T1 cb-extract 结构化 RenderDoc/Block/Span（markdown 链路原样未动，7 golden + 6 新测试全绿）
- [x] T2 cb-core：FetchOutcome.doc、login::detect_login_wall、needs_login（5 单测 + 2 e2e）
- [x] T3 cookie jar + store v3（tabs/bookmarks/cookies + SecretBox；DPAPI 留 T12）
- [x] T4 cb-render 布局引擎（折行/CJK/表格/代码/链接索引，8 测试）
- [x] T5 cb-tui 状态机与键位（mode/action/keymap/editbuffer/app/route，24 测试）
- [x] T6 全屏屏幕与视图（alternate screen + RAII 恢复 + 整屏重绘 + TestBackend 快照 + 驱动循环 + cb tui 接线）
- [x] T7 加载/错误页/需登录页 + Esc 取消（加载代数作废；错误页/需登录页有测试）
- [x] T8 链接焦点环 / 查找 / 链接列表浮层（从文档链接填充）
- [ ] T9 标签页与会话恢复
- [x] T10 浮层：帮助/历史/命令面板/`: ` 命令行(spawn 自身)/AI/日志环
- [ ] T11 人工接力（cb-assist + CDP + cb login/cookies）
- [ ] T12 鼠标与打磨
- [ ] T13 接线与文档