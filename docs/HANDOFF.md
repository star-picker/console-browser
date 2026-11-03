# 交接文档（HANDOFF）

> 给下一个会话的接手人看。写于目标第 20 轮，HEAD `46bc05a`。

## 一句话状态

试用报告（docs/TRIAL_REPORT.md）里的 P0/P1 全部完成；设计文档里只剩 4 项没做：cb-js 事件循环/完整 DOM、daemon+句柄 `@eN`、AI 流式、MCP sampling。CI 三门禁全绿，199 测试通过。

## 怎么验证（每次改动前先跑一遍）

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# 冒烟
cb --version
cb --db (New-TemporaryFile) --json selftest   # all_ok:true / status:degraded
```

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

## 下一步建议顺序

1. **daemon + 句柄 `@eN`**：`cb daemon` 长驻进程，命名管道（Windows）承载会话；句柄格式 `@1`、`@2` 指向页面快照；delta 只回变化。做完后 cb-js 事件循环能挂上去。
2. **AI 流式**：`cb ai ask --stream`，需要 cb-net 支持增量读；如果没有 bytes_stream，就先在 ai.rs 里换流式客户端。
3. **MCP sampling**：当挑战/AI 需要模型但本地没 key 时，server → client 发 `sampling/createMessage`，等响应继续；注意 stdio RPC 的状态机（当前是请求-响应单线程循环）。
4. **cb-js 事件循环**：Timer 队列 + Promise 微任务泵；做法是 eval 后循环抽 job，参考 rquickjs 的 `Context::execute_pending_job`。
5. JS 站实测：deepseek 分享页、GitHub 文件树、一个 SERP（记录在 docs/tui.md 的手动清单风格里）。

## 已知瑕疵（不阻塞，但别忘）

- `meta.cache.age_s` 还是 null（缓存层没带年龄）。
- `meta.extract.container_score` 预留未接。
- 插件 JS 钩子未执行（只有清单/权限/发现）。
- webui 没有导航输入历史/SSE 断线重连细节（前端有 retry 提示）。
- TUI 鼠标只做滚轮 + 单击聚焦，没有双击/右键。
- 离线 CI 需要 NASM（BoringSSL），workflow 已装。

## 常用入口

- 状态板：[TASKS.md](TASKS.md)
- 台账/裁决：[PROGRESS.md](PROGRESS.md)
- 试用反馈：[TRIAL_REPORT.md](TRIAL_REPORT.md)
- 数据库设计：[database.md](database.md)
- TUI 使用与人工接力清单：[tui.md](tui.md)
- 设计文档：[superpowers/specs/console-browser-design.md](superpowers/specs/console-browser-design.md)
