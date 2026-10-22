# AiReader

跨平台桌面阅读器。界面用 [Tauri 2](https://tauri.app) + React，文档解析与渲染用 Rust。

| 格式 | 支持情况 |
| --- | --- |
| EPUB 2/3 | 按 spine 顺序读取章节；NCX/nav 目录（含锚点跳转）；内嵌图片与封面 |
| PDF | 使用 PDFium 渲染；书签目录；连续滚动 / 单页两种模式；可缩放 |
| TXT | 自动识别编码（UTF-8/UTF-16/GBK/Big5 等）；自动识别「第X章」「Chapter N」等章节标题 |
| Markdown | 标题、列表、引用、代码块、表格、本地图片 |
| MOBI / AZW / AZW3 | MOBI6 与 KF8 尽量支持；**不支持 DRM 加密的书** |
| FB2 / FB2.ZIP | 章节嵌套、诗歌、题记、内嵌图片、windows-1251 编码 |
| DOCX | 标题样式或字号推断标题、粗体/斜体、列表、表格（按行显示）、图片 |
| CBZ | 漫画压缩包，图片按文件名自然排序 |

## 功能

**书架**
- 导入单本书、多本书或整个文件夹（递归扫描），也可以把文件或文件夹拖进窗口；显示导入进度
- 导入时会把书**复制**到书库目录，按内容哈希去重，并自动提取书名、作者和封面（PDF/CBZ 取第一页）
- 网格（封面）和列表两种视图；按最近阅读、添加时间、书名、作者、阅读进度排序；按书名或作者搜索
- 自定义分组：一本书可以属于多个分组，侧栏按分组筛选；分组可新建、重命名、删除
- 编辑书名、作者和分组；可多选后批量设置分组或删除
- 显示每本书的阅读进度和最近阅读时间

**阅读**
- 12 种预设背景色（纯白、米白、羊皮纸、护眼绿、薄荷、天青、樱粉、浅灰、深灰、夜间、深蓝、墨黑），也可以用取色器自定义；文字颜色可以自动匹配，也可以自定义；整个界面跟随背景色变化
- 字体（黑体/宋体/楷体）、字号、行距、版心宽度可调
- 目录侧栏、全文搜索（结果高亮）、自动保存阅读位置

## 开发

需要 Rust（1.85 及以上）和 Node.js。

```bash
npm install
./src-tauri/scripts/fetch-pdfium.sh   # 下载 PDFium 动态库到 src-tauri/lib（只有 PDF 需要）
npm run tauri dev                     # 开发模式
npm run tauri build                   # 打包安装程序
```

- 也可以在命令行直接打开文件：`aireader 书.epub`。不在书架上的书会先导入，再直接进入阅读。
- `npm run dev` 后在普通浏览器中打开 http://localhost:1420，会使用 `src/dev/mock.ts` 中的模拟后端，方便调试界面。生产构建不会包含这部分代码。

### PDFium

程序会依次在以下位置查找 PDFium：环境变量 `AIREADER_PDFIUM_DIR`、应用资源目录下的 `lib/`、可执行文件旁的 `lib/`、`../Frameworks` 和 `../Resources/lib`（macOS app 包）、`src-tauri/lib`（开发时），最后是系统库路径。找不到时只有 PDF 打不开，其他格式不受影响。

打包时会自动带上 PDFium：macOS 放进 `Contents/Frameworks/`（见 `tauri.macos.conf.json`），Linux 和 Windows 作为资源文件随安装包分发（见 `tauri.linux.conf.json`、`tauri.windows.conf.json`）。在 macOS 和 Linux 上，`npm run tauri build` 会先运行 `fetch-pdfium.sh --if-missing`，`src-tauri/lib` 中没有这个库时自动下载。在 Windows 上，请先在 Git Bash 中运行该脚本。

### 数据位置

书库保存在系统应用数据目录下的 `library/` 中（macOS：`~/Library/Application Support/com.aireader.reader/library/`）：

- `library.json`：书籍信息、分组、阅读进度和阅读设置
- `books/<id>/`：导入书籍的副本
- `covers/<id>.jpg`：封面缩略图

### 测试

```bash
cd src-tauri
cargo test                                      # 解析器、书库、JSON 结构等单元测试与端到端测试
cargo run --example inspect -- book.epub a.pdf  # 打印解析结果：目录、章节、图片数量
```

## 代码结构

```
src/                      React 前端
  shelf/                  书架：网格/列表、分组侧栏、编辑对话框
  reader/                 阅读：文字类视图、分页视图、设置面板
  settings.ts             阅读设置与配色方案（背景色预设、根据背景自动计算文字颜色）
  api.ts / types.ts       调用 Tauri 命令的封装，以及与 Rust 对应的数据类型
src-tauri/                Rust 后端
  src/formats/            各格式解析器（html.rs 由 EPUB 与 MOBI 共用）
  src/model.rs            与格式无关的文档模型：可重排章节，或按页渲染的文档
  src/library.rs          书库：导入、去重、封面、分组、进度（不依赖 Tauri，便于测试）
  src/commands.rs         提供给前端调用的命令
  src/protocol.rs         book:// 协议，用于加载封面、书中图片和 PDF/CBZ 页面
```

键盘快捷键：← / → 上一章或上一页、下一章或下一页；空格 / PgDn 翻一屏（读到章节末尾时进入下一章）；Home / End 跳到开头或末尾；Ctrl/⌘ +/- 调整字号（PDF 为缩放）；Ctrl/⌘ F 搜索；T 显示或隐藏目录；Esc 关闭面板，或返回书架。
