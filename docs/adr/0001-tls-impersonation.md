# ADR 0001 · TLS 指纹冒充选型

- 状态：已采纳（待 oracle 实测确认）
- 日期：T1

## 背景

反爬的命门是 TLS ClientHello 与 HTTP/2 帧的行为特征（JA3/JA4、h2 settings 与优先级顺序）。标准库的指纹一眼假。

## 决定

使用 **wreq 0.15.3**（rquest 后继版本）作为网络客户端，通过其 browser emulation profile 选择 Chrome/Firefox/Safari 的 TLS + HTTP/2 特征。

## 理由

- 开箱提供多种浏览器的完整指纹族（TLS 版本、套件顺序、扩展顺序、h2 settings/优先级、header 顺序）。
- 纯 Rust，不引入 WebKit/Blink 之类内核，也不依赖系统 curl/openssl。
- 0.16.3 需要 rustc 1.98+，本机 1.96，故锁 0.15.3（后续升级只需改版本）。

## 备选

1. `rquest`（更旧分支）—— 无必要。
2. 自建 rustls ClientHello 定制 —— 可控性最高，工作量最大，作为备用方案。
3. 直接用 hyper —— 无指纹能力，只用于回放/测试后端。

## 代价

依赖一个较大的 HTTP 栈；升级需要跟随上游。若其指纹与实际浏览器漂移，用 `tests/tls_profile.rs`（oracle 校验）发现。

## 验证

`cargo test -p cb-net --test tls_profile -- --ignored`（需要联网，向 oracle 校验 ja3/ja4）。
