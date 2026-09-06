# Project Implementation Spec: `catdo` - Local Knowledge Base Engine

## 1. 项目定位与核心设计原则
`catdo` 是一个用 **Rust + Svelte 5** 开发的超轻量、秒级冷启动、单二进制分发的本地知识库工具。
- **启动形式**：CLI 传入知识库路径（`catdo D:/vault` 或当前目录执行 `catdo`），启动本地 Axum 服务并自动唤起默认浏览器打开 `http://127.0.0.1:<port>`。
- **核心体验**：
  - 双链联动：原生支持 `[[WikiLink]]` 自动补全与即时跳转。
  - 资源智能入库：粘贴或拖拽图片时，自动存入当前笔记所在目录的 `_assets/` 子文件夹（不存在则自动创建），并在正文插入相对路径。
  - 项目自包含配置：知识库根目录下自动维护 `.catdo/config.json`，存储专属偏好，开箱自适应。
  - 独立 Zen 标签页：支持将任意笔记在新浏览器 Tab 中独立打开为纯净沉浸式编辑模式（隐藏侧边栏与文件树）。
- **分发标准**：前端静态资源全量编译并内嵌进单个 Rust 二进制文件（`rust-embed`），零系统运行时依赖。

---

## 2. 严格确定的技术栈规范（无替代选项）

### 2.1 后端技术栈 (Rust)
- **CLI 参数解析**: `clap = { version = "4.5", features = ["derive"] }`
- **异步运行时**: `tokio = { version = "1.40", features = ["full"] }`
- **Web 框架**: `axum = { version = "0.8", features = ["ws", "multipart"] }`
- **文件监控**: `notify = "7.0"`
- **静态资源内嵌**: `rust-embed = "8.5"`
- **系统调用**: `open = "5.3"`（唤醒系统默认浏览器）
- **辅助工具**: `serde = { version = "1.0", features = ["derive"] }`, `serde_json = "1.0"`, `mime_guess = "2.0"`, `tower-http = { version = "0.6", features = ["cors"] }`

### 2.2 前端技术栈 (Web)
- **构建工具 & 框架**: `Vite 6` + `Svelte 5 (Runes 语法)` + `TypeScript`
- **CSS 与设计系统**: `Tailwind CSS v4` + `@tailwindcss/typography`
- **明暗双色主题**: `mode-watcher`
- **图标系统**: `lucide-svelte`
- **Markdown 编辑器**: `@milkdown/kit` + `@milkdown/crepe` (基于 ProseMirror，原生 Markdown AST 所见即所得)
- **PDF 查看器**: `pdfjs-dist`
- **通知系统**: `svelte-sonner`

---

## 3. 知识库配置与资产管理规范

### 3.1 `.catdo/config.json` 规则
启动时后端检测目标 Vault 根目录。若不存在 `.catdo/` 目录，则立即自动创建，并初始化 `.catdo/config.json`：
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
*注：后端与前端的目录扫描必须**严格忽略并隐藏** `.catdo`、`.git`、`.obsidian` 目录，禁止在文件树中展示。*

### 3.2 粘贴图片入库流水线
当用户在编辑器中触发 `paste` 或 `drop`，且内容为图片时：
1. **生成命名**：格式固定为 `Pasted_image_YYYYMMDD_HHmmss.png`。
2. **计算目标物理目录**：
   - 设当前正在编辑的文件相对路径为 `work/project/note.md`。
   - 目标资源相对目录即为 `work/project/_assets/`。
   - 目标存储相对路径即为 `work/project/_assets/Pasted_image_20260906_180000.png`。
3. **调用上传接口**：发送 `POST /api/assets/upload`（携带 `file` 与 `target_dir`）。
4. **后端操作**：检查若目标 `_assets` 目录不存在，调用 `std::fs::create_dir_all` 自动新建，保存二进制文件。
5. **正文插入**：前端获取返回后，在光标处插入 Markdown 语法：
   ```markdown
   ![](_assets/Pasted_image_20260906_180000.png)
   ```

---

## 4. 后端接口规范 (API Spec)

Vault 内所有路径参数均采用**相对于知识库根目录的 Unix 格式相对路径**（以 `/` 分隔）。

