# AiReader

跨平台桌面阅读器。界面用 [Tauri 2](https://tauri.app) + React，文档解析与渲染用 Rust。

**下载**：在 [Releases](https://github.com/CorollaD/aireader/releases)（最新版本 [v0.5.0](https://github.com/CorollaD/aireader/releases/tag/v0.5.0)）页面下载 macOS（Apple 芯片 / Intel）、Windows 和 Linux 的安装包，`SHA256SUMS.txt` 可用来校验文件。首次打开的方法见 Release 说明，签名情况见下文「代码签名」。

| 格式 | 支持情况 |
| --- | --- |
| EPUB 2/3 | 按 spine 顺序读取章节；NCX/nav 目录（含锚点跳转）；书内链接与脚注可点击跳转；内嵌图片与封面 |
| PDF | 使用 PDFium 渲染；书签目录；连续 / 单页 / 双页；可缩放、旋转；可选中文字、全文搜索（文字层） |
| TXT | 自动识别编码（UTF-8/UTF-16/GBK/Big5 等）；自动识别「第X章」「Chapter N」等章节标题 |
| Markdown | 标题、列表、引用、代码块、表格、本地图片 |
| MOBI / AZW / AZW3 | MOBI6 与 KF8 尽量支持；**不支持 DRM 加密的书** |
| FB2 / FB2.ZIP | 章节嵌套、诗歌、题记、内嵌图片、windows-1251 编码 |
| DOCX | 标题样式或字号推断标题、粗体/斜体、列表、表格（按行显示）、图片 |
| CBZ | 漫画压缩包，图片按文件名自然排序；双页显示、从右往左翻页 |

## 功能

**书架**
- 导入单本书、多本书或整个文件夹（递归扫描），也可以把文件或文件夹拖进窗口；显示导入进度
- 导入时会把书**复制**到书库目录，按内容哈希去重，并自动提取书名、作者和封面（PDF/CBZ 取第一页）
- 查找重复：找出同一本书的不同文件（不同格式、不同扫描版），按是否有文字、扫描清晰度（DPI）、格式、目录和内容完整度推荐保留最好的一本；不同译本、改动大的版本按正文相似度区分开。去掉的副本的阅读进度、书签和笔记会尽量转到保留的书上。导入新书时发现可能重复也会提示
- 网格（封面）和列表两种视图；按最近阅读、添加时间、书名、作者、阅读进度排序；按书名或作者搜索
- 自定义分组：每本书属于一个分组（或未分组），侧栏按分组筛选；分组可新建、重命名、删除
- 编辑书名、作者和分组；可多选后批量移到某个分组或删除
- 显示每本书的阅读进度和最近阅读时间
- 备份书库（设置 → 通用）：导出一个 zip，含数据库和封面，可选包含书籍文件
- 安装后可以在文件管理器中用 AiReader 打开 EPUB、MOBI/AZW3、FB2、CBZ（以及 PDF）；程序已在运行时会在当前窗口打开，不会再启动一个

**阅读**
- 书签（⌘/Ctrl+D）、高亮（5 种颜色）与笔记，侧栏「目录 / 书签 / 笔记」，笔记可导出为 Markdown
- 后退 / 前进（跳转后返回原位置）、可拖动的进度条、输入百分比或页码跳转
- 文字类书籍可在「滚动」和「分页」（单栏 / 双栏）之间切换；段距、首行缩进、两端对齐、页边距可调
- 专注模式（F）、全屏、快捷键帮助（?）
- 阅读时长统计、本章与全书剩余时间估算（按你的实际阅读速度）
- PDF 与漫画：连续 / 单页 / 双页（封面单独、横向跨页单独显示），适应宽度 / 整页 / 高度，页面旋转；漫画可设为从右往左翻页
- EPUB 中的脚注、交叉引用链接可以点击跳转
- 12 种预设背景色（纯白、米白、羊皮纸、护眼绿、薄荷、天青、樱粉、浅灰、深灰、夜间、深蓝、墨黑），也可以用取色器自定义；文字颜色可以自动匹配，也可以自定义；整个界面跟随背景色变化
- 字体（黑体/宋体/楷体，也可以导入自己的 TTF/OTF/WOFF 字体）、字号、行距、版心宽度可调
- 目录侧栏、全文搜索（结果高亮）、自动保存阅读位置（关闭窗口或退出前也会保存）

**AI（基于 [rig](https://github.com/0xPlaygrounds/rig)）**
- 支持 Claude、OpenAI、DeepSeek、Kimi、智谱、Gemini、OpenRouter，本地的 Ollama、llama.cpp，以及任意 OpenAI 兼容服务（通义、硅基流动、LM Studio 等）
- 语义检索（可选）：在 AI 设置中选择向量模型（本地 Ollama 如 bge-m3，或 OpenAI / OpenAI 兼容服务），在书中手动建立语义索引后，「问问这本书」会同时按关键词和意思查找相关段落
- 总结本章 / 本页 / 全书（长文自动分段；每章总结按模型缓存；全书总结前显示用量和费用估算）
- 选中文字后让 AI 解释或翻译，可以继续追问，答案可存为笔记
- 对话按书保存，可以在历史对话中切换；面板里可以直接切换模型，每次回答显示用量（token 和服务报告或按价目估算的费用）
- 网络错误、限流时自动重试；全书总结中断后再次运行会从已完成的部分继续
- 书架「AI 整理」：AI 为每本书建议一个分组和缺失的作者，逐条确认后才写入（已有分组的书默认不移动）
- API Key 保存在系统钥匙串；使用云端服务前会请你确认发送书籍内容；本地服务直连，不经过系统代理
- 回答语言默认跟随界面语言，也可以单独指定

**界面**
- 界面语言：简体中文、繁體中文、English、Español、Italiano、Français、Deutsch、日本語、한국어、Português；默认跟随系统，可以随时切换
- 界面大小：小 / 标准 / 大 / 特大（书架中也可以用 ⌘/Ctrl +、-、0 调整）；正文字号单独设置，不受影响
- 设置窗口（⌘/Ctrl+,）：通用、阅读、AI、快捷键
- 记住窗口大小和位置；支持键盘操作（书架卡片、进度条、对话框）

## 开发

需要 Rust（1.97.1 及以上，rig 0.45 的要求）和 Node.js（20 及以上）。

```bash
npm install
./src-tauri/scripts/fetch-pdfium.sh   # 下载 PDFium 动态库到 src-tauri/lib（只有 PDF 需要）
npm run tauri dev                     # 开发模式
npm run tauri build                   # 打包安装程序
```

- 也可以在命令行直接打开文件：`aireader 书.epub`。不在书架上的书会先导入，再直接进入阅读。
- `AIREADER_LIBRARY_DIR=/path/to/dir` 让程序使用另一个书库目录（测试用，不影响正常书库）。
- `npm run dev` 后在普通浏览器中打开 http://localhost:1420，会使用 `src/dev/mock.ts` 中的模拟后端，方便调试界面。生产构建不会包含这部分代码。

### PDFium

程序会依次在以下位置查找 PDFium：环境变量 `AIREADER_PDFIUM_DIR`、应用资源目录下的 `lib/`、可执行文件旁的 `lib/`、`../Frameworks` 和 `../Resources/lib`（macOS app 包）、`src-tauri/lib`（开发时），最后是系统库路径。找不到时只有 PDF 打不开，其他格式不受影响。

打包时会自动带上 PDFium：macOS 放进 `Contents/Frameworks/`（见 `tauri.macos.conf.json`），Linux 和 Windows 作为资源文件随安装包分发（见 `tauri.linux.conf.json`、`tauri.windows.conf.json`）。在 macOS 和 Linux 上，`npm run tauri build` 会先运行 `fetch-pdfium.sh --if-missing`，`src-tauri/lib` 中没有这个库时自动下载。在 Windows 上，请先在 Git Bash 中运行该脚本。脚本按要构建的 Rust 目标（`PDFIUM_TARGET` 或 Tauri 提供的 `TAURI_ENV_TARGET_TRIPLE`）选择对应的库，版本固定为 `PDFIUM_VERSION`（默认 8111）。

### 发布

每次推送和 Pull Request 都会运行 `.github/workflows/check.yml`：前端 lint、测试和构建，`cargo fmt`、`clippy`（警告视为错误）和 `cargo test`。

推送 `v*` 标签（版本号与 `package.json`、`Cargo.toml`、`tauri.conf.json` 一致，例如 `v0.5.0`）会触发 `.github/workflows/release.yml`：在 GitHub Actions 上分别构建 macOS（Apple 芯片、Intel）、Linux 和 Windows 安装包，全部成功后发布为 GitHub Release，并附上 `SHA256SUMS.txt`；也可以在 Actions 页面手动运行。Release 的说明取自 `.github/release-notes.md`，发版前更新其中的「更新内容」一节。

### 数据位置

书库保存在系统应用数据目录下的 `library/` 中（macOS：`~/Library/Application Support/com.aireader.reader/library/`）：

- `aireader.db`（SQLite）：书籍信息、分组、阅读进度与时长、阅读设置和 AI 设置（不含 API Key）、书签高亮与笔记、AI 总结与导图缓存、查重分析缓存，以及语义检索的向量（rig-sqlite + sqlite-vec）
- `books/<id>/`：导入书籍的副本
- `covers/<id>.jpg`：封面缩略图
- `fonts/`：导入的阅读字体
- `backup-json/`：旧版本的 JSON 数据。新版第一次打开时会把它们导入数据库，原文件移到这里保留

API Key（包括向量模型的 Key）只保存在系统钥匙串中。

日志写在系统日志目录（macOS：`~/Library/Logs/com.aireader.reader/`），出错时可以附上。

从备份恢复：退出程序，把备份 zip 解压到一个空的书库目录（或用 `AIREADER_LIBRARY_DIR` 指向它）。

### 测试

```bash
npm run lint                                    # ESLint + Prettier
npm test                                        # 前端单元测试（vitest）
cd src-tauri
cargo test                                      # 解析器、书库、JSON 结构等单元测试与端到端测试
cargo run --example inspect -- book.epub a.pdf  # 打印解析结果：目录、章节、图片数量
```

## 代码结构

```
src/                      React 前端
  shelf/                  书架：网格/列表、分组侧栏、编辑对话框、查找重复
  reader/                 阅读：文字类视图（滚动/分页、高亮）、PDF/漫画视图（连续/单页/双页）、侧栏、进度条、设置面板
  ai/                     AI 设置、AI 面板（对话/总结/思维导图）、书架整理对话框
  i18n/                   界面翻译：zh-CN.ts 定义全部文案的键，其他语言必须逐一提供（TypeScript 检查 + 测试）
  dev/mock.ts             浏览器调试用的模拟后端
  SettingsDialog.tsx      设置窗口；icons.tsx 图标；zoom.ts 界面缩放
  settings.ts             阅读设置与配色方案（背景色预设、根据背景自动计算文字颜色）
  lifecycle.ts            退出前需要保存的内容；ErrorBoundary.tsx 界面出错时的兜底
  api.ts / types.ts       调用 Tauri 命令的封装，以及与 Rust 对应的数据类型
  **/*.test.ts            前端单元测试（vitest）
src-tauri/                Rust 后端
  src/formats/            各格式解析器（html.rs 由 EPUB 与 MOBI 共用）
  src/model.rs            与格式无关的文档模型：可重排章节，或按页渲染的文档
  src/db.rs               SQLite 书库数据库：表结构、旧版 JSON 数据导入、对话记录
  src/library.rs          书库：导入、去重、封面、分组、进度、字体、备份（不依赖 Tauri，便于测试）
  src/annotations.rs      书签与高亮笔记的存储、导出 Markdown
  src/dedupe.rs           查找重复：内容相似度、版本质量评分、笔记迁移
  src/search.rs           书内全文搜索（文字书按段落，PDF 按页）
  src/ai/                 AI：rig 连接与错误处理、设置与钥匙串、检索与语义索引、总结/问答/导图/整理任务
  src/i18n.rs             后端文案（错误信息、生成的章节名、导出笔记）的多语言目录
  src/commands.rs         提供给前端调用的命令，以及退出、单实例打开文件的处理
  src/protocol.rs         book:// 协议：封面、书中图片、PDF/CBZ 页面（带缓存）、自定义字体
```

键盘快捷键：← / → 上一章或上一页、下一章或下一页（漫画设为从右往左时方向相反）；空格 / PgDn 翻一屏（读到章节末尾时进入下一章）；Home / End 跳到开头或末尾；Ctrl/⌘ +/- 调整字号（PDF 为缩放）；Ctrl/⌘ F 搜索（含 PDF）；Ctrl/⌘ D 书签；Ctrl/⌘ Shift A 打开或关闭 AI 面板；Alt+← / → 后退、前进；T 显示或隐藏目录；F 专注模式；? 快捷键帮助；⌘/Ctrl+, 设置；Esc 关闭面板，或返回书架。

## 代码签名 / Code signing policy

- **Windows**：计划申请 [SignPath Foundation](https://signpath.org) 为开源项目提供的免费代码签名，获批前安装包不签名。获批后：Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).
- **macOS**：暂不签名。
- **Linux**：不签名，用 Release 中的 `SHA256SUMS.txt` 校验。

所有安装包都由 GitHub Actions 从本仓库的源码构建（`.github/workflows/release.yml`），不在个人电脑上构建。All release builds come from this repository's source via GitHub Actions.

| 角色 / Role | 成员 / Members |
| --- | --- |
| Committers and reviewers | [CorollaD](https://github.com/CorollaD) |
| Approvers | [CorollaD](https://github.com/CorollaD) |

**隐私 / Privacy**：程序不会主动向任何网络服务发送信息，除非用户自己配置并使用了 AI 功能，此时所需的书籍文字会发送给用户选择的 AI 服务（使用云端服务前会先征得同意；本地模型不离开本机）。This program will not transfer any information to other networked systems unless specifically requested by the user: the AI features send the needed book text to the AI service the user configures, after asking for consent (local models stay on the computer).

## 许可证

[GPL-3.0](LICENSE)
