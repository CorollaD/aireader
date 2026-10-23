# AiReader

跨平台桌面阅读器。界面用 [Tauri 2](https://tauri.app) + React，文档解析与渲染用 Rust。

**下载**：在 [Releases](https://github.com/CorollaD/aireader/releases) 页面下载 macOS（Apple 芯片 / Intel）、Windows 和 Linux 的安装包，`SHA256SUMS.txt` 可用来校验文件。首次打开的方法见 Release 说明，签名情况见下文「代码签名」。

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
- 查找重复：找出同一本书的不同文件（不同格式、不同扫描版），按是否有文字、扫描清晰度（DPI）、格式、目录和内容完整度推荐保留最好的一本；不同译本、改动大的版本按正文相似度区分开。去掉的副本的阅读进度、书签和笔记会尽量转到保留的书上。导入新书时发现可能重复也会提示
- 网格（封面）和列表两种视图；按最近阅读、添加时间、书名、作者、阅读进度排序；按书名或作者搜索
- 自定义分组：每本书属于一个分组（或未分组），侧栏按分组筛选；分组可新建、重命名、删除
- 编辑书名、作者和分组；可多选后批量移到某个分组或删除
- 显示每本书的阅读进度和最近阅读时间

**阅读**
- 书签（⌘/Ctrl+D）、高亮（5 种颜色）与笔记，侧栏「目录 / 书签 / 笔记」，笔记可导出为 Markdown
- 后退 / 前进（跳转后返回原位置）、可拖动的进度条、输入百分比或页码跳转
- 文字类书籍可在「滚动」和「分页」（单栏 / 双栏）之间切换；段距、首行缩进、两端对齐、页边距可调
- 专注模式（F）、全屏、快捷键帮助（?）
- 阅读时长统计、本章与全书剩余时间估算（按你的实际阅读速度）
- PDF：适应宽度 / 适应整页、页面旋转
- 12 种预设背景色（纯白、米白、羊皮纸、护眼绿、薄荷、天青、樱粉、浅灰、深灰、夜间、深蓝、墨黑），也可以用取色器自定义；文字颜色可以自动匹配，也可以自定义；整个界面跟随背景色变化
- 字体（黑体/宋体/楷体）、字号、行距、版心宽度可调
- 目录侧栏、全文搜索（结果高亮）、自动保存阅读位置

**AI（基于 [rig](https://github.com/0xPlaygrounds/rig)）**
- 支持 Claude、OpenAI、DeepSeek、Kimi、智谱、Gemini、OpenRouter，本地的 Ollama、llama.cpp，以及任意 OpenAI 兼容服务（通义、硅基流动、LM Studio 等）
- 总结本章 / 本页 / 全书（长文自动分段；每章总结按模型缓存；全书总结前显示用量和费用估算）
- 选中文字后让 AI 解释或翻译，可以继续追问，答案可存为笔记
- 书架「AI 整理」：AI 为每本书建议一个分组和缺失的作者，逐条确认后才写入（已有分组的书默认不移动）
- API Key 保存在系统钥匙串；使用云端服务前会请你确认发送书籍内容；本地服务直连，不经过系统代理
- 回答语言默认跟随界面语言，也可以单独指定

**界面**
- 界面语言：简体中文、繁體中文、English、Español、Italiano、Français、Deutsch、日本語、한국어、Português；默认跟随系统，可以随时切换
- 界面大小：小 / 标准 / 大 / 特大（书架中也可以用 ⌘/Ctrl +、-、0 调整）；正文字号单独设置，不受影响
- 设置窗口（⌘/Ctrl+,）：通用、阅读、AI、快捷键

## 开发

需要 Rust（1.85 及以上）和 Node.js。

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

推送 `v*` 标签（版本号与 `tauri.conf.json` 一致，例如 `v0.2.0`）会触发 `.github/workflows/release.yml`：在 GitHub Actions 上分别构建 macOS（Apple 芯片、Intel）、Linux 和 Windows 安装包，全部成功后发布为 GitHub Release；也可以在 Actions 页面手动运行。

### 数据位置

书库保存在系统应用数据目录下的 `library/` 中（macOS：`~/Library/Application Support/com.aireader.reader/library/`）：

- `library.json`：书籍信息、分组、阅读进度、阅读时长、阅读设置和 AI 设置（不含 API Key）
- `annotations/<id>.json`：书签、高亮与笔记
- `ai/<id>/summaries.json`：AI 总结缓存
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
  reader/                 阅读：文字类视图（滚动/分页、高亮）、PDF 视图、侧栏、进度条、设置面板
  ai/                     AI 设置、AI 面板、书架整理对话框
  i18n/                   界面翻译：zh-CN.ts 定义全部文案的键，其他语言必须逐一提供（TypeScript 检查）
  SettingsDialog.tsx      设置窗口；icons.tsx 图标；zoom.ts 界面缩放
  settings.ts             阅读设置与配色方案（背景色预设、根据背景自动计算文字颜色）
  api.ts / types.ts       调用 Tauri 命令的封装，以及与 Rust 对应的数据类型
src-tauri/                Rust 后端
  src/formats/            各格式解析器（html.rs 由 EPUB 与 MOBI 共用）
  src/model.rs            与格式无关的文档模型：可重排章节，或按页渲染的文档
  src/library.rs          书库：导入、去重、封面、分组、进度（不依赖 Tauri，便于测试）
  src/annotations.rs      书签与高亮笔记的存储、导出 Markdown
  src/ai/                 AI：rig 连接与错误处理、设置与钥匙串、总结/划词/整理任务
  src/i18n.rs             后端文案（错误信息、生成的章节名、导出笔记）的多语言目录
  src/commands.rs         提供给前端调用的命令
  src/protocol.rs         book:// 协议，用于加载封面、书中图片和 PDF/CBZ 页面
```

键盘快捷键：← / → 上一章或上一页、下一章或下一页；空格 / PgDn 翻一屏（读到章节末尾时进入下一章）；Home / End 跳到开头或末尾；Ctrl/⌘ +/- 调整字号（PDF 为缩放）；Ctrl/⌘ F 搜索；T 显示或隐藏目录；F 专注模式；? 快捷键帮助；⌘/Ctrl+, 设置；Esc 关闭面板，或返回书架。

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
