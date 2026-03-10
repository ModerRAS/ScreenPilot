# ScreenPilot

一款基于 **Rust + Tauri** 构建的局域网数字标牌控制器，使用 **DLNA / UPnP AV** 协议来控制多个显示屏幕。

## 功能特性

- **SSDP 设备发现** — 自动发现局域网中的 DLNA MediaRenderer 设备（每 30 秒自动刷新）
- **DLNA 控制** — 向每个渲染器发送 UPnP AVTransport SOAP 命令（SetAVTransportURI、Play、Pause、Stop）
- **媒体 HTTP 服务器** — 内置 Axum HTTP 服务器，从本地 `media/` 目录提供媒体文件服务
- **独立屏幕控制** — UI 中可独立控制每个设备
- **场景功能** — 将设备→媒体分组，一键应用

## 项目架构

```
ScreenPilot/
├── Cargo.toml                  # 工作空间根目录
├── package.json                # Tauri CLI / 前端依赖
├── src/                        # 前端 (HTML + 原生 JS)
│   ├── index.html
│   ├── main.js
│   └── styles.css
├── src-tauri/                  # Rust / Tauri 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs              # 共享类型 + 导出到 main 的辅助函数
│       ├── main.rs             # Tauri 命令 + 应用入口
│       ├── discovery.rs        # SSDP 设备发现
│       ├── dlna.rs             # UPnP AVTransport SOAP 命令
│       ├── media_server.rs     # Axum 媒体文件服务器
│       └── state.rs            # RendererDevice、Scene、AppState
└── media/                      # 将 .mp4 / .webm 文件放入此处
    └── README.txt
```

## 环境准备

### Linux
```bash
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev
```

### macOS / Windows
请参考 [Tauri 环境准备指南](https://tauri.app/start/prerequisites/)。

## 开发运行

```bash
# 安装 Node 依赖 (Tauri CLI)
npm install

# 开发模式运行 (热重载 UI)
npm run dev

# 或者构建发布版本
npm run build
```

## 使用说明

1. 将媒体文件（`.mp4`、`.webm` 等）放入可执行文件旁边的 `media/` 目录
2. 启动 ScreenPilot
3. 点击 **Discover Devices** 扫描局域网设备
4. 选择一个媒体文件，点击 **▶ Play** 对任何已发现的渲染器进行播放
5. 使用 **Scenes** 功能定义并一键应用多屏幕布局

## Rust 模块说明

| 模块 | 职责 |
|---|---|
| `discovery` | SSDP M-SEARCH，设备描述 XML 解析 |
| `dlna` | SOAP 信封构建，AVTransport HTTP 调用 |
| `media_server` | Axum 静态文件服务器，`local_ip()` 辅助函数 |
| `state` | `RendererDevice`、`Scene`、`AppState`、`PlaybackStatus` |

## 示例 SOAP — Play

```xml
POST /upnp/control/AVTransport HTTP/1.1
Content-Type: text/xml; charset="utf-8"
SOAPAction: "urn:schemas-upnp-org:service:AVTransport:1#Play"

<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <Speed>1</Speed>
    </u:Play>
  </s:Body>
</s:Envelope>
```
