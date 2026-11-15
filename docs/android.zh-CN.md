# Android 协议接入

[English](android.md) · 简体中文

Pocket Android 通过 Java／C 的小型 `NativeCore` JNI 桥调用同一套 C ABI v1。仓库用 submodule
固定 `openduo/pocket-rs` 版本；Gradle 分别构建 JVM 宿主库与 Android arm64-v8a／x86_64 库，
支持 API26+ 和 16 KiB ELF 对齐，不把编译后的二进制提交到源码仓库。

| Rust 负责 | Android 接线 |
| --- | --- |
| Endpoint、粘贴配对、请求／响应、query 编码 | `Endpoint`、`ChannelClient.pair` |
| Chat、语音、状态／历史、文件、朗读与 live 请求 | `ChannelClient`、`RoomAccess.speech` |
| 上传回执／摘要校验 | `ChannelClient.upload`；平台计算实际文件字节的摘要 |
| 增量 LE16 Opus 分帧及错误／截断检查 | `PacketStream`，由流线程关闭 |
| ready／row／message 校验及新增事件兼容 | `PocketController.frame` |
| Ambient hello／席位、媒体帧、重叠按压及最终恢复麦克风 | `AmbientInput`、controller 上下行 |
| 播放声明、接收／完成／超时、打断及迟到进度过滤 | `AmbientOutput`、`RoomPlayback` |
| 单调时钟语音预算、live 退避 | `VoiceRetry`、`RedialBackoff`；平台提供时间与调度 |

Java 门面只映射现有领域类型和快照，原来的对应协议状态机已删除。Rust 加载失败不会回退到另一套
Java 实现。错误通过结构化 code 返回，再进入 APP 既有国际化边界。

UI、网络执行、SQLite 事务、凭据、音频焦点、麦克风、编解码、AudioTrack 和前台服务仍由 Android
管理。Android 保留 OutboxSender／RoomLink 的任务／取消／落账顺序，使用细粒度 Rust 所有者，
不会同时驱动另一套 Rust 聚合 Session 重复发消息。新 Linux 宿主可以直接使用 Session 的命令／
副作用，两种入口复用相同核心实现。流所有者在完成／取消时关闭；APP 全局所有者与 controller
同寿命。JNI 将结果复制成 Java 字节数组后，统一释放 Rust 缓冲，错误路径也同样释放。

构建需要 Rust 1.100.0、两个 Android target、JDK17 和固定 NDK `28.2.13676358`。
`scripts/build-android.sh` 只构建 FFI 和 JNI；`python3 scripts/package-android.py` 生成标准
双 ABI AAR、混淆保留规则、许可证和含源码／说明的 SDK 包。普通 Android 工程可用
`implementation files('libs/pocket-core-0.1.0.aar')`，原 Pocket Android 则从固定源码构建。
接口说明见 [ABI](abi.md) 和 [Java 示例](../examples/java/Consumer.java)。

Android 的既有 Go／Tailscale 网络适配器继续提供网络；Rust 协议库没有 Go 依赖。Linux 可以直接
使用设备可达的系统网络，无须带上 Android 的网络适配器。

验收分成实际加载 Rust 的 JVM 契约、E2 上的 JNI／协议检查、Try 内的文字／预制语音／文件／
朗读／取消／重连，以及独立的短锁屏播放。锁屏检查用静音 Opus／AudioTrack 和预制本地源。
本次不占麦克风，不向真实房间／真人会话发测试内容，不测 Sight，也不启动广泛 UI 矩阵。
这些证据证明协议与平台接线，不代替真人声学验收。确切执行结果见[当前状态](status.zh-CN.md)。
