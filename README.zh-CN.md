# Pocket Rust

[English](README.md) · 简体中文

多多随身房间协议的跨平台 Rust 实现，包含 API v2、Chat 语音消息及 Ambient 音频状态逻辑。
Rust 应用使用 `pocket-core`，其他语言通过版本化 C ABI 调用 `libpocket_core`。
库没有 Go／Tailscale 嵌入节点依赖，Linux 使用进程已经可达的网络。

核心负责协议、语音分帧、会话状态、幂等重试、取消和播放进度；平台负责网络执行、麦克风／扬声器、
凭据、持久化和后台运行。Android、Linux、未来 iOS 均可使用同一核心，保留各自原生界面和音频接口。
Linux 参考端接收预编码语音文件，用于接入与协议验证，尚不是完整智能音箱应用。

构建和自动化验证：

```sh
cargo test --workspace --locked
cargo build --workspace --release --locked
target/release/pocket self-test
```

`self-test` 使用原生本地 HTTP／WebSocket 和明确的预制服务，覆盖配对认证、文字重试、语音上传、
朗读分帧、附件和 Ambient 收音／播放／打断；不占用麦克风、不出声、不联系真实房间。

下载安装包后指定独立目录：

```sh
tar -xzf pocket-rs-0.1.1-aarch64-unknown-linux-gnu.tar.gz
cd pocket-rs-0.1.1-aarch64-unknown-linux-gnu
./install.sh --prefix "$HOME/.local/pocket-rs"
"$HOME/.local/pocket-rs/bin/pocket" self-test
"$HOME/.local/pocket-rs/bin/pocket-c-smoke"
```

安装包含动态／静态库、头文件、Swift module map、pkg-config、参考端、C／Swift 使用示例、许可证和
文档。具体依赖、只读系统的安装方式与验证边界见[安装说明](docs/install.zh-CN.md)和
[当前状态](docs/status.zh-CN.md)。

兼容性参考固定版本的 iOS 和 Android。测试向量通过实际执行未修改的上游 Swift 源码生成，并保存源文件
和向量摘要。核心沿用既有协议，也保留 Android 的最终恢复麦克风、取消旧回调和锁屏播放生命周期修复。
版本与后续 APP 接入步骤见[迁移计划](docs/migration.zh-CN.md)。

Android 现已通过 JNI 接入共享协议：Chat 请求、配对、语音分帧、实时事件校验、Ambient 输入／播放
和重试规则实际调用 Rust。UI、网络、数据库事务和硬件生命周期继续由 Android 管理，详见
[Android 接入](docs/android.zh-CN.md)。iOS APP 仍待接入，已提供 C module／XCFramework。
[0.1.1 SDK 预览版](https://github.com/openduo/pocket-rs/releases/tag/v0.1.1)修正 Ambient 媒体下行：
只有上行增加类型字节，接收的 Opus 包保持完整。Android Sight 协商与视频上传已在 Pocket Android
单独通过短验收，相机仍由宿主拥有。Push 继续暂缓；Linux 设备音频驱动、声学 AEC、唤醒词和
免按键交互仍属后续产品接入。代码许可为 [FSL-1.1-Apache-2.0](LICENSE)。

文字、语音消息的 ID 必须是 `8-4-4-4-12` 十六进制 UUID。核心统一校验 Rust、C ABI/JNI 请求构造和 Session 入队/恢复；无效 ID 在产生请求或持久化副作用前返回 `invalid_idempotency_key`。大写十六进制输入可以使用，请求统一小写；重试始终保留同一 ID 和正文。
