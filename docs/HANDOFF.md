# 交接文档（HANDOFF）

> **2026-11-05 更新**：SSH 通道已恢复，本地 19 个提交已全部推送（`e649c46..1cd2717`）。
> 在 HEAD `1cd2717` 上复验：`cargo fmt --check` 通过、`clippy -D warnings` 0 告警、`cargo test --workspace` **274 passed**。

> 给下一个会话的接手人看。写于第三阶段（daemon / AI 流式 / MCP sampling / cb-js 事件循环 + DOM 树），HEAD `7edea72`。

## 一句话状态

第二轮试用修复（9 条 FIX）全部完成；HANDOFF 里的下一步做了 4 项：**cb daemon + 命名管道句柄 `@eN` + delta 快照**、**AI 流式 `cb ai ask --stream`**、**MCP sampling 回调**、**cb-js 事件循环 + 真实 DOM 树（父子/子树选择器/属性/事件）**。仍缺：外链脚本执行策略、布局/CSS/innerHTML/表单细节、插件 JS 钩子、`export --verify`。三门禁全绿（274 测试 / 55 个测试二进制）。

## 怎么验证（每次改动前先跑一遍）

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# 冒烟（需要网络；Hermes 脚本）
cargo build -p cb-cli
cd D:\hermes-tools; python cb_smoke.py
```

## 本轮新增的四个能力

### 1) daemon + 句柄 `@eN` + delta（`d37038c`）

- 新 crate **cb-session**：换行分隔 JSON-RPC 风格协议 `{id, method, params}` / `{id, ok, result|error}`；方法 `ping/open/refresh/get/list/close`。
- `cb daemon [--pipe NAME] [--replay FILE]` 常驻；Windows 命名管道（`\\.\pipe\<name>`），Unix 走 `$TMP/cb-<name>.sock`；默认名 `cb-daemon`，全局 `--pipe` / `CB_PIPE`。
- 页面句柄 `@1/@2/...` 只增不减；`cb session open <url>` 分配句柄，`cb session get @1 --since N` 返回 `full | delta | unchanged`。
- delta 是块级（RenderDoc blocks）：共同前后缀之外，等长替换、长度变化先删后插；`ops` 里是 `{op: replace|insert|remove, index, block}`。
- 会话表在 daemon 内，客户端每次连接一次请求-响应；多连接共享一个 Mutex 串行化。
- 测试：cb-session 7 条（差量/句柄/协议/未命中）+ cb-cli 2 条真起进程走命名管道的端到端。

### 2) AI 流式（`c79e770`）

- cb-net 新增 `NetClient::stream()`：live 用 wreq `bytes_stream`，replay 按行切块（`--replay` 可离线测流）。
- cb-core 新增 `StreamAccumulator`（SSE `data:` 行、跨 chunk 半行、`[DONE]`、usage、非流式整段 JSON 回退）与 `complete_stream(_with)`：边流边回调、预算先拦、成功失败都出账（缺 usage 用 estimate_tokens 估算）。
- CLI：`cb ai ask "..." --stream` 直接打印增量；`--json` 与 `--stream` 互斥（流式输出不是 JSON 信封）。
- 测试：累加器 3 条 + wire 1 条 + CLI 2 条。

### 3) MCP sampling（sampling/createMessage）

- cb-core 增加 `ModelSampler` trait + `Ctx::with_sampler`；`challenge::llm_advice` 在**本地没有模型 key** 时改问调用方。
- cb-mcp 改为：stdin 读线程 / stdout 写线程 / 主循环三件套；采样请求用 `cb-sampling-N` 字符串 id，读线程把响应路由回等待者，不会和主循环抢 stdin。
- `initialize` 时记录 client 是否声明 `capabilities.sampling`；没声明就报 `sampling_unavailable`。
- 新工具 `ask_model {prompt, max_tokens}`：本地 key 优先，其次采样；`browse_url` 新增 `ai_escalate` 参数，挑战时走同一阶梯（403 → 换指纹 → 本地模型/采样建议）。MCP 错误文本现在带 `needs_user.hint`，调用方能看到采样建议。
- 测试：cb-mcp 文本形状 1 条 + handshake 3 条（无能力报错 / 真发 sampling 并回填结果 / 挑战页采样建议）。

### 4) cb-js 事件循环 + 真实 DOM 树（`7edea72`）

- `JsRuntime::run_event_loop(ms)`：先清 Promise 微任务（`execute_pending_job`），再按到期时间跑定时器；中断器兜底，单次预算内最多 10 万回调。`cb js --loop-ms N` 驱动它（默认 0，`--run-inline` 后也会跑）；无 URL 时也装空文档桩。
- DOM 是**真实父子树**：Rust `document_tree` 从 HTML 建属性/直接文本/children（上限 8000 节点），prelude 物化成带 `parentElement/children` 的元素对象；选择器支持逗号组、后代/子代、`#id/.class/tag`、`[attr]/[attr=value]`、`:first-child/:last-child/:nth-child`。
- 元素 API：`textContent/innerText`（递归）、`getAttribute/setAttribute/hasAttribute/removeAttribute`、`classList`、`appendChild/removeChild/insertBefore`、`getElementsByTagName`、`addEventListener/dispatchEvent/click`；`document.title` 的 setter 会回写 `<title>`。
- 边界（诚实）：没有布局/CSS/innerHTML/表单值同步；JS 修改只活在 JS 本地树里，不回写成 RenderDoc；外链 `<script src>` 不抓不执行，所以纯 SPA 壳依旧读不出内容。
- 实测（本机、2026-11-03）：百度首页 `--run-inline` 跑 7 段内联脚本，title 正确；deepseek 首页 0 段内联脚本（逻辑在外链 bundle，读不到）；百度 SERP 返回空壳（被拦）；GitHub 文件树在当前离线网络下三指纹全失败（`meta.attempts` 有证据）。

