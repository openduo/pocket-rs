# 验证状态

[English](status.md) · 简体中文

更新于 2026-11-15。[0.1.1 SDK 预览版](https://github.com/openduo/pocket-rs/releases/tag/v0.1.1)
已重新通过原生安装与调用方验收，源码／发布位于
`openduo/pocket-rs`。Android 已接入共享协议，iOS APP 接入仍属后续工作。

后续 Android Sight 适配对照 ambient `0.4.0-rc.2`，发现媒体接收规则偏差：只有上行增加类型字节，
下行始终是完整 Opus 包。Ambient 与细粒度 JNI 输入现共用正确下行规则，本机网络测试服务也改为
真实下行。新增两个测试覆盖 typed owner／JNI、首字节为 `0x01` 的包及长度边界。macOS ARM64
的 21 个 workspace 测试及无警告 Clippy 通过；Android 实际 JNI 共 2,634 条断言通过，更新的 E2 APK
另通过 14 条真实 Sight 协商／上传检查。这项修正没有把相机所有权加入 SDK，C ABI v1 不变。
全部二进制目标已重建为 0.1.1，下表记录本次 Linux／macOS／iOS 的重新验证。

已实现配对／凭据、绑定 origin 的 API 请求、状态／历史／消息／附件／朗读协议、有限增量 Opus
分帧、实时游标和消息生命周期、Ambient 席位／hello／按键静音／播放／打断、持久化后发送、
幂等重试／取消、纯 Rust 网络参考端及 C ABI v1。

上游兼容向量来自实际执行固定版本的原始 Swift：15 个配对、4 个语音包、8 个 surface、10 个退避
步骤及 11 组 Ambient 场景的 110 步。另有针对存储失败、迟到回调、固定请求重试、期限、恢复麦克风
和接收／播放排空的确定性回归。

Android 已实际接入 JNI 共享协议，原对应 Java 状态机已替换。加载真实 Rust 库的 2,622 条既有
JVM 断言通过，双 ABI Release APK 已覆盖安装到 E2 的当前公开包中，未卸载／清数据／重新配对，
也未新增 Tailscale 节点。16 条 Rust／ART 断言和 22 条 Try 应用联调断言通过；另有 15 条短锁屏
检查通过，覆盖静音原生 Opus／AudioTrack、接收／排空、服务／唤醒锁释放与设置恢复。原真实房间
恢复 Online。本次不占麦克风、不向真实房间发送测试内容。所有权见[Android 接入](android.zh-CN.md)。
这些初次接入探针使用此前本地包；Android Preview 3 固定此 SDK，包含接入及媒体下行修正。
14 条 Sight 检查单独记录：实际麦克风采集只提供发送时钟，传输前替换为静音。

已实际完成的验证：

| 项目 | 证据 |
| --- | --- |
| Rust 自动化 | macOS ARM64 和原生 Linux ARM64 的 21 个测试通过，含上游向量、媒体下行回归和真实本地网络链路；格式及 Clippy 无警告 |
| iOS 兼容 | 0.1.0 已执行固定版本未修改的 Swift 并核对向量逐字节一致；0.1.1 运行未变的向量，CI 重新生成 |
| Linux 原生构建 | 固定 Debian Bookworm Rust 镜像摘要，实际 aarch64 环境，构建动态／静态库和参考端 |
| 物理设备安装 | Ubuntu 26.04.1 aarch64、glibc 2.43、约 485 MiB 内存、根目录只读；0.1.1 安装到 `/var/tmp/pocket-rs-0.1.1` |
| 物理设备协议运行 | 安装后的参考端通过配对认证、文字幂等重试、语音上传、朗读分帧、附件及 Ambient 播放／打断等 13 项本地链路领域 |
| 共享库实际加载 | C 调用方 54 项断言通过；再用设备自己的 GCC 15.2 编译分发源码，链接安装后的 `.so`，同样通过 |
| 实际依赖 | ELF64 AArch64 共享库，仅依赖 libc／libgcc_s 和系统加载器，无 Go／JVM／Tailscale 运行时 |
| Swift 调用 | macOS ARM64 实际导入安装后的 C module，完成配对及会话创建／连接／关闭和缓冲释放 |
| Java 调用 | macOS 实际执行宿主 JNI 的二进制分帧、finish、结果复制、错误输入与失效句柄检查 |
| iOS 接入准备 | 构建 iOS 真机及 ARM64 模拟器静态库并合成 XCFramework；不等于原 iOS APP 已迁移 |

共享库 SHA-256：`9026cb903cded3a8a712e5c0fd3c41a9efde03c18eae3d03e174a2aa99721467`。
设备原始日志仅留 ignored 目录。Linux 测试使用预制 Opus 和明确模拟的硬件完成事件；Android 使用
Try 预制内容和静音原生解码／播放。均不采集麦克风、不出声、不向真实房间发送。安装位于 tmpfs，**重启后消失**。

CI 配置包含 x86_64 Linux、原生 ARM64 Linux 和 ARM64 macOS 的相同契约／安装调用，以及
Swift 向量重生成和 XCFramework 构建，以及 Android JNI／AAR。
此前 [SDK 源码 16c43fa 的 CI](https://github.com/openduo/pocket-rs/actions/runs/43675723689) 与
[Android 接入 4208864 的 CI](https://github.com/openduo/pocket-android/actions/runs/43675746653) 均通过。
0.1.1 发布说明链接本次 SDK 对应 CI 及发布校验文件。

仍需后续接入的范围：

- Android 已通过 JNI 使用 Rust 的请求／配对／分帧／实时校验／Ambient／重试所有者，UI、SQLite
  事务、凭据、网络、音频资源和任务生命周期留在平台层。不会并行启动 Rust 聚合 Session。
  SessionViewer 的会话 watch／通知命令序列化及 Passport／Sight／地理相关协议仍保留既有实现，
  不在这次 Chat／Ambient 接入验收范围。完整数据库／产品状态迁移和 iOS APP 接入仍待后续推进。
- Linux 参考端使用预编码包文件；麦克风／扬声器驱动、重采样、设备 Opus 编解码、AEC、唤醒词和
  免按键交互属于平台接入。协议验收不等于真实房间识别和出声验收。
- Push 继续暂缓；Android Sight 协商／上传已在 Pocket Android 单独短验收。相机采集／上传、
  Passport BLE、地理监控、Markdown／UI 仍由宿主拥有，不属于 SDK。未知的新增事件会留给宿主处理。
- C／Swift 示例和 XCFramework 构建用于证明接口可用，不代表 iOS APP 已迁移。
- Linux 二进制首先支持 ARM64 GNU；其他 ABI／系统需要单独构建验证。物理设备临时安装重启后消失。

所有边界及后续迁移见[迁移计划](migration.zh-CN.md)，安装步骤见[安装说明](install.zh-CN.md)。
