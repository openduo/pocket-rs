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
tar -xzf pocket-rs-0.1.0-aarch64-unknown-linux-gnu.tar.gz
cd pocket-rs-0.1.0-aarch64-unknown-linux-gnu
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

本轮提供共享库，并不替换现有 Android／iOS APP。Push、Sight 延续暂缓；硬件驱动、声学 AEC、
唤醒词及完整 APP 集成属于后续工作。代码许可为 [FSL-1.1-Apache-2.0](LICENSE)。
