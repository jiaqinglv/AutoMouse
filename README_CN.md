# AutoMouse

AutoMouse 是一个用 Rust 编写的、面向 ESP32-S3 的极简 `#![no_std]` USB HID 鼠标固件。

它把开发板枚举为一个 USB 鼠标，并由两个相互独立的输入源驱动光标：物理 GPIO 按钮，以及通过 Wi-Fi 收到的 MQTT 消息。两者都汇入同一个共享通道，由 HID 写入任务消费。

## 功能

- 基于 `embassy-usb` 和 `usbd-hid` 的 USB HID 鼠标设备
- 通过 GPIO 输入实现左键和右键点击
- 按住按钮时产生光标拖拽效果
- 通过 Wi-Fi 远程控制：订阅一个 MQTT 主题，并把收到的鼠标报告注入 HID
- `TIMG1` 看门狗，固件卡死时复位芯片
- 报告描述符和 `MouseReport` 类型与 RMK 键盘固件的鼠标实现兼容

## 硬件要求

- ESP32-S3 开发板
- 连接到板载 USB 引脚的 USB 线（GPIO20 D+ / GPIO19 D-）
- 两个瞬时按钮（可选；使用内部下拉）
- 一个可连接的 Wi-Fi 热点，以及同一网络内可达的 MQTT broker

## 引脚定义

| 引脚  | 功能           | 说明                 |
|-------|----------------|----------------------|
| GPIO2 | 鼠标左键       | 下拉输入，高电平有效 |
| GPIO4 | 鼠标右键       | 下拉输入，高电平有效 |
| GPIO19| USB D-         | USB OTG 固定功能引脚 |
| GPIO20| USB D+         | USB OTG 固定功能引脚 |

## 配置

配置都是编译期的，位于源码树内。构建期和运行期都不会读取环境变量。

Wi-Fi 凭据被放在一个 git 忽略的文件里，不进仓库，因此首次构建前需要先创建它：

```sh
cp src/wifi_credentials.rs.example src/wifi_credentials.rs
```

然后填入你自己的值：

```rust
const WIFI_SSID: &str = "your-ssid";
const WIFI_PASSWORD: &str = "your-password";
```

`src/wifi.rs` 通过 `include!` 引入该文件，因此凭据在编译期就被写入二进制——运行期不会读取文件，也没有解析步骤。由于该文件被 git 忽略，新克隆的仓库在创建它之前无法构建；文件缺失时 `build.rs` 会提前失败并给出提示。

其余配置项随仓库提交，直接修改即可：

| 配置项 | 位置 |
|--------|------|
| `MQTT_BROKER`、`MQTT_PORT`、`MQTT_CLIENT_ID`、`MQTT_TOPIC` | `src/mqtt.rs` |
| USB VID/PID 及描述符字符串 | `src/hid/mod.rs` |
| 堆分配器大小 | `src/bin/main.rs` |

注意：

- `MQTT_BROKER` 按 IPv4 地址解析，不是主机名。
- MQTT 客户端走明文 TCP：没有 TLS，也没有鉴权。
- `MQTT_CLIENT_ID` 是固定的，因此两块板子连同一个 broker 会争抢同一个会话。

绝不要提交 `src/wifi_credentials.rs`。一旦泄露，该值会一直留在 git 历史里，除非重写整个历史，因此唯一可靠的补救方式是在路由器上更换凭据。

## 构建

本项目目标平台为 `xtensa-esp32s3-none-elf`，需要 `esp` Rust 工具链（不是 stable）。`rust-toolchain.toml` 固定了工具链通道，`.cargo/config.toml` 指定了目标平台并启用了 `build-std = ["alloc", "core"]`。

```sh
cargo build          # dev 配置
cargo build --release
cargo clippy
```

两种配置都使用 `opt-level = "s"`；二进制体积会影响分区布局，因此不要随意改动。

## 烧录

`.cargo/config.toml` 中已配置 `espflash` 作为 runner。连接开发板后执行：

```sh
cargo run
```

该命令会构建固件、烧录到开发板，并打开串口监视器。日志级别由 `.cargo/config.toml` 中的 `ESP_LOG=info` 控制。

## 运行行为

开发板上电后会枚举为一个 USB HID 鼠标。按下或释放按钮时，相应的 HID 报告会发送到主机：

