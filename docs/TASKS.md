# 任务与状态

> 台账（为什么这么做、证据）在 [PROGRESS.md](PROGRESS.md)；这里只记状态。
> 最后更新：第三阶段（daemon / AI 流式 / MCP sampling / cb-js 事件循环）2026-11-03

## 第二轮试用修复（Hermes TRIAL_REPORT_2 / FIX_TASKS_2）

- [x] FIX-1 `--to md` 图片：按位置插入、alt 空用文件名、figcaption 斜体、`--images/--no-images`、截断保图；链接包图不再在 Links 段重复
- [x] FIX-2 fetch 重试 + fingerprint 阶梯：连接 5s / 读取 25s / 总预算；network+timeout 退避换指纹；`meta.attempts`；全挂聚合错误
- [x] FIX-3 `meta.cache.age_s` 真值（冷启动仍 null）
- [x] FIX-4 `extract.container_score` 真值（可见文本占比）+ `is_article` 标注
- [x] FIX-5 429 → `rate_limited` + `retry_after_s`（fetch/search 都覆盖）
- [x] FIX-6 DNS 失败 → `dns_error`、不可重试、消息带域名
- [x] FIX-7 `--scope` 回执：matched / chars_before / chars_after；未命中不报错、stderr 提示
- [x] FIX-8 webui 首页 `?url=` 生效（转义注入 + 自动 navigate）
- [x] FIX-9 bench 失败保留 samples/completed/planned/p50 + data.error
- [x] 测试与门禁：每条带测试；fmt/clippy/test 全绿；两条真实网络用例 `#[ignore]`

## M1 无头内核

- [x] CLI 骨架 + JSON 信封 + 退出码契约（usage/network/challenge/not_found/timeout/needs_auth/budget/parse_failed）
- [x] cb-net：wreq + 指纹冒充（chrome/firefox/safari）+ cassette 回放/录制 + cookie jar
- [x] cb-extract：正文评分、markdown、token 预算、图片（懒加载属性链 + 占位判定）、纯文本
- [x] cb-store：sqlite（WAL，schema v6）pages/cache/history/logs/settings/llm_calls/ai_budget/challenge_events/cookies/tabs/bookmarks/page_revisions/site_strategy + FTS5 + DPAPI cookie 加密
- [x] 搜索：ddg/bing/gh/arxiv + `--engine auto` 回退 + `empty_reason/http_status/bytes` 证据 + parse_failed 显性失败
- [x] challenge 阶梯：403 → 换指纹 3 轮 + `challenge_events` + `cb challenge stats`（LLM 升级可开）
- [x] AI：provider/记账/日预算；agent 最小循环
- [x] MCP server（4 工具）+ `cb tools --json`（带 description）
- [x] export：单文件 HTML / markdown / 纯文本；`cb plain`
- [x] 双语 + **单语言整体切换**（CLI 帮助 + tools + TUI 全部文案）
- [x] 版本号 `xx.yy.zz.YYYYMMDD.githash`

## M2 阅读体验

- [x] 浏览器式排版：限宽居中栏（≤92 列）、标题层级间距、段落呼吸、代码/引用/表格样式
- [x] `cb render <url>` 离屏文本帧（无 GUI 时的自查/预览）
- [x] 门户页不压平、JSON 数据不当正文、空 alt 图标不污染

## M3 界面

- [x] 全屏 TUI：alternate screen、原地重绘、omnibox、链接焦点、找字、标签会话恢复、浮层（帮助/历史/链接/面板/AI/日志）、加载动画、Ctrl-C 取消、`?`/`cb:help` 呼出帮助
- [x] 人工接力：Edge/Chrome + CDP（`cb login`、TUI `e`/`c`）+ 粘贴 Cookie 兜底（真浏览器链路见 docs/tui.md 手动清单）
- [x] webui：axum + SSE 单文件前端，同状态镜像，浏览器式排版
- [ ] 鼠标右键/双击等更细交互（当前滚轮+点击聚焦）

## M4 JS 引擎（cb-js，进行中）

- [x] QuickJS 运行时 + `cb js` + `js_error` 错误码
- [x] DOM 只读快照 + `querySelectorAll`（#id/.class/tag）
- [x] BOM 桩：location/localStorage/navigator/console
- [x] **fetch 桥**（预取缓存版：调用方先走 cb-net 抓，脚本里同步取缓存；不是真并发网络）
- [x] 事件循环：Promise 微任务泵 + setTimeout/setInterval/clearTimeout，`cb js --loop-ms`
- [x] DOM 桩增强：querySelector/getElementById/createElement/appendChild、属性读写、textContent、事件
- [x] 完整 DOM 树：父子/children/parentElement、后代/子代/逗号/属性/`:nth-child` 选择器、textContent/classList/append/remove/insertBefore、文档事件、title 回写
- [ ] DOM 收尾：布局/CSS/innerHTML/表单值；JS 修改回写成 RenderDoc
- [ ] 外链脚本执行策略（同源/白名单/大小数量上限/超时，显式开关，默认关）
- [x] JS 站实测：百度首页 7 段内联脚本、title 正确；deepseek 首页 0 段（外链 bundle）；百度 SERP 空壳（拦截）；GitHub 文件树当前网络不可达（attempts 有证据）

## M5 插件

- [x] plugin.json schema + 权限白名单（fail closed）+ 目录发现 + `cb plugin list/info/dir`
- [ ] 插件 JS 钩子执行（与 cb-js 共用引擎）+ 站点适配器/挑战 hook 示例

## M6 会话与打磨

- [x] cb-session + daemon（命名管道、句柄 `@eN`、delta 快照）
- [x] AI 流式输出（`cb ai ask --stream`）
- [x] MCP sampling 回调（挑战/AI 升级时问调用方）
- [ ] 日志轮转（已做 jsonl+轮转+脱敏）之外的查询/统计
- [ ] 性能守门与长文实测

## 已完成的旧计划（T0-T13）

- [x] T0-T8、T10：骨架、版本戳、RenderDoc、登录墙、cookie jar、布局引擎、状态机、全屏视图、加载/错误页、链接焦点/查找/浮层
- [x] T9 标签页与会话恢复（快照存 sqlite，启动恢复）
- [x] T11 人工接力（cb-assist + CDP + `cb login`/`cb cookies`）
- [x] T12 鼠标/`--no-mouse`/`--ascii`/`NO_COLOR`/小终端保护 + cookie DPAPI
- [x] T13 接线与文档（TUI 默认、`cb shell`/`cb plain` 保留、docs/tui.md、README/TASKS/PROGRESS、CI 三门禁）
