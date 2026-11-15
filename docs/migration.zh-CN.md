# Rust 迁移计划

[English](migration.md) · 简体中文

本轮交付独立的、无 Go 依赖的房间协议库、可安装的共享库及 ARM64 Linux 原生验证，并完成 Android 的协议接入及短真机验收，
为后续 iOS 接入保留稳定接口。界面、音频驱动和 Tailscale 登录继续由各平台提供。

参考源固定为 iOS `b9d91bb9b77a12c3e59e6f359af1e4537a4ded39` 和 Android
`9f59814df246c22c6e01c18b0c150ea1e93f37b0`。保持客户端 API v2、16 kHz 单声道、20 ms
采集帧、Opus 长度前缀、surface 和设备令牌协议；同时保留 Android 新增的取消、最终恢复房间
麦克风及锁屏播放生命周期修复。Rust 版本与 C ABI 版本独立于服务端 API 版本。

按顺序完成以下验收：

1. 协议核心：配对／权限、请求与响应、文字／历史／附件／朗读、语音分帧、实时游标与消息生命周期、
   Ambient 收音席位／静音／打断／播放进度，以及重试与取消。核心不执行网络、系统音频或 UI 操作。
2. 兼容证据：执行未修改的上游 Swift 源码生成固定向量，记录源文件校验值；Rust 消费相同输入输出。
   针对 Android 后续修复补充必要的确定性回归，不扩大设备测试矩阵。
3. 使用入口：纯 Rust HTTP／WebSocket 参考端和版本化 C ABI。Linux 使用进程已有网络；系统 Tailscale
   可以提供连通性，协议库不嵌入节点。C／Swift 接口明确内存、线程、取消及错误归属。
4. ARM64 实机：在原生 aarch64 Linux 构建，将共享库、头文件和参考端安装到独立前缀；在物理 Linux
   设备运行本地 Chat／语音／Ambient 协议链路及独立 C 调用方，核对 ELF、依赖和摘要。设备根目录只读，
   本轮用临时目录验收，不改分区、挂载或开机服务，并明确重启后临时安装会消失。
5. Android 接入：Chat 请求、配对、语音分帧、实时校验、Ambient 输入／播放与重试规则通过 JNI
   实际使用 Rust。保留当前公开包名、签名、配对、Tailscale 节点和数据库；完成相关宿主契约、
   双 ABI 构建与 E2 短联调后，才计为已接入。
6. iOS 准备：macOS 上实际编译运行 Swift 调用方；有 SDK 时构建 iOS 静态库，提供后续集成方式。
   这些证据与 iOS APP 实际迁移分开记录。
7. 发布：格式／lint／测试通过，完成打包、校验值及含原生 ARM64 的 CI，再在 `openduo/pocket-rs`
   发布源码和版本资产。

Android 已通过 JNI 替换上述协议所有者，保留界面、硬件、凭据、数据库事务、应用任务调度和前台服务。
Android 使用细粒度所有者，新的 Linux 宿主可使用聚合 Session，禁止同一链路同时驱动两者；见
[Android 接入表](android.zh-CN.md)。iOS
通过 C module／静态库／XCFramework 接入，保留 SwiftUI、AVAudioSession／Engine、Keychain
及后台政策。任何阶段每个会话行为都只有一个实际执行者，避免新旧核心重复发消息或同时占用麦克风。

本轮范围包括房间 Chat／语音／Ambient 及相邻数据协议。媒体下行修正支持 Android 恢复后的 Sight
连接，相机采集／上传仍在 Pocket Android 单独实现与验收。Push 继续暂缓；Passport BLE、地理
监控、Markdown／界面状态、完整 APP 数据库迁移、唤醒词、设备驱动与声学 AEC 留在后续迁移清单。
具体实现和实际验收分开记录，不能将参考端本地协议通过写成真实房间／人声验收。