- 按下 GPIO2：左键按下
- 释放 GPIO2：左键释放
- 按下 GPIO4：右键按下
- 释放 GPIO4：右键释放
- 按住任意按钮：每 10 毫秒发送一次轻微拖拽报告（x=2, y=2）
- 释放：发送一次无按键、位移为零的报告，用于停止拖拽

HID 鼠标报告中的 `x` 和 `y` 是**相对增量（delta）**，不是绝对坐标。主机会维护光标位置，并把每次收到的增量累加到当前位置上。例如，`x=2` 表示光标向右移动 2 个像素，`x=-3` 表示向左移动 3 个像素。

## MQTT 协议

连接 Wi-Fi 并获取到 DHCP 租约后，固件会订阅所配置的主题，并把该主题上的每条消息都当作一次鼠标报告。载荷为 13 字节，小端序：

| 偏移 | 类型 | 字段 |
|------|------|------|
| 0..8 | `u64` 小端 | `msg_id` —— 关联用 ID，固件不做解释 |
| 8 | `u8` | `buttons` 位掩码 |
| 9 | `i8` | `x` 增量 |
| 10 | `i8` | `y` 增量 |
| 11 | `i8` | `wheel` —— 正数向上滚动 |
| 12 | `i8` | `pan` —— 正数向右滚动 |

`buttons` 位掩码定义如下：

| 位 | 值 | 按键 |
|----|-----|------|
| 0 | `0x01` | 左键 |
| 1 | `0x02` | 右键 |
| 2 | `0x04` | 中键 |

任何连接或订阅失败都会在 10 秒延迟后重试。

## 架构

所有输入源都把 `MouseReport` 送入同一个通道，而不是直接操作 HID 写入器：

```text
GPIO 按钮 ─┐
           ├─► hid::MOUSE_CHANNEL ─► mouse_task ─► HidWriter ─► USB
MQTT 消息 ─┘
```

`hid::MOUSE_CHANNEL`（`src/hid/mod.rs`）是集成点。USB HID 写入器只由 `mouse_task` 独占；新增输入源时必须向该通道发送，而不是绕过它。

由 `main` 派生的 Embassy 任务：

| 任务 | 职责 |
|------|------|
| `watchdog_task` | 每 500 毫秒喂一次 `TIMG1` 看门狗（超时 1 秒，超时后复位芯片） |
| `usb_task` | 在整个程序生命周期内驱动 `embassy-usb` 设备栈 |
| `mouse_task` | `MOUSE_CHANNEL` 的唯一消费者，把报告写入 HID 端点 |
| `button_task` | 每 10 毫秒轮询 GPIO2/GPIO4；状态变化时发报告，按住时持续发拖拽报告 |
| `wifi::net_task` | 运行 `embassy-net` runner，网络栈才能推进 |
| `wifi::wifi_task` | 扫描、连接、等待 DHCP，然后派生 `mqtt::mqtt_task` |

## 项目结构

```text
src/
├── bin/main.rs    # 应用入口，外设初始化和任务派生
├── hid/mod.rs     # USB HID 鼠标初始化、MOUSE_CHANNEL 和按钮常量
├── wifi.rs        # Wi-Fi 扫描/连接与 DHCP，之后派生 MQTT 任务
├── wifi_credentials.rs.example  # git 忽略的凭据文件模板
├── mqtt.rs        # 订阅鼠标主题的 MQTT v5 客户端
└── lib.rs         # 模块重新导出和 mk_static! 辅助宏
```

## 依赖

- `esp-hal` 1.1，启用 `esp32s3`、`log-04`、`unstable` 特性
- `esp-rtos` 0.3，启用 Embassy 集成
- `esp-radio` 0.18（Wi-Fi + BLE 共存）
- `embassy-usb` 0.6 和 `usbd-hid` 0.10
- `embassy-net` 0.9（DHCP/TCP），底层为 `smoltcp` 0.13
- `rust-mqtt` 0.5（MQTT v5，启用 `bump` 特性）

`bleps` 固定到了某个 git 版本，但 BLE 目前在 `main.rs` 中处于注释状态。

## 许可证

Copyright (c) AutoMouse 作者。许可证详情请参见源码仓库。
