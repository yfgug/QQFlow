# QQFlow-Rust

QQ 聊天记录本地解密导出工具。基于 **Tauri 2 + React + Rust** 构建，是原 Electron + Python 版本的 Rust 重写版本。

**所有数据仅在本地处理，绝不上传至任何服务器。**

## 简介

QQFlow 是一款用于导出 QQ NT（新版 QQ）聊天记录的桌面工具。QQ NT 基于 Electron 架构，聊天数据存储在本地的 SQLCipher 加密数据库中。本工具通过调试 API 自动提取加密密钥，解密数据库后将聊天记录导出为 TXT / CSV 文件，并提供消息统计分析功能。

### 与原版对比

| 特性 | 原版 (Electron + Python) | Rust 版 (Tauri) |
|---|---|---|
| 桌面框架 | Electron 33 | Tauri 2 |
| 后端语言 | Python + Flask | Rust (原生) |
| 数据库解密 | sqlcipher3 (Python) | rusqlite + bundled-sqlcipher |
| 密钥提取 | PowerShell + C# (P/Invoke) | Rust (windows crate) |
| 安装包大小 | ~150MB (含 Python runtime) | ~10MB |
| 内存占用 | ~200MB | ~30MB |

### 技术栈

**前端：** React 18 + TypeScript + Vite + SCSS + ECharts + Lucide Icons
**后端：** Rust + Tauri 2 + rusqlite (SQLCipher) + windows crate
**打包：** Tauri CLI (MSI / NSIS)

### 灵感来源

