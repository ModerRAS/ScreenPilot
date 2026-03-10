# ScreenPilot

一款基于 **Rust + Axum**（后端）和 **Vue 3 + ElementUI**（前端）构建的局域网数字标牌控制器，使用 **DLNA / UPnP AV** 协议来控制多个显示屏幕。

## 功能特性

- **SSDP 设备发现** — 自动发现局域网中的 DLNA MediaRenderer 设备（手动触发）
- **DLNA 控制** — 向每个渲染器发送 UPnP AVTransport SOAP 命令（SetAVTransportURI、Play、Pause、Stop）
- **媒体 HTTP 服务器** — 内置 Axum HTTP 服务器，从本地 `media/` 目录提供媒体文件服务（端口 8090）
- **独立屏幕控制** — UI 中可独立控制每个设备
- **场景功能** — 将设备→媒体分组，一键应用

## 项目架构

```
ScreenPilot/
├── Cargo.toml                  # 工作空间根目录
├── package.json                # 根包（pnpm workspace）
├── backend/                     # Rust + Axum API 服务器
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # Axum 路由 + 应用入口
│       ├── discovery.rs         # SSDP 设备发现
│       ├── dlna.rs             # UPnP AVTransport SOAP 命令
│       ├── media_server.rs     # Axum 媒体文件服务器
│       └── state.rs            # RendererDevice、Scene、AppState
├── frontend/                   # Vue 3 + ElementUI + Vite + pnpm
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── main.ts            # Vue 入口
│       ├── App.vue            # 根组件
│       ├── api/               # API 客户端 (axios)
│       ├── views/              # DevicesView、ScenesView
│       ├── stores/             # Pinia 状态管理
│       └── types/              # TypeScript 类型定义
└── media/                     # 将 .mp4 / .webm 文件放入此处
```

## 环境准备

- **Rust** — 通过 [rustup](https://rustup.rs/) 安装
- **Node.js** — v20+
- **pnpm** — 通过 `npm install -g pnpm` 安装

## 开发运行

```bash
# 终端 1: 先构建前端
cd frontend && pnpm build

# 终端 2: 启动后端（同时提供 API 和前端，端口 8080）
cd backend && cargo run
```

然后在浏览器中打开 http://localhost:8080/web

## 构建

```bash
# 构建前端
cd frontend && pnpm build

# 构建后端
cd backend && cargo build --release

# 或使用 pnpm workspace 构建两者
pnpm --filter frontend build
pnpm --filter backend build
```

## 生产部署

```bash
# 运行后端
./backend/target/release/screen-pilot-backend

# 提供前端 dist（可使用 nginx，或使用 Vite preview）
cd frontend && pnpm preview
```

## 使用说明

1. 将媒体文件（`.mp4`、`.webm` 等）放入 `media/` 目录
2. 启动后端：`cargo run --manifest-path backend/Cargo.toml`
3. 启动前端：`pnpm --dir frontend dev`
4. 打开 http://localhost:5173
5. 点击 **Discover Devices** 扫描局域网设备
6. 选择一个媒体文件，点击 **▶ Play** 对任何已发现的渲染器进行播放
7. 使用 **Scenes** 功能定义并一键应用多屏幕布局

## API 接口

| 方法 | 端点 | 描述 |
|------|------|------|
| GET | `/api/devices` | 获取已发现设备 |
| POST所有 | `/api/devices/discover` | 触发 SSDP 发现 |
| POST | `/api/devices/:uuid/play` | 在设备上播放媒体 |
| POST | `/api/devices/:uuid/pause` | 暂停播放 |
| POST | `/api/devices/:uuid/stop` | 停止播放 |
| GET | `/api/media` | 获取媒体文件列表 |
| GET | `/api/scenes` | 获取已保存场景 |
| POST | `/api/scenes` | 保存场景 |
| DELETE | `/api/scenes/:name` | 删除场景 |
| POST | `/api/scenes/:name/apply` | 应用场景到设备 |
| GET | `/api/config/media-server-url` | 获取媒体服务器 URL |

## Rust 模块说明（后端）

| 模块 | 职责 |
|------|------|
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
