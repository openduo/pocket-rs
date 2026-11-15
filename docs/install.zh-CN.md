# 安装与原生验证

[English](install.md) · 简体中文

ARM64 安装包面向 `aarch64-unknown-linux-gnu`，使用原生 Debian Bookworm 构建，glibc 基线为
2.36。共享协议库不依赖 Go、JVM、OpenSSL、音频服务或 Tailscale；参考端使用 Rust TLS 和已有网络。
此 GNU 安装包不能直接用于 Alpine／musl 或 32 位系统。

下载 `.tar.gz` 与摘要文件，校验后解压并指定独立目录：

```sh
sha256sum -c pocket-rs-0.1.1-aarch64-unknown-linux-gnu.tar.gz.sha256
tar -xzf pocket-rs-0.1.1-aarch64-unknown-linux-gnu.tar.gz
cd pocket-rs-0.1.1-aarch64-unknown-linux-gnu
./install.sh --prefix "$HOME/.local/pocket-rs"
"$HOME/.local/pocket-rs/bin/pocket" self-test
"$HOME/.local/pocket-rs/bin/pocket-c-smoke"
```

安装器检查包内摘要、系统和架构，只写指定前缀。C 示例实际链接安装后的共享库；可通过
`lib/pkgconfig/pocket-core.pc` 编译自己的调用方。原生 ARM64 构建入口为 `scripts/build-linux.sh`，
固定镜像摘要，运行协议契约及本地 HTTP／WebSocket 链路，构建库／参考端／独立 C 程序再打包。

本次物理设备根目录只读，约 485 MiB 内存，验收前缀位于 `/tmp`，**重启后会消失**。安装不要求
设备安装 Rust 工具链、不修改分区／挂载／启动服务，也不采集麦克风。生产部署应选设备已有的持久
应用目录，本轮不擅自修改存储布局。

使用真实房间时，先确保设备网络可达；系统 Tailscale 可以提供该网络。将新配对链接放入私有文件，
执行 `pocket pair --link-file … --config …`；`state`／`history` 为读取，`text`／`voice` 为显式发送。
`retry --id` 使用保存的相同请求，启动和读取不会自动发送积压消息。凭据和待发记录使用私有文件权限。
`speech` 下载长度前缀 Opus，`live` 可读写预制包文件。参考端不占麦克风、不直接播放，也不为下载数据
伪造硬件播放回执；声学驱动按 ABI 约定另行接入。

Apple 入口 `scripts/build-apple.sh` 构建 macOS 库并实际运行 C／Swift 使用方，再生成 iOS 真机和
ARM64 模拟器的静态 XCFramework。Xcode 导入后 `import PocketCore`，保留 Swift 的音频会话、
Keychain 和后台政策。此项证明 SDK 构建与接口可调用，不能作为原 iOS APP 已完成迁移的证据。
