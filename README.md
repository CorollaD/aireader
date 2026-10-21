# AiReader

用 Rust 编写的跨平台桌面阅读器，基于 [egui/eframe](https://github.com/emilk/egui)。

| 格式 | 支持情况 |
| --- | --- |
| EPUB 2/3 | 按 spine 顺序读取章节；NCX/nav 目录（含锚点跳转）；内嵌图片 |
| PDF | 使用 PDFium 渲染；书签目录；连续滚动 / 单页两种模式；可缩放 |
| TXT | 自动识别编码（UTF-8/UTF-16/GBK/Big5 等）；自动识别「第X章」「Chapter N」等章节标题 |
| Markdown | 标题、列表、引用、代码块、表格、本地图片 |
| MOBI / AZW / AZW3 | MOBI6 与 KF8 尽量支持；**不支持 DRM 加密的书** |
| FB2 / FB2.ZIP | 章节嵌套、诗歌、题记、内嵌 base64 图片、windows-1251 编码 |
| DOCX | 标题样式或字号推断标题、粗体/斜体、列表、表格（按行显示）、图片 |
| CBZ | 漫画压缩包，图片按文件名自然排序 |

其他功能：中文字体自动加载、明亮/夜间/护眼主题、字号/行距/版心宽度可调、全文搜索、自动记忆阅读位置、最近阅读列表、拖拽打开。

## 构建与运行

```bash
./scripts/fetch-pdfium.sh      # 下载 PDFium 动态库到 ./lib（只有 PDF 需要）
cargo run --release            # 打开启动页
cargo run --release -- 书.epub  # 直接打开文件
```

程序会依次在以下位置查找 PDFium：环境变量 `AIREADER_PDFIUM_DIR`、可执行文件旁的 `lib/`、`../Frameworks`（macOS app 包）、当前目录下的 `lib/`，最后是系统库路径。找不到时只有 PDF 打不开，其他格式不受影响。

发布时请把 `libpdfium.dylib`（macOS）、`libpdfium.so`（Linux）或 `pdfium.dll`（Windows）放在可执行文件旁边的 `lib/` 目录中。

## 快捷键

| 按键 | 文字类文档 | PDF / 漫画 |
| --- | --- | --- |
| ← / → | 上一章 / 下一章 | 上一页 / 下一页 |
| 空格、PgDn / Shift+空格、PgUp | 向下 / 向上翻一屏，到章节末尾时自动进入下一章 | 翻页（单页模式）或滚动一屏 |
| ↑ / ↓ | 小幅滚动 | 上一页 / 下一页（单页模式） |
| Home / End | 章节开头 / 末尾 | 第一页 / 最后一页 |
| Ctrl/⌘ + = / - | 调整字号 | 缩放 |
| Ctrl/⌘ + F | 全文搜索 | — |
| Ctrl/⌘ + O | 打开文件 | 打开文件 |
| T | 显示 / 隐藏目录 | 显示 / 隐藏目录 |

## 配置

设置、最近阅读和阅读位置保存在系统配置目录下的 `settings.json` 中（macOS：`~/Library/Application Support/com.aireader.AiReader/`）。

- `AIREADER_FONT=/path/font.ttf`：指定正文使用的字体
- `AIREADER_SCREENSHOT=out.png`：（开发用）文档加载完成后截取窗口并退出

## 开发

```bash
cargo test                                      # 单元测试 + 各格式端到端测试（测试文件在运行时生成）
cargo run --example inspect -- book.epub a.pdf  # 打印解析结果：目录、章节、图片数量
```

代码结构：

- `src/model.rs`：与格式无关的文档模型。分为两类内容：可重排的 `Chapter`/`Block`（文字类）和固定版式的 `PageSource`（PDF、CBZ）
- `src/formats/`：各格式解析器；`html.rs` 由 EPUB 和 MOBI 共用，`builder.rs` 负责行内样式和空白处理
- `src/ui/`：启动页、阅读页、可重排视图（只布局视口附近的内容块）、分页视图（后台线程渲染页面）
- `src/app.rs`：页面切换、后台加载、设置保存
