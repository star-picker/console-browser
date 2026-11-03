# 交接文档（HANDOFF）

> 给下一个会话的接手人看。写于第二轮试用修复完成，HEAD `0e43633`。

## 一句话状态

Hermes 第二轮试用报告（`docs/TRIAL_REPORT_2.md`）里的 9 条 FIX（`docs/FIX_TASKS_2.md`）全部修完，每条带测试、单独 commit；三门禁全绿，240 个测试通过（50 个测试二进制）。设计文档里仍剩 4 项没做：cb-js 事件循环/完整 DOM、daemon+句柄 `@eN`、AI 流式、MCP sampling。

## 怎么验证（每次改动前先跑一遍）

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# 冒烟（Hermes 的脚本，需要网络；D:\hermes-tools\cb_smoke.py）
cargo build -p cb-cli
cd D:\hermes-tools; python cb_smoke.py
```

## 本轮改了什么（对应 FIX 单）

1. **FIX-1 md 图片**（`025f922`、`0e43633`）：`push_image` 统一记录+输出；图片独立成行；alt 空用文件名；`figcaption` → `*caption*`；`--images/--no-images`；截断时图片块强制保留。md 图片数 == `images[]`（链接包图不再在 `## Links` 重复）。
2. **FIX-2 重试/指纹阶梯**（`db6b70c`）：连接 5s / 读取 25s / 总预算 30s（`--timeout` 覆盖）；只有 `network`/`timeout` 退避重试（500ms→1500ms）并换指纹；404/429/DNS/403 不内部重试；`meta.attempts` 报每次 `{profile, ok, ms, error}`；全挂 message 汇总。
3. **FIX-3**（`6b5430e`）：`meta.cache.age_s` 真值；`data.cache_age_s` 兼容保留。
4. **FIX-4**（`7ae67a8`、`0e43633`）：`container_score` = 选中容器可见文本 / 页面可见文本（跳过 script/style）；新增 `extract.is_article`。
5. **FIX-5**（`c6a63dd`）：429 → `rate_limited` + `retryable:true` + `retry_after_s`（Retry-After 优先，缺省 5s）；fetch/search 共用。
6. **FIX-6**（`868452f`）：DNS 失败 → `dns_error`、不可重试、消息带主机名；wreq 超时单独归 `Timeout`。
7. **FIX-7**（`f68bbfd`）：`meta.extract.scope {selector, matched, chars_before, chars_after}`；未命中不报错、stderr 提示。
8. **FIX-8**（`a0848a0`）：webui `GET /?url=...` 注入转义后的起始地址并自动 navigate。
9. **FIX-9**（`701f472`）：bench 失败保留 `samples/completed/planned/p50` + `data.error`；cassette 增加 `ReplaySession` 跨请求游标。

## 关键约束（别推翻，都是踩过坑后的裁决）

1. **两条渲染链路**：AI 用 markdown（`cb-extract/src/lib.rs`），TUI/webui 用 RenderDoc（`doc.rs`）。改抽取时两边都要过回归（7 golden + doc 测试）。
2. **块级判定用 BLOCK_TAGS**：不要退回「直接子元素里有没有非容器标签」那种写法——那会把门户页压成一行（百度事故）。
3. **JSON 数据转储不进正文**：`looks_like_json_dump` 保护，别删。
4. **搜索 parse_failed 必须失败**：退出码 10，别让它伪装成 `ok:true` 空结果。
5. **人工接力只在真登录墙触发**：验证码只做诊断（剥 script 后再判），不弹「需要你」。
6. **语言是整体开关**：单语言输出，不拼中英；`i18n::set` 是进程级全局，测试里要加锁。
7. **插件先清单/权限、后执行 JS**：没有权限模型不要跑第三方 JS。
8. **cb-js 边界要诚实**：DOM 是只读快照、fetch 是预取缓存版；没实现事件循环就别声称支持框架页面。
9. **写文件用 read/edit/write 工具，别用 PowerShell 改源码**（UTF-8 被破坏过两次）。
10. **大文件（app.rs/main.rs）只做定点 edit，不要整文件重写**（被截断过多次，git 救回）。
11. **md 图片与 `images[]` 是同一份事实**：只报 markdown 里真实出现过的图；图片不计入截断删除。
12. **fetch 重试只覆盖 network/timeout**：429 可重试但由调用方按 `retry_after_s` 决定，别在 fetch 内狂重试限流站。
13. **attempts 只放一处**：`meta.attempts`；`error.attempts` 标了 `#[serde(skip)]`，别让信封里出现两份。
14. **container_score 是字面占比**，不是质量分；「是不是文章页」看 `is_article`。不要把链接密度再混进去。
15. **cassette 兼容**：`error` 字段可缺省；同 URL 多条按顺序消费，用完重复最后一条；`ReplaySession` 可跨 fetch 共享。
16. **bench 失败时自己打信封并 `return Ok(exit_code)`**；不要 `return Err`（会被 main 再打一次错误信封）。

## 下一步建议顺序

1. **daemon + 句柄 `@eN`**：`cb daemon` 长驻进程，命名管道（Windows）承载会话；句柄格式 `@1`、`@2` 指向页面快照；delta 只回变化。做完后 cb-js 事件循环能挂上去。
2. **AI 流式**：`cb ai ask --stream`，需要 cb-net 支持增量读；如果没有 bytes_stream，就先在 ai.rs 里换流式客户端。
3. **MCP sampling**：当挑战/AI 需要模型但本地没 key 时，server → client 发 `sampling/createMessage`，等响应继续；注意 stdio RPC 的状态机（当前是请求-响应单线程循环）。
4. **cb-js 事件循环**：Timer 队列 + Promise 微任务泵；做法是 eval 后循环抽 job，参考 rquickjs 的 `Context::execute_pending_job`。
5. JS 站实测：deepseek 分享页、GitHub 文件树、一个 SERP（记录在 docs/tui.md 的手动清单风格里）。

## 已知瑕疵（不阻塞，但别忘）

- 插件 JS 钩子未执行（只有清单/权限/发现）。
- webui 没有导航输入历史/SSE 断线重连细节（前端有 retry 提示）。
- TUI 鼠标只做滚轮 + 单击聚焦，没有双击/右键。
- `export` 仍无 `--verify` 入口（TRIAL_REPORT P1-4）。
- `history --limit` 输出 TSV 无表头、时间戳是 epoch（P2-2）。
- `selftest` 的 skipped 不区分原因（offline / not_implemented / not_configured，P2-3）。
- 离线 CI 需要 NASM（BoringSSL），workflow 已装。
- 真实网络下境外站仍会三指纹全超时；`meta.attempts` 现在能证明「试过什么」，但环境问题不是工具能修的。

## 常用入口

- 状态板：[TASKS.md](TASKS.md)
- 台账/裁决：[PROGRESS.md](PROGRESS.md)
- 本轮任务单：[FIX_TASKS_2.md](FIX_TASKS_2.md)
- 试用反馈：[TRIAL_REPORT_2.md](TRIAL_REPORT_2.md)
- 数据库设计：[database.md](database.md)
- TUI 使用与人工接力清单：[tui.md](tui.md)
- 设计文档：[superpowers/specs/console-browser-design.md](superpowers/specs/console-browser-design.md)