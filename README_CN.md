# AutoMouse

AutoMouse 是一个用 Rust 编写的、面向 ESP32-S3 的极简 USB HID 鼠标固件。

## 功能

- 基于 `embassy-usb` 和 `usbd-hid` 的 USB HID 鼠标设备
- 通过 GPIO 输入实现左键和右键点击
- 按住按钮时产生光标拖拽效果
- 报告描述符和 `MouseReport` 类型与 RMK 键盘固件的鼠标实现兼容

## 硬件要求

- ESP32-S3 开发板
- 连接到板载 USB 引脚的 USB 线（GPIO20 D+ / GPIO19 D-）
- 两个瞬时按钮（可选；使用内部下拉）

## 引脚定义

| 引脚  | 功能           | 说明                 |
|-------|----------------|----------------------|
| GPIO2 | 鼠标左键       | 下拉输入，高电平有效 |
| GPIO4 | 鼠标右键       | 下拉输入，高电平有效 |
| GPIO19| USB D-         | USB OTG 固定功能引脚 |
| GPIO20| USB D+         | USB OTG 固定功能引脚 |

## 构建

本项目目标平台为 `xtensa-esp32s3-none-elf`，使用 `esp` Rust 工具链。

```sh
cargo build
```

## 烧录

`.cargo/config.toml` 中已配置 `espflash` 作为 runner。连接开发板后执行：

```sh
cargo run
```

该命令会构建固件、烧录到开发板，并打开串口监视器。

## 运行行为

开发板上电后会枚举为一个 USB HID 鼠标。按下或释放按钮时，相应的 HID 报告会发送到主机：

- 按下 GPIO2：左键按下
- 释放 GPIO2：左键释放
- 按下 GPIO4：右键按下
- 释放 GPIO4：右键释放
- 按住任意按钮：每 10 毫秒发送一次轻微拖拽报告（x=2, y=2）

HID 鼠标报告中的 `x` 和 `y` 是**相对增量（delta）**，不是绝对坐标。主机会维护光标位置，并把每次收到的增量累加到当前位置上。例如，`x=2` 表示光标向右移动 2 个像素，`x=-3` 表示向左移动 3 个像素。

## 项目结构

```text
src/
├── bin/main.rs    # 应用入口，USB/GPIO 初始化和任务
├── hid/mod.rs     # USB HID 鼠标初始化和按钮常量
└── lib.rs         # 共享的 mk_static! 辅助宏
```

## 依赖

- `esp-hal` 1.x，启用 `esp32s3` 特性
- `embassy-usb` 0.6
- `usbd-hid` 0.10
- `esp-rtos` 0.3，启用 Embassy 集成

## 许可证

Copyright (c) AutoMouse 作者。许可证详情请参见源码仓库。