### 4.1 数据结构
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileNode {
    pub name: String,
    pub path: String,       // 相对路径，如 "notes/arch.md"
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
    pub children: Option<Vec<FileNode>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultConfig {
    pub version: u32,
    pub assets_dir_name: String,
    pub theme: String,
    pub auto_save_interval_ms: u64,
}
```

### 4.2 REST 接口
1. **`GET /api/tree`**
   - **响应**：`Vec<FileNode>`（自动过滤 `.catdo`, `.git`, `.obsidian` 以及各个层级的 `_assets` 目录，`_assets` 仅存放静态资源，不污染笔记树）。
2. **`GET /api/file?path=<relative_path>`**
   - **响应**：`{ "path": string, "content": string, "modified": number }`。
3. **`PUT /api/file`**
   - **请求体**：`{ "path": string, "content": string }`
   - **操作**：原子覆盖写盘。如果文件所在父目录不存在，递归自动创建。
4. **`POST /api/assets/upload`**
   - **请求体**：`multipart/form-data`（包含 `target_dir`: string, `file`: binary）。
   - **操作**：将文件保存至 `<vault_root>/<target_dir>/<file_name>`，返回 `{ "relative_path": string }`。
5. **`GET /api/config`** 与 **`PUT /api/config`**
   - 读取或更新 `.catdo/config.json`。
6. **`GET /raw/*relative_path`**
   - **响应**：读取图片/PDF，利用 `mime_guess` 设置正确的 `Content-Type`，直接以二进制流返回。

### 4.3 WebSocket 同步接口 (`/api/ws`)
- `notify` 监听目标 Vault 目录。文件发生物理改动时广播：
  ```json
  { "event": "change" | "create" | "remove", "path": "notes/todo.md" }
  ```

---

## 5. 前端架构与核心模块实现

### 5.1 路由与沉浸式 Zen 模式
利用标准 URL Query 参数区分视图，无需额外引入笨重路由库：
- **完整模式**：`http://127.0.0.1:<port>/?file=notes/todo.md`
  - 显示左侧文件树、顶部 Tab、全局搜索栏、状态条。
- **独立 Tab / Zen 纯净模式**：`http://127.0.0.1:<port>/?file=notes/todo.md&zen=true`
  - **完全不挂载** 侧边栏和多 Tab 管理器。
  - 界面仅包含一个全屏容器，编辑区最大宽度限制为 `max-w-4xl mx-auto`。
  - 右上角仅悬浮极简图标：保存状态圆点指示器、明暗切换按钮。
  - Tab 标签页右键菜单提供：“在独立新标签页打开 (Zen Mode)”，直接调用 `window.open('/?file=' + encodeURIComponent(path) + '&zen=true', '_blank')`。

### 5.2 编辑器与双链 (Milkdown Crepe 增强)
1. **基础编辑体验**：
   - 采用 `@milkdown/crepe` 提供的现代化预设（包括 Block Handle、Slash Menu、Floating Toolbar）。
2. **双链语法支持 (`[[Note Name]]`)**：
   - 配置输入建议监听：输入 `[[` 触发补全下拉浮层，数据源为前端已缓存的全部 Markdown 相对路径与文件名。
   - 选择对应文件后，自动补全为 `[[path/to/target.md|显示标题]]`。
   - 渲染双链节点：在编辑器内部挂载为带有内部跳转样式的特殊 Inline 节点，绑定 `click` 事件。点击后直接切换主工作区当前文件（若目标文件不存在，自动请求 `PUT /api/file` 生成空笔记并打开）。
3. **粘贴劫持**：
   - 在 Milkdown 挂载的容器上监听 `paste` 事件：
     ```typescript
     editorElement.addEventListener('paste', async (event: ClipboardEvent) => {
       const items = event.clipboardData?.items;
       if (!items) return;
       for (const item of items) {
         if (item.type.startsWith('image/')) {
           event.preventDefault();
           const file = item.getAsFile();
           if (file) await handleImageUpload(file);
         }
       }
     });
     ```

### 5.3 界面布局与明暗模式 (Design System)
- **配色规范**：
  - 浅色模式（Light）：背景 `#ffffff`，侧边栏 `#f9fafb`，主文本 `#111827`，边框 `#e5e7eb`。
  - 深色模式（Dark）：背景 `#09090b`，侧边栏 `#121215`，主文本 `#f4f4f5`，边框 `#27272a`。
- **统一布局组件树**：
  ```text
  App.svelte
  ├── [if !isZenMode] Sidebar.svelte (文件树、搜索、新建笔记、明暗切换)
  ├── MainArea.svelte
  │   ├── [if !isZenMode] TabBar.svelte (已打开标签页列表、独立新标签打开入口)
  │   ├── EditorContainer.svelte
  │   │   ├── MilkdownEditor.svelte (Markdown / 双链 / 粘贴入库)
  │   │   ├── PdfViewer.svelte (pdfjs-dist Canvas 渲染)
  │   │   └── ImageViewer.svelte (图片原生渲染，附带平移缩放)
  ```

---

## 6. 项目物理目录结构

```text
catdo/
├── Cargo.toml
├── src/
│   ├── main.rs            # CLI 解析 (clap)、端口分配、唤起浏览器
│   ├── config.rs          # .catdo/config.json 自动初始化与读写
│   ├── server/
│   │   ├── mod.rs         # 路由组装
│   │   ├── routes.rs      # REST API 接口定义
│   │   ├── assets.rs      # Multipart 图片上传与静态文件分发
│   │   ├── ws.rs          # WebSocket 实时推送
│   │   └── embed.rs       # rust-embed 托管前端构建产物
│   └── watcher.rs         # notify 文件系统变动监听
└── web/
    ├── package.json
    ├── vite.config.ts
    ├── src/
    │   ├── app.css
    │   ├── main.ts
    │   ├── App.svelte
    │   └── lib/
    │       ├── types.ts
    │       ├── api.ts
    │       ├── components/
    │       │   ├── Sidebar.svelte
    │       │   ├── TabBar.svelte
    │       │   ├── MilkdownEditor.svelte
    │       │   ├── PdfViewer.svelte
    │       │   ├── ImageViewer.svelte
    │       │   └── ThemeToggle.svelte
    │       └── stores/
    │           └── workspace.svelte.ts
```

---

## 7. 执行步骤 Checklist（交由 Codex 依次执行）

- [ ] **Step 1: 建立 Rust CLI 与配置内核**
  - 使用 `clap` 解析 `vault_path`。
  - 实现 `config.rs`：在知识库下自动创建 `.catdo/` 并在其中初始化/加载 `config.json`。
- [ ] **Step 2: 实现后端核心文件接口**
  - 构建 `GET /api/tree`（递归遍历目录并自动排除 `.catdo`, `.git`, `.obsidian`, `_assets`）。
  - 构建 `GET /api/file` 和 `PUT /api/file`（具备父级目录自动递归创建逻辑）。
  - 构建 `POST /api/assets/upload`（流式处理 Multipart 上传，目标 `_assets` 目录不存在时自动创建）。
  - 构建 `GET /raw/*relative_path` 二进制静态资源服务。
  - 集成 `notify` 与 Axum WebSocket 路由，实现文件变动推送。
- [ ] **Step 3: 构建 Svelte 5 前端框架与主题**
  - 初始化 Vite + Svelte 5 + Tailwind CSS v4。
  - 集成 `mode-watcher`，实现高对比度的深浅双色主题无缝切换。
  - 实现 URL Query 解析：当检测到 `zen=true` 时切换为极简独立编辑视图。
- [ ] **Step 4: 集成 Milkdown Crepe、双链与资产入库**
  - 接入 `@milkdown/crepe`，封装为 `MilkdownEditor.svelte`。
  - 拦截 `paste` 与 `drop` 事件，调用 `POST /api/assets/upload`，实现图片存入当前目录下的 `_assets/` 并插入相对路径 Markdown。
  - 实现 `[[WikiLink]]` 键入补全、显示与跨文档跳转机制。
- [ ] **Step 5: 接入 PDF 与图片查看器**
  - 集成 `pdfjs-dist`，为 PDF 预览在深色模式下提供明暗滤镜反色支持。
  - 实现 `ImageViewer.svelte`，提供纯净图片查看。
- [ ] **Step 6: 单二进制构建验证**
  - 在 Rust 中使用 `rust-embed` 映射 `web/dist`，实现未匹配路由 fallback 至 `index.html`。
  - 执行 `cargo build --release`，产出单个 `catdo` 可执行文件，并在实际本地文件夹中验证全套链路。