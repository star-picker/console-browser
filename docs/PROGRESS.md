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

### 实机反馈修复 · 只对真登录介入人工 + CI 门禁

- **误判修复**：之前把「页面里有验证码标记」直接当登录墙，导致 baidu 这类页一跳就弹「需要你」。现在：
  - 只有**真登录**（password 表单 + 登录路径 / 401 www-authenticate / 2FA）才触发人工接力；
  - 验证码只做诊断（`detect_captcha`），不再打断浏览；
  - 扫描前先剥掉 `<script>/<style>`，脚本里的 `nc_`/`grecaptcha` 不再算证据。
  （login 模块 7 测试，含「脚本噪声不触发」「验证码单独不算登录墙」两条新回归。）
- **登录页体验**：真登录墙时不再把整页换成提示页，而是**保留真实页面内容**，顶部贴一条「需要登录 / login required」通知条；tab 标题用页面真实标题。
- **CI 门禁**：本地按 CI 三条硬门禁逐条验证 —— `cargo fmt --all --check` 通过、`cargo clippy --workspace --all-targets -- -D warnings` 零告警、`cargo test --workspace` **166 passed**；另加一条 CI 冒烟步骤 `cargo run -q -p cb-cli -- --db ci.db --json selftest`，防止「测试绿但二进制起不来」。
- 顺带修掉一处真实笔误：`mouse: !args.no_mouse && !args.no_mouse`。
- 待办（用户要求，下一轮）：语言开关改成**单语言整体切换**（TUI 文案 + CLI 帮助都只显示所选语言，不再中英拼接）。

### M3-UI · 收尾：DPAPI、clippy 归零、目标达成

- **cookie 加密**：`SecretBox` 增 `DpapiBox`（Windows，`CryptProtectData/CryptUnprotectData` + `CRYPTPROTECT_UI_FORBIDDEN`），`default_secret_box()` 在 Windows 自动用它，其它平台退回 `PlainBox`；失败时回落明文标记，不丢数据。2 条测试（DPAPI roundtrip 且密文不含明文、能读旧的 plain 标记数据）。
- **clippy 归零**：`cargo clippy --workspace --all-targets` 零告警（修掉未使用字段、过多参数、可简化布尔、items-after-test-module 等；顺带发现并修正 `mouse: !no_mouse && !no_mouse` 这处真实笔误）。
- **回归**：`cargo test --workspace` **165 passed**；`cargo fmt --check` 通过。
- Ruling: 大文件（app.rs 等）一律用 read/edit 定点改，不做整文件覆盖 —— 这轮有两次整文件写入把 app.rs 截断，靠 git 还原才救回 —— 代价：改大文件要多次小编辑。
- Ruling: `--plain` 做成 `cb plain <url>` 子命令而不是全局开关 —— 避免与 `--json` 组合歧义 —— 代价：与计划里的字面写法不同，文档已注明。
- 诚实缺口：CDP 那条链路用 mock/纯函数测过（解析、粘贴、目标选择、过期过滤），**没有在自动化里真的拉起 Edge 登录一次**；TUI 交互用 TestBackend 渲染 + 非 TTY e2e 覆盖，真终端手感需人工验一次。

### M3-UI · T11/T12 落地 + 输入法修复（回应实机反馈）

- **实机反馈修复**：输入框打不出冒号/大写 —— 之前只接受「无修饰键」的字符，Shift 被误杀。新增 `text_input_char`（放行 Shift，只排除 Ctrl/Alt），有测试（`:`、`G` 可输入；Ctrl-W/Alt-1 不输入）。
- **键位可发现**：地址栏可直接敲 `?` / `/?` / `cb:help` 呼出帮助浮层；`cb:history|links|palette|trace|ai|reload|back|forward|top|bottom|quit|export-md|export-html|lang` 都能从地址栏触发（route::Target::Internal）。空状态提示也改成中英双行 + 一行键位速览。
- **T11 人工接力（cb-assist）**：
  - 浏览器发现（Edge/Chrome 常见路径）、临时 profile + `--remote-debugging-port`、CDP `Network.enable` + 轮询 `getAllCookies`（tokio-tungstenite）、会话 cookie 出现即收工、结束杀进程并删临时目录（`--keep-open` 保留）。
  - 纯函数可测：`import_manual_cookies`（Cookie 头 / Set-Cookie 两种写法）、`pick_target`、`cookies_from_cdp`（过滤过期）。
  - 入口：TUI `e`（打开助手浏览器，收工后自动写 jar 并重载）、`c` 粘贴 Cookie；CLI `cb login <url> [--manual] [--browser edge|chrome] [--keep-open] [--timeout-secs]`。
