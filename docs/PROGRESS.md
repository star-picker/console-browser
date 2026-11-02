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

### T5 fetch 管道 · T6 能力表/自检 · T7 MCP · T8 测试台

- T5：`cb fetch` 打通缓存 → 网络 → DOM → 提取 → 存库 → 历史；JSON 信封 + 退出码契约。e2e 4 绿（缓存命中、500→exit 3、html 原样、未知格式→exit 2）。
- T6：`ToolSpec` 注册表（browse_url / history_search / selftest_status），`cb tools`、`cb selftest` 八项检查（db/fs/net/tls/js/llm/cache/mcp）。契约测试 3 绿。
- T7：手写 JSON-RPC 2.0 stdio MCP 服务端（initialize / tools/list / tools/call / ping），`--replay` 可离线；握手测试 2 绿。Hermes 配置文档落地。
- T8：`scripts/cb-e2e.ps1`、Windows CI（NASM + fmt + clippy + test，全程离线）、README、`.gitattributes`。
- Ruling: replay 与本地缓存并存（去掉「回放时跳过缓存」）—— 回放只是网络后端，缓存语义不变 —— 代价：回放测试第二次读会命中缓存，需显式 `--fresh` 才重新回放。
- Ruling: M1 的 MCP 只宣告 3 个工具，`search_web` 延到 M2 —— 不宣告做不到的能力 —— 代价：与最初计划差一个工具名。
- 事故：用 PowerShell `Set-Content` 改源码导致 UTF-8 被二次编码、文件损坏；已从 git 还原并用 read/edit 工具重做。**规则：源码只用 read/edit/write 工具改，脚本只做只读检查。**
- 证据：`cargo test --workspace` 20 passed / 1 ignored（oracle）；`cargo clippy --workspace --all-targets` 无告警；`cargo fmt --all --check` 通过；`scripts/cb-e2e.ps1 -SkipTests` 退出 0。

### 独立审查 · M1 首轮（修复 8 项）

审查由独立 reviewer 只读完成，报 10 条。已修：

1. **CRITICAL 正文容器两段式评分**：语义容器门槛从 120 降到 40；结构性容器（body/main/article）永远递归，不再把整页压成一行。补两条回归测试（无语义容器的小页面 / 整页包裹 div 不得胜出）。
2. **`--to text` 名不副实**：新增 `to_plain_text`（去标记、表格转制表符、链接转 `text (url)`），CLI 与 MCP 都走它。测试锁死「纯文本里不得出现 # 与 **」。
3. **MCP 非 UTF-8 行会杀死服务端**：改成 `read_until(b'\n')` + lossy 解码，坏行不中断会话。测试：先写非法字节行，再 initialize，仍要应答。
4. **字符集**：按 Content-Type charset（其次 `<meta charset>`）用 `encoding_rs` 解码，GBK/Big5/Shift_JIS 不再乱码。测试覆盖 GBK 字节与 meta 声明、带引号的 charset。
5. **体积与类型闸门**：正文上限 8MB（`response_too_large`）；非文本类型直接拒绝并给出明确错误，不再把 PDF/视频塞进解析器和库里。
6. **README 与实现不符**：删掉未实现的 robots.txt / 1rps / 文件日志承诺，改成「M2 计划」；新增真实存在的 `cb db prune|stats`。
7. **错误映射**：只有 403 算挑战；429/5xx 是瞬时网络错（可重试），不再把 CDN 抖动说成反爬。
8. **省 token 的估算**：CJK 一字 ≈ 一 token，其余 4 字符 ≈ 一 token；截断改成逐字符回退直到估算达标（中文页不再超预算 3-4 倍）。

顺带修：
- noise 判断从子串改成 class/id **词元**匹配（不再误杀 download-/shareholder）；页眉页脚只在正文容器外丢弃，文章内 header 保留。
- 缓存命中保留重定向后的 `final_url`（元数据不再翻转）；`bytes` 不再是硬编码 0。
- `--json` 与 `--out` 同时给 = 用法错误（之前 --out 被静默忽略）。
- cassette 录制剔除 Set-Cookie / Authorization 等凭证类响应头。
- `prune` 的缓存条件从「过期后再留 7 天」改成「按过期时间」，并新增 `--log` 落账（logs 表不再是死表）。
- 补测试：退出码字符串表→退出码、`--to text`、缓存命中重定向一致性、MCP 坏行韧性。

Ruling: 审查报告的 10 条里「GitHub 文件列表被当成正文」不在本轮修 —— 静态 HTML 里没有 README 正文（在 JS 载荷里），属 M3 的 JS 执行 + 站点适配器范畴 —— 代价：M1 对 JS 站与部分 GitHub 页面质量不达标，已在 TASKS 里标明。
- 证据：`cargo test --workspace` 28 passed / 1 ignored；`cargo clippy --workspace --all-targets` 无告警；`cargo fmt --all --check` 通过。

### M2 切片 2 · 搜索引擎适配 + search_web

