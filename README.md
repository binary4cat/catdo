# catdo

`catdo` 是一个本地知识库工具：使用 Rust 提供本地服务，使用 Svelte 5 构建界面，把 Markdown、图片和 PDF 文件组织在一个本地 Vault 中。

它适合个人笔记、项目文档和离线知识库。数据保存在本地文件系统，不依赖远程数据库。

## 特性

- Rust + Axum 后端，Svelte 5 + TypeScript 前端
- 单机本地运行，启动后自动打开浏览器
- 新建空笔记使用 Milkdown Crepe；已有 Markdown 使用原文编辑模式，避免 Obsidian 语法、换行和列表格式被自动重写
- `[[WikiLink]]` 双链补全、跳转和自动创建
- 文件树、文件搜索、多标签页和 Zen 沉浸模式
- 明暗主题切换
- 粘贴或拖拽图片自动保存到当前笔记目录的 `_assets/`
- PDF 查看器和图片查看器
- 文件系统变更通过 WebSocket 同步到前端
- 编辑保存使用版本校验、原子写入和幂等请求保护
- 路径穿越、隐藏目录和 symlink 越界防护
- 前端构建产物嵌入 Rust 二进制，运行时不需要 Node.js

## 安装

### 下载 Release

从 [GitHub Releases](https://github.com/binary4cat/catdo/releases/tag/v0.1.1) 下载对应平台的压缩包：

| 平台 | 文件 |
| --- | --- |
| Windows x64 | `catdo-windows-x64.zip` |
| Linux x64 | `catdo-linux-x64.tar.gz` |
| Linux ARM64 | `catdo-linux-arm64.tar.gz` |
| macOS Intel | `catdo-macos-x64.tar.gz` |
| macOS Apple Silicon | `catdo-macos-arm64.tar.gz` |

`SHA256SUMS.txt` 提供发布文件校验和。

解压后直接运行对应的 `catdo` 或 `catdo.exe` 即可。

## 使用

### Windows

```powershell
.\catdo.exe C:\Users\you\Documents\vault
```

### Linux/macOS

```bash
./catdo ~/Documents/vault
```

不传路径时使用当前目录：

```bash
catdo
```

启动后，catdo 会：

1. 检查 Vault 路径；
2. 创建或加载 `.catdo/config.json`；
3. 监听本地随机端口；
4. 打开默认浏览器访问本地界面。

服务只监听 `127.0.0.1`。

## Vault 结构

```text
vault/
├── notes/
│   └── todo.md
├── projects/
│   └── catdo.md
├── _assets/              # 图片等资源，不显示在笔记文件树中
└── .catdo/
    └── config.json       # catdo 配置，不显示在文件树中
```

`.catdo`、`.git`、`.obsidian` 和 `_assets` 会从文件树中隐藏。图片粘贴或拖拽到笔记时，资源会保存到当前笔记目录下的 `_assets/`。

## 配置

首次启动会生成：

```json
{
  "version": 1,
  "assetsDirName": "_assets",
  "theme": "system",
  "autoSaveIntervalMs": 1000,
  "wikiLinks": {
    "autoCreate": true
  }
}
```

配置可以在界面中读取和保存，也可以直接编辑 Vault 下的 `.catdo/config.json`。

## 界面功能

### Markdown 原文模式

已有 Markdown 文件会直接以原文模式打开。打开文件不会触发保存或格式化；编辑时只写入用户实际输入的内容，并尽量保持原文件的换行格式、Frontmatter、Callout、Dataview、嵌入和其他 Obsidian 语法。

### WikiLink

在编辑器中输入：

```markdown
[[项目规划]]
```

选择补全项后，catdo 会生成指向目标 Markdown 文件的链接。点击链接即可打开目标笔记；启用自动创建时，不存在的目标文件会被创建。

### Zen 模式

完整模式示例：

```text
http://127.0.0.1:<port>/?file=notes/todo.md
```

Zen 模式示例：

```text
http://127.0.0.1:<port>/?file=notes/todo.md&zen=true
```

Zen 模式隐藏文件树和标签栏，只保留编辑器、保存状态和主题切换。

## HTTP API

Vault 路径参数均使用相对于 Vault 根目录的 Unix 风格路径，例如 `notes/todo.md`。

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `GET` | `/api/tree` | 获取文件树 |
| `GET` | `/api/search?query=...` | 搜索文件 |
| `GET` | `/api/markdown-paths` | 获取 Markdown 路径，用于 WikiLink 补全 |
| `GET` | `/api/file?path=...` | 读取文件和版本 |
| `PUT` | `/api/file` | 版本校验后原子保存完整文件 |
| `PATCH` | `/api/file` | 使用行级操作增量修改文件 |
| `POST` | `/api/assets/upload` | 上传图片等资源 |
| `GET` | `/api/config` | 读取配置 |
| `PUT` | `/api/config` | 保存配置 |
| `GET` | `/raw/{path}` | 读取图片/PDF 等原始资源 |
| `GET` | `/api/ws` | 接收文件变更事件 |

WebSocket 文件事件格式：

```json
{
  "event": "change",
  "path": "notes/todo.md"
}
```

## 从源码构建

要求：

- Rust stable toolchain
- Node.js 22+
- npm

先构建前端静态资源，再构建 Rust 二进制：

```bash
cd web
npm ci
npm run build
cd ..
cargo build --release --locked
```

构建产物：

```text
Windows: target/release/catdo.exe
Linux/macOS: target/release/catdo
```

开发时可分别启动前端和后端：

```bash
cd web
npm run dev
```

```bash
cargo run -- ./path/to/vault
```

## 验证

```bash
cargo test --quiet
cargo build --release --locked
cd web
npm ci
npm run build
npx tsc --noEmit
```

## 发布

项目包含 GitHub Actions 发布工作流：

```text
.github/workflows/release.yml
```

推送符合 `v*.*.*` 格式的标签后，工作流会构建 Windows x64、Linux x64、Linux ARM64、macOS x64 和 macOS ARM64，并创建 GitHub Release：

```bash
git tag -a v0.1.1 -m "catdo v0.1.1"
git push origin v0.1.1
```

## 许可证

本项目采用 [MIT License](LICENSE) 开源。