- **T12 打磨**：鼠标滚轮/点击聚焦链接、`--no-mouse`、`--ascii`（ASCII 线框与箭头）、`NO_COLOR`、终端过小保护。
- 新增 `cb plain <url>`：纯文本（去 markdown 标记）输出，管道友好 —— 对应计划里的「--plain」入口（裁决：做成子命令而不是全局开关，避免和 `--json` 组合歧义）。
- Ruling: futures-util 在镜像上锁到 0.3.32（0.3.34 的 futures-macro 镜像缺版本），用 `cargo update --precise` 降级 —— 属于依赖水面，不影响功能 —— 代价：等镜像补齐后可升级。
- 证据：`cargo test --workspace` 全绿（新增 assist 6、login/plain 3、input 2、internal route/1）；clippy/fmt 干净。

### M3-UI · T7-T10 落地：取消、链接列表、会话恢复、浮层

- **T7 取消**：Browse 模式按 Esc → 标签加载代数 +1，迟到结果按 gen 丢弃，页面回到「已取消」通知页；有测试（cancel 后 on_loaded 不得落地）。
- **T8 链接列表**：`L` 从当前 RenderDoc 的链接表填充浮层（`[id] 文本 / URL`），Enter 直接导航；焦点环/查找此前已通。测试覆盖填充与选中。
- **T9 会话恢复**：store 增 `save_tabs/load_tabs`（快照语义：整批覆盖）；App 增 `snapshot_tabs/restore_tabs`（过滤空占位条目、恢复导航栈与滚动）；driver 启动时恢复（`--no-restore` 关闭）、退出时保存。store 与会话各 1 条测试。
- **T10 浮层**：
  - 历史：`Ctrl-H` → driver 从 sqlite 取 100 条填充，Enter 打开。
  - 命令面板：静态动作表（20 项）+ `action:<name>` 回投到 App，测试断言每项都能映射到动作。
  - 命令行 `:`：提交后 driver 用 `current_exe` 起子进程跑原 CLI 参数，输出截断进状态栏/日志。
  - AI 面板：`Ctrl-A` 进输入模式，提交后带「标题+URL+前 120 行」上下文调模型，回答进浮层。
  - 日志：App 维护 200 条 trace 环，`cb:trace` 浮层倒序展示。
- Ruling: 命令行用**子进程调用自身**而不是把 clap 搬进 cb-tui —— 依赖方向干净（cb-cli 依赖 cb-tui，不能反向），且复用全部参数解析 —— 代价：每次命令多一次进程启动（毫秒级）。
- Ruling: 会话快照落 `tabs` 表按「整批覆盖」，不做增量 —— 语义简单、不会残留幽灵标签 —— 代价：多标签频繁切换时写放大（可忽略，行数少）。
- 证据：`cargo test --workspace` 全绿；新增 store 1 + cb-tui 6 条测试。

### M3-UI · T4-T6 落地：能跑起来的全屏浏览器

- **T4 cb-render**：`Layout::new(doc, width)` → 折行行（CJK 用 unicode-width）、表格列对齐、代码不折行、引用前缀、链接 id→行号索引、`find(needle)` 返回命中行。8 测试。
- **T5 cb-tui 纯逻辑**：`Mode`（Browse/Input/Overlay）、`Action`（40+ 动作）、`key_to_action`（键位表 + 唯一性/覆盖测试）、`EditBuffer`（光标/删词/历史召回）、`route::classify`（URL/域名/localhost/搜索/!bang/cb:）、`App`（标签栈、前进后退、加载代数丢弃过期结果、链接焦点环、查找循环、浮层、滚动钳制）。24 测试。
- **T6 屏幕与视图**：`Screen` 进 alternate screen + raw mode + `Drop`/panic 恢复（这就是「fresh and clean」：只画这一屏、不留滚动垃圾）；`views::draw` 画地址栏/标签条/正文/状态栏/浮层；`driver::run` 事件循环 + 后台加载线程（独立 tokio runtime，主线程只管画，加载不冻界面）；加载结果按 gen 比对丢弃。
- **CLI 接线**：`cb`（TTY）→ TUI；无 TTY → 原 REPL；`cb tui [--url/--no-mouse/--no-restore/--replay]`；`cb shell` 保留原 REPL。3 条 e2e（无 TTY 拒绝并退出 2、管道下仍是 REPL、help 列 tui/shell）。
- Ruling: 布局快照测试改用「TestBackend 渲染成字符串 + 关键断言」而不是 insta 快照文件 —— 少一堆二进制快照，断言更直白 —— 代价：细粒度排版回归不如快照敏感。
- Ruling: 本轮 `OpenAssist/export/palette/history` 先用状态栏提示占位，T10/T11 接实 —— 先把「打开就能浏览」跑通 —— 代价：几个键暂时是空操作。
- 证据：`cargo test --workspace` 全绿（cb-render 8 + cb-tui 31 + CLI 新增 3）；TUI 三件套已可编译成 `cb tui`。