## 关键约束（别推翻，都是踩过坑后的裁决）

1. **两条渲染链路**：AI 用 markdown（`cb-extract`），TUI/webui 用 RenderDoc（`doc.rs`）。改抽取时两边都过回归。
2. **块级判定用 BLOCK_TAGS**：别退回「直接子元素里有没有非容器标签」——会把门户页压成一行。
3. **JSON 数据转储不进正文**：`looks_like_json_dump` 保护，别删。
4. **搜索 parse_failed 必须失败**：退出码 10，别伪装成 `ok:true`。
5. **人工接力只在真登录墙触发**：验证码只做诊断，不弹「需要你」。
6. **语言是整体开关**：`i18n::set` 进程级全局，测试要加锁。
7. **插件先清单/权限、后执行 JS**：没有权限模型不要跑第三方 JS。
8. **cb-js 边界要诚实**：DOM 是可查询的本地树（快照来源，不回写页面），fetch 是预取缓存版，外链脚本不执行。
9. **写文件用 read/edit/write 工具，别用 PowerShell 改源码**；大文件只做定点 edit。
10. **md 图片与 `images[]` 是同一份事实**；截断不删图。
11. **fetch 重试只覆盖 network/timeout**；429 由调用方按 `retry_after_s` 决定。
12. **attempts 只放 `meta.attempts`**（error.attempts 标了 skip）。
13. **container_score 是字面占比**，「是不是文章页」看 `is_article`。
14. **cassette 兼容**：`error` 可缺省；同 URL 多条按顺序消费，用完重复最后一条；`ReplaySession` 可跨 fetch 共享。
15. **bench 失败自己打信封并 `return Ok(exit_code)`**，别再让 main 打第二份错误信封。
16. **daemon 协议是单连接单请求**：别在连接里塞 multiplex；会话状态只活在 daemon。
17. **MCP sampling 是阻塞 stdio 往返**：采样器同步完成再返回 ready future；否则 fetch future 会被非 Send 的锁毒化。
18. **`--stream` 与 `--json` 互斥**：流式输出不是信封。

## 下一步建议顺序

1. **外链脚本执行策略**：先做「同源 + 白名单 + 大小/数量上限 + 超时」，并且只对显式开关生效；这是安全面，别默认开。
2. **DOM 收尾**：布局/CSS/innerHTML/表单值；再把 JS 修改回写成新的 RenderDoc（现在只影响 JS 内部）。
3. **插件 JS 钩子**：复用 cb-js，把站点适配器挂到 fetch 后/抽取前；权限模型已有，先接 `extract` 钩子。
4. **daemon 增强**：多请求并发（每连接一条 pipeline）、句柄 TTL/上限、`cb session watch --since` 持续等 delta、把 cb-js 挂到 daemon 会话上（同一页面多次 eval）。
5. 旧账：`export --verify`、`history` TSV 表头/时间戳、selftest skipped 原因。

## 已知瑕疵（不阻塞）

- TUI 鼠标只有滚轮 + 单击聚焦；webui 无导航历史/SSE 重连细节。
- 插件只有清单/权限/发现，JS 钩子未执行。
- daemon 一次只处理一个 tools call（Mutex 串行）；采样等待上限 300s。
- 离线 CI 需要 NASM；外网站点仍可能三指纹全超时（有 `meta.attempts` 可自证）。

## 常用入口

- 状态板：[TASKS.md](TASKS.md)
- 台账/裁决：[PROGRESS.md](PROGRESS.md)
- 第二轮任务单：[FIX_TASKS_2.md](FIX_TASKS_2.md)
- 试用反馈：[TRIAL_REPORT_2.md](TRIAL_REPORT_2.md)
- 数据库设计：[database.md](database.md)
- TUI 使用与人工接力清单：[tui.md](tui.md)
- 设计文档：[superpowers/specs/console-browser-design.md](superpowers/specs/console-browser-design.md)