- 新模块 cb-core/src/search.rs：`Engine`（ddg / bing / gh / arxiv）+ 每引擎 URL 构造 + SERP 解析 + 历史记账。
- DDG 的 `/l/?uddg=` 跳转链接会还原成真实地址（否则 AI 拿到的是一条追踪链）。
- GitHub 走 REST(`/search/repositories`)，arXiv 走 Atom；两者都不是 HTML 抓取，省一次解析。
- 接入三处：`cb search <q> --engine … --limit …`、MCP 工具 `search_web`、能力表（registry 现在 4 个工具）。
- Ruling: 搜索不写 `pages` 表、只写历史 —— SERP 是「入口」不是「正文资产」，正文在结果链接里 —— 代价：`cb history` 能看到搜索但 `pages` 里没有 SERP 全文。
- Ruling: 搜索本轮不接本地缓存 —— 上一次结果过期策略还没定（引擎侧时效性强）—— 代价：重复搜索会重复请求；M2 下一步接 cache_entries 时再定 TTL。
- 证据：新增 9 个测试（cb-core 5 个解析单测 + CLI 4 个 e2e + MCP 1 个工具用例），`cargo test --workspace` 58 passed / 1 ignored；clippy 无告警。

### 迭代 · 双语 + 交互外壳 + 逐命令帮助 + 数据库文档

- **交互模式**：`cb` 不带参数（或 `cb shell`，或双击 exe）= rustyline 外壳，prompt 里直接跑 `fetch/export/tools/selftest/db/mcp`，不再每次重开进程；历史落在 `shell_history.txt`；内置 `help` / `lang zh|en` / `clear` / `exit`；Ctrl-C 取消本行、Ctrl-D 退出。
- **中英双语**：新增 `cb-core/src/i18n.rs`（`Lang`、探测顺序 `--lang` > `CB_LANG` > 库内设置 > 系统区域、词条表 + 完整性测试）。CLI 的 flag/命令帮助用「中英同行」写法，一眼看两种语言；运行时消息与外壳文案走 i18n。
- **逐命令帮助**：每个子命令都有 `after_help` 例子；`cb <cmd> --help` 与 `cb help <cmd>` 都能用；交互模式里敲 `fetch --help` 只打印帮助、不退出外壳。
- **数据库文档化**：新增 `docs/database.md`（文件位置、表结构、缓存键公式、保留期、排查命令、后续表计划）；新增 `cb db schema`（表 + 列 + 行数）。
- Ruling: clap 的帮助文案不做运行时翻译，直接写「中文 · English」同行 —— 少一套本地化机制、两种语言永远同时可见 —— 代价：帮助文本更长，第三种语言要重写这些行。
- Ruling: 交互外壳先做 REPL（rustyline）而不是全屏 TUI —— 先让「双击就能用」成立，全屏界面留 M4 —— 代价：M4 要重做布局层，REPL 保留当降级路径。
- 证据：`cargo test --workspace` 43 passed / 1 ignored（新增 i18n 3、shell 4、db schema 1）；clippy 无告警；fmt 通过。

### M2 切片 1 · 单文件 HTML 导出

- `cb export <url> --to html|md [--out FILE]`：`single_file_html` 去 `<script>` / `<noscript>` / 外链 `<link>` / 内联 `on*` 属性，替换为权威标题，注入自包含阅读 CSS，末尾加 source 脚注；无 `<head>` 的碎片 HTML 自动补壳。
- 安全：标题与 URL 做 HTML 转义（`&lt;` / `&amp;`），不让导出文件变成注入面。
- Ruling: 导出允许 `--out` 与 `--json` 并存（文件照写，信封只报 path/bytes）—— 导出是「要文件」的场景，跟 `fetch` 的互斥不同 —— 代价：两个命令的规则略不对称，已在 README/用法里写明。
- 证据：`cargo test --workspace` 33 passed / 1 ignored；clippy 无告警；fmt 通过。

### 推送 · 远端落地（2026-01-01）

- HTTPS 被墙（`git push` 连接被 reset）。改用 SSH over 443：`~/.ssh/config` 里 `Host github.com → ssh.github.com:443`，已存在 ed25519 密钥且 GitHub 侧已登记。
- 远端 `star-picker/console-browser` 已有 GitHub 自动生成的 Initial commit（LICENSE=M⼀T、README）。用 `--allow-unrelated-histories` 合并，README 取本地版本，LICENSE 保留远端。
- 结果：`main` 已推送（6 → 8 个提交）。
- Ruling: 不用 force push 覆盖远端初始提交，改为合并 —— 远端历史也是用户资产 —— 代价：历史里多一个 merge 提交。
- 提醒：GitHub PAT 曾以明文出现在会话中，**仍需吊销并重建**；SSH 已可用，日常推送不再需要 PAT。

### 安全事件 · 凭证泄露（2026-01-01）

- 会话中有人贴入一枚 GitHub PAT。该令牌已视为**泄露**，不得写入仓库、remote、配置、日志或脚本。
- 处置：不落盘、不入 git、不使用；请到 GitHub 设置里**吊销并重建**。
- 需要推送时，改用不在会话中出现的凭证（本地 credential manager 或交互式 `gh auth login`），并确认命令回显不含密钥。
- 已扫描：工作区与 git 历史中无 GitHub 令牌前缀（PAT 明文前缀）命中；本文件也不写这些前缀字面，避免自触发扫描。
- Ruling: 任何密钥只走环境变量或本地未跟踪文件；`.env` 已在 `.gitignore`。代价：用户要多做一步本地配置。