### M3-UI · T2/T3 落地

- **登录墙识别**（cb-core/src/login.rs）：URL 路径、password 表单、2FA 关键词、验证码厂商标记（recaptcha/hcaptcha/turnstile/geetest/阿里 nc_/cloudflare/ddos-guard）、www-authenticate。输出 LoginWall{kind, provider, hints, evidence}，5 条单测。
- `FetchOutcome` 增 `doc`（结构化，`#[serde(skip)]` 不进 JSON）与 `login`；CLI/MCP 调用方可以「当页可读」但知道要人。clap 侧新增 `CbError::needs_login`（退出码 8）。
- **cookie jar**（cb-net/src/cookies.rs）：Cookie 结构 + CookieStore 接口 + 内存实现 + 解析 Set-Cookie（Domain/Path/Max-Age/Secure/HttpOnly/SameSite）+ 域后缀/路径匹配 + 过期与 secure 过滤，6 条单测。
- **持久化**（cb-store）：schema v3 增 `cookies`/`tabs`/`bookmarks`；Store 改为 `Arc<Mutex<Connection>>` 以支持共享给 jar；`StoreCookieJar` 实现接口，值经 `SecretBox` 封装；域查询按主机后缀展开（a.b.c 匹配 b.c 与 c）。
- **接线**：`Ctx.cookies` 注入 `NetClient`（请求带 Cookie、响应落库），登录墙判断可用「jar 里已有会话 cookie」给出更准提示。`cb cookies list|clear`（只报名字，绝不打印值）。
- Ruling: `SecretBox` 先给 `PlainBox`（本机 sqlite 明文 + 标记），DPAPI 实现排到 T12 —— 接口已固定，换实现不改调用方 —— 代价：当前版本 cookie 落盘是明文，文档与 `cb cookies list` 必须明说。
- 证据：cookies 6 单测 + store 4 单测 + 2 条 CLI e2e + 1 条「jar 有会话但页面仍要登录」的提示测试；全仓测试绿。

### M3-UI · T0/T1 落地（2026 迭代）

- **版本号**改为 `xx.yy.zz.YYYYMMDD.<gitshort>`：`cb-core/build.rs` 从 git 取提交 hash 与提交日期（可用 `CB_BUILD_COMMIT`/`CB_BUILD_DATE` 覆盖；无 git 时日期用 UTC 回退），`cb-core::version::VERSION` 注入到 `cb --version`、信封 `meta.engine`、MCP `serverInfo`。形状有测试守着（5 段、日期 8 位、hash 非空）。
- **结构化文档模型**（`cb-extract/src/{blocks,doc}.rs`）：`RenderDoc { title, blocks, links }` + `Block`（Heading/Paragraph/ListItem/Code/Quote/Table/Rule/Image/Notice）+ `Span`（Normal/Bold/Italic/Code + 链接 id）。链接 id 按出现顺序稳定，供 TUI 焦点与差量用。
- 6 条新测试（链接 id 稳定、嵌套列表深度、代码语言与行、表格首行表头、行内样式成 span、Notice 可渲染）。
- Ruling: TUI 走**独立 walker**（`to_document`），不改动现有 markdown 序列化 —— AI 主链路的 7 条 golden 零风险 —— 代价：一次 HTML 解析跑两遍（毫秒级），后续若要合并再以「两路输出等价」测试收口。
- Ruling: `Block::Quote` 内含 `Vec<Block>`（而非扁平 span）—— 引用里可能有列表/代码 —— 代价：比计划里的 `Quote{spans}` 多一层结构。
- 证据：`cargo test -p cb-extract` 6 + 7 全绿；版本形状测试 2 绿。

### M2 收尾 · 挑战升级最后一阶（模型兜底）