本项目的 MessageStore 缓存架构和导出流程参考了 [WeFlow](https://github.com/hicccc77/WeFlow)（微信聊天记录导出工具），在此感谢 WeFlow 作者的开源贡献。

## 功能

- **密钥提取** — 通过 Windows Debug API 注入 QQ 进程，自动提取 SQLCipher 数据库加密密钥
- **数据库扫描** — 自动扫描本机所有 QQ 账号的数据库文件
- **聊天解密** — 解密 SQLCipher 加密的聊天数据库（流式复制 + 磁盘缓存）
- **TXT 导出** — 群聊/私聊记录导出为可读 TXT，支持选择性导出指定群聊
- **CSV 导出** — 导出为 CSV 表格（Excel 可打开），支持选择性导出指定群聊
- **聊天分析** — 群列表/联系人列表 → 点击进入详细分析（24h分布、类型饼图、成员排行、高频短语），支持搜索过滤
- **密钥持久化** — XOR 混淆 + Base64 编码保存密钥，支持多 QQ 账号，避免重复提取
- **主题切换** — 深色 / 浅色主题

## 工作原理

```
QQ 进程 (qq.exe)
    │
    ▼  Windows Debug API → 提取 SQLCipher 密钥
密钥 (16字节 ASCII)
    │
    ▼  std::io::copy 流式复制 (跳过 1024B 头部) → 磁盘缓存 (%TEMP%/qqflow_cache/)
解密后的数据库副本 (仅首次复制)
    │
    ▼  SELECT * 全表扫描 → Rust HashMap<String, Vec<Msg>>
MessageStore (内存常驻, O(1) 查找)
    │
    ▼  Tauri async 命令 + tokio::spawn_blocking
群聊分析 / 私聊分析 / TXT导出 / CSV导出
    │
    ▼  文本纯净度过滤 (排除 Protobuf varint 伪装的生僻字)
消息内容展示 (ECharts 图表)
```

## 环境要求

**运行时（用户）：**
- Windows 10/11（64 位）
- QQ NT（新版 QQ，基于 Electron 架构）

**开发 / 构建：**
- Rust 1.75+
- Node.js 18+
- Strawberry Perl（编译 OpenSSL 依赖时需要）
- Visual Studio Build Tools（C++ 桌面开发工作负载）

## 开发

```bash
# 安装前端依赖
npm install

# 启动开发模式（热重载）
npx tauri dev
```

## 构建

构建时需要编译 OpenSSL（rusqlite 的 SQLCipher 依赖），要求系统中有 Strawberry Perl。

```bash
# 将 Strawberry Perl 加入 PATH（Git Bash）
PATH="/c/Strawberry/perl/bin:$PATH"

# 构建
npx tauri build
```

输出位于 `src-tauri/target/release/bundle/`：

| 格式 | 路径 | 说明 |
|---|---|---|
| MSI | `msi/QQFlow_1.1.5_x64_en-US.msi` | Windows Installer，适合企业部署 |
| NSIS | `nsis/QQFlow_1.1.5_x64-setup.exe` | 安装向导，适合普通用户 |

## 项目结构

```
qqflow-rust/
├── src-tauri/                      # Rust 后端 (Tauri)
│   ├── Cargo.toml                  # Rust 依赖配置
│   ├── tauri.conf.json             # Tauri 应用配置
│   ├── capabilities/default.json   # Tauri v2 权限配置
│   ├── build.rs                    # Tauri 构建脚本
│   └── src/
│       ├── main.rs                 # Tauri 入口，注册命令
│       ├── commands.rs             # Tauri 命令处理器（前端调用的 API）
│       ├── db_scan.rs              # QQ 数据库文件扫描
│       ├── export_chat.rs          # 数据库解密与聊天记录导出
│       ├── analysis.rs             # 聊天统计分析
│       └── message_parser.rs       # QQ 消息 BLOB 二进制解析
│
├── src/                            # React 前端
│   ├── main.tsx                    # 应用入口
│   ├── App.tsx / App.scss          # 根组件与全局样式
│   ├── lib/api.ts                  # Tauri invoke 封装层
│   ├── stores/
│   │   ├── appStore.tsx            # 全局状态（密钥、数据库、导出进度）
│   │   └── themeStore.tsx          # 主题状态（深色/浅色）
│   ├── components/
│   │   ├── TitleBar.tsx / .scss    # 自定义标题栏（无边框窗口）
│   │   └── Sidebar.tsx / .scss     # 侧边导航栏
│   ├── pages/
│   │   ├── HomePage.tsx / .scss    # 首页
│   │   ├── KeyExtractPage.tsx      # 密钥提取页
│   │   ├── DatabasePage.tsx        # 数据库选择页
│   │   ├── ExportPage.tsx          # 导出页
│   │   ├── AnalysisPage.tsx        # 分析主页
│   │   ├── GroupAnalysisTab.tsx    # 群聊分析标签
│   │   ├── PrivateAnalysisTab.tsx  # 私聊分析标签
│   │   └── SettingsPage.tsx        # 设置页
│   └── styles/main.scss            # 全局 SCSS 变量与基础样式
│
├── package.json
├── vite.config.ts
├── tsconfig.json
└── index.html
```

## 已知问题

### 首次打开数据库需一次性全表扫描（MessageStore 加载）

QQ 的 `group_msg_table` 和 `c2c_msg_table` 在过滤列上**没有索引**。本工具采用 **MessageStore** 方案：首次访问数据库时，执行一次流式 `SELECT *` 全表扫描，将全部消息按群号/对端 UID 分组写入 Rust `HashMap`。此后所有查询直接走内存（O(1) 查找），不再访问 SQLite。

- 190MB 数据库首次加载约 30-60 秒（含 BLOB 解密和解析），有进度日志反馈。缓存常驻内存约 200-400MB，后续查询均为毫秒级
- 加载完成后的群列表、分析、导出均为毫秒级，无额外等待
- MessageStore 在应用生命周期内常驻内存，切换页面不会丢失

**调试方法**：桌面 `qqflow_debug.txt` 记录每步耗时。

## 常见问题

### OpenSSL 编译失败：`Can't locate Locale/Maketext/Simple.pm`

Git Bash 自带的 MSYS2 Perl 缺少编译 OpenSSL 所需的模块。解决方案：

1. 安装 [Strawberry Perl](http://strawberryperl.com/)（自带所有必要模块）
2. 构建时将其加入 PATH：`PATH="/c/Strawberry/perl/bin:$PATH"`

项目中的 `perl5lib/` 目录包含部分补丁模块，但不足以完整编译 OpenSSL，推荐直接使用 Strawberry Perl。

### TypeScript 编译错误：`'>' expected` in `.ts` 文件

如果 `.ts` 文件中包含 JSX 语法（如 `<Component />`），TypeScript 会报错。将文件扩展名改为 `.tsx` 即可。

### `CreateProcessA` 找不到

`windows` crate 中 `CreateProcessA` 需要 `Win32_Security` feature。确保 `Cargo.toml` 中已添加：

```toml
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_Security",   # ← 必须
    "Win32_System_Threading",
    ...
] }
```

### `IsWindowVisible` / `HWND` / `BOOL` 未找到

`windows` 0.58 中这些类型需要在模块级导入，不能仅在函数内部的 `use` 块中导入。确保在文件顶部有：

```rust
#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
```

### `NaiveTime` 没有 `hour()` 方法

`chrono` 的 `hour()` 方法来自 `Timelike` trait，需要显式导入：

```rust
use chrono::{Datelike, Timelike, NaiveDateTime};
```

### NSIS / WiX 下载超时

打包时 Tauri 需要从 GitHub 下载 WiX 和 NSIS 工具链。如果网络不通，设置代理：

```bash
HTTP_PROXY="http://127.0.0.1:端口" HTTPS_PROXY="http://127.0.0.1:端口" npx tauri build
```

## 更新记录

### v1.1.5（当前版本）

首个正式版本。包含完整的密钥提取、数据库解密、聊天导出和统计分析功能。

**核心功能**
- 密钥提取：通过 Windows Debug API 自动提取 SQLCipher 数据库加密密钥
- 数据库解密：流式复制 + 磁盘缓存，支持大数据库（190MB+）
- 聊天导出：TXT / CSV 格式，支持选择性导出指定群聊或私聊
- 聊天分析：24h 分布、类型饼图、成员排行、高频短语
- 多账号支持：每个 QQ 号独立缓存，密钥独立存储

**架构设计**
- MessageStore 内存缓存：首次 `SELECT *` 全表扫描 → `HashMap`，后续查询 O(1)
- 异步命令架构：Tauri async 命令 + `tokio::spawn_blocking`，不阻塞 UI
- 操作预算：BLOB 解析器内置操作计数器，防止极端慢路径
- UID 映射 4 级策略：已知列名 → 自动检测 → 模糊搜索 → 暴力扫描

**灵感来源**
- MessageStore 缓存架构和导出流程参考了 [WeFlow](https://github.com/hicccc77/WeFlow)（微信聊天记录导出工具）

## 安全说明

- 所有数据处理均在本地完成，不联网
- 密钥通过 XOR 混淆 + Base64 编码后以 JSON 格式存储在本地配置文件中（支持多账号）
- SQLCipher 解密使用 `bundled-sqlcipher-vendored-openssl`，加密库静态编译进二进制文件
- 应用不请求任何网络权限

## 风险与免责声明

**本工具仅供个人学习、研究和数据备份用途。使用者应自行承担使用本工具所产生的一切后果。**

### 法律风险
- 本工具用于解密和导出 QQ 聊天记录，可能涉及他人隐私数据。**使用前请确保您有权访问和处理相关数据**
- 在中国大陆地区，请遵守《中华人民共和国网络安全法》《中华人民共和国个人信息保护法》等相关法律法规
- **严禁**将本工具用于任何非法用途，包括但不限于：窃取他人聊天记录、侵犯他人隐私、传播他人个人信息等
- 导出的聊天记录可能包含他人的个人信息（昵称、头像、发言内容等），请妥善保管，**未经授权不得传播或公开**

### 技术风险
- 本工具通过 Windows Debug API 注入 QQ 进程提取加密密钥，**可能触发安全软件报警**（如杀毒软件、EDR）
- QQ 版本更新后，数据库结构、加密方式或进程行为可能发生变化，导致本工具**失效或产生不可预期的结果**
- 密钥提取过程中会临时启动并调试 QQ 进程，**请确保 QQ 已关闭**，否则可能导致 QQ 异常
- 解密后的数据库缓存存储在系统临时目录（`%TEMP%/qqflow_cache/`），使用完毕后**建议手动清理**
- 本工具不保证导出数据的完整性和准确性，部分消息类型（如图片、视频、文件）仅导出占位符

### 数据安全
- 密钥以 XOR 混淆形式存储在本地配置文件中，**并非安全加密**，仅作简单混淆
- 本工具**不会**将任何数据上传至网络，但请确保您的电脑本身是安全的
- 导出的 TXT/CSV 文件以明文存储，请注意文件的访问权限和存储安全
- **建议**：导出完成后删除临时缓存文件，妥善保管导出文件

### 免责声明
- 本工具按"现状"提供，不作任何明示或暗示的保证
- 作者不对因使用本工具而产生的任何直接、间接、偶然、特殊或后果性损害负责
- 使用本工具即表示您已阅读、理解并同意上述条款
- **如有疑问，请在使用前咨询法律专业人士**
