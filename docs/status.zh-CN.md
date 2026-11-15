# 验证状态

[English](status.md) · 简体中文

更新于 2026-11-15。0.1.0 底层房间协议和共享 SDK 已通过本地原生验收，源码／发布位于
`openduo/pocket-rs`。Android／iOS APP 接入属于后续工作。

已实现配对／凭据、绑定 origin 的 API 请求、状态／历史／消息／附件／朗读协议、有限增量 Opus
分帧、实时游标和消息生命周期、Ambient 席位／hello／按键静音／播放／打断、持久化后发送、
幂等重试／取消、纯 Rust 网络参考端及 C ABI v1。

上游兼容向量来自实际执行固定版本的原始 Swift：15 个配对、4 个语音包、8 个 surface、10 个退避
步骤及 11 组 Ambient 场景的 110 步。另有针对存储失败、迟到回调、固定请求重试、期限、恢复麦克风
和接收／播放排空的确定性回归。

已实际完成的验证：

| 项目 | 证据 |
| --- | --- |
| Rust 自动化 | macOS ARM64 和原生 Linux ARM64 的 17 个测试通过，含上游向量和真实本地网络链路；格式及 Clippy 无警告 |
| iOS 兼容 | 执行固定版本未修改的 Swift 源码重新生成向量，与提交版本逐字节一致 |
| Linux 原生构建 | 固定 Debian Bookworm Rust 镜像摘要，实际 aarch64 环境，构建动态／静态库和参考端 |
| 物理设备安装 | Ubuntu 26.04.1 aarch64、glibc 2.43、约 485 MiB 内存、根目录只读；安装到 `/var/tmp/pocket-rs-0.1.0` |
| 物理设备协议运行 | 安装后的参考端通过配对认证、文字幂等重试、语音上传、朗读分帧、附件及 Ambient 播放／打断等 13 项本地链路领域 |
| 共享库实际加载 | C 调用方 54 项断言通过；再用设备自己的 GCC 15.2 编译分发源码，链接安装后的 `.so`，同样通过 |
| 实际依赖 | ELF64 AArch64 共享库，仅依赖 libc／libgcc_s 和系统加载器，无 Go／JVM／Tailscale 运行时 |
| Swift 调用 | macOS ARM64 实际导入安装后的 C module，完成配对及会话创建／连接／关闭和缓冲释放 |
| iOS 接入准备 | 构建 iOS 真机及 ARM64 模拟器静态库并合成 XCFramework；不等于原 iOS APP 已迁移 |

共享库 SHA-256：`60edf68be47a908be596d9a6c456ed8b242c28e04bfd853d023414e4870014c3`。
设备原始日志仅留 ignored 目录。测试使用预制 Opus 和明确模拟的硬件完成事件，不采集麦克风、不出声、
不向真实房间发送。安装位于 tmpfs，**重启后消失**。

CI 配置包含 x86_64 Linux、原生 ARM64 Linux 和 ARM64 macOS 的相同契约／安装调用，以及
Swift 向量重生成和 XCFramework 构建；远端执行结果与上述本地／实机证据分开记录。

仍需后续接入的范围：

- Android／iOS APP 保留现有引擎，JNI／Swift 业务接入、数据库迁移及真实后台／音频验收以后完成。
- Linux 参考端使用预编码包文件；麦克风／扬声器驱动、重采样、设备 Opus 编解码、AEC、唤醒词和
  免按键交互属于平台接入。协议验收不等于真实房间识别和出声验收。
- Push、Sight 延续暂缓；Passport BLE、地理监控、Markdown／UI 等不在本轮库中。未知的新增事件
  会留给宿主处理。
- C／Swift 示例和 XCFramework 构建用于证明接口可用，不代表 iOS APP 已迁移。
- Linux 二进制首先支持 ARM64 GNU；其他 ABI／系统需要单独构建验证。物理设备临时安装重启后消失。

所有边界及后续迁移见[迁移计划](migration.zh-CN.md)，安装步骤见[安装说明](install.zh-CN.md)。