- 新增 `cb-core/src/challenge.rs`：截取挑战页前 2000 字符，问模型「下一步该干什么」，回答截 400 字符拼进 `needs_user.hint`。
- 触发条件：仅 `cb fetch --ai-escalate`（默认关闭，绝不偷偷花 token）；没有配置模型时保持原来的 challenge_unsolved 行为。
- 记账：成功/失败都写 `challenge_events`，strategy=`llm-assist`，`cb challenge stats` 能看到这一阶的胜负。
- 完整阶梯现在是：换指纹（最多 3 轮）→ 可选模型兜底 → `needs_user` 交人/交调用方（sampling 回调留 M3）。
- Ruling: 模型建议只作为「提示」返回，不自动执行（不自动设 cookie、不自动重放）—— 自动改请求头是安全面，需要人的判断 —— 代价：还得人工/agent 决策一次。
- 证据：cb-core 新增 1 单测（提示词有界 + 不诱导编造凭证）；CLI 新增 2 e2e（有模型→hint 带 model 建议且台账有 llm-assist；无模型→保持原错误）；`cargo test --workspace` 全绿。

### M2 切片 4 · 自主循环 + 反爬阶梯

- **agent**（cb-core/src/agent.rs）：`cb agent run "<goal>"`。循环 = 提示词（目标 + 已走步骤 + 剩余步数）→ 模型出**一个** JSON 动作（search / fetch / answer）→ 执行 → 观察（按 `--observation-chars` 裁剪）→ 下一轮。护栏：`--max-steps`、`--budget`（本任务 completion token 上限）、模型没按格式回就把整段当答案（不把用户卡死在循环里）。
- **反爬阶梯**（fetch 管道）：403 就换浏览器指纹重试，最多 3 轮（用户选的优先，然后另外两个）；每次尝试写 `challenge_events`（domain / kind / strategy / ok / detail）。`cb challenge stats` 按域名×策略汇总胜负，接上 schema v2。
- 复用：agent 的 search/fetch 都走同一套 `SearchOpts` / `FetchOpts`，回放能力一并继承 —— 测试全程离线。
- Ruling: agent 动作空间本轮只给 search/fetch/answer 三个 —— 没把握的「点击/填表」要等 M3 的会话与句柄 —— 代价：暂时不能做多步表单交互。
- Ruling: 403 每次重试都记一条失败的 challenge 事件（而不是只记最后一轮）—— 这样「哪个指纹对该域有效」才有样本 —— 代价：表增长快一些，靠 `challenge_summary` 的时间窗收敛。
- Ruling: `--budget` 按「本任务 completion token」计，不叠加 `ai_budget` 日预算；两者同时生效 —— 意图与额度分开管 —— 代价：用户要理解两个上限。
- 证据：`cargo test --workspace` 全绿（新增 agent 5、challenge 1）；clippy 无告警；fmt 通过。

### M2 切片 3 · AI provider + 出账 + 预算闸

- `cb-core/src/ai.rs`：`Provider`（deepseek / openai-compatible / ollama）、OpenAI 兼容 chat completions 调用、响应解析、**每次调用必出账**、按天预算闸。
- 新表（schema v2）：`llm_calls`（逐次明细）、`ai_budget`（按天累计）、`challenge_events`（挑战结果）。迁移靠 `PRAGMA user_version` 升到 2，老库自动补齐。
- 命令：`cb ai config`（看解析结果，只报密钥有无、绝不打印本体）、`cb ai usage [--days N]`（按天汇总）、`cb ai ask "<prompt>" [--max-tokens]`。
- 密钥来源：只用环境变量（`DEEPSEEK_API_KEY` / `OPENAI_API_KEY` / `CB_AI_KEY`），**不进库、不进仓库、不进日志**；有专门测试断言密钥不会出现在输出里。
- 超预算 → `budget_exhausted` + 退出码 9（调用前就拦，不烧钱）。
- Ruling: 模型调用走 `cb-net` 而不是另起 HTTP 栈 —— 复用同一套 TLS 指纹与录制回放 —— 代价：模型 API 也吃指纹 profile 的配置。
- Ruling: 预算默认「不限但必记账」（`CB_AI_DAILY_BUDGET` 才设上限）—— 不替用户拍金额 —— 代价：默认没有硬刹车，靠 `cb ai usage` 自查。
- 证据：cb-core 24 单测（含出账与预算闸两条 wire 测试）+ CLI 3 条（配置不泄密、无密钥报错、空台账）；`cargo test --workspace` 全绿；clippy 无告警。

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