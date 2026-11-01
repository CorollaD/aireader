## What's new in 0.6.0 / 更新内容

- **Sync between devices** / 多设备同步：shelf, reading position, bookmarks, highlights, AI conversations, flashcards and reading time, through WebDAV or a cloud-drive folder. Book files are optional. Data is not encrypted. / 通过 WebDAV 或网盘文件夹同步书架、进度、书签、高亮、AI 对话、闪卡和阅读时长，书籍文件可选。数据不加密。
- **Text recognition for scanned PDFs** / 扫描版 PDF 文字识别：runs on your device and makes pages searchable, selectable and readable aloud. The first use downloads about 31 MB of models. / 在本机识别，识别后可搜索、选中和朗读。首次使用下载约 31MB 模型。
- **Reading papers** / 论文阅读：import by DOI, arXiv id, link or title (open copies only), look up details, click `[12]` to see a reference, and get an AI brief. Lookups run only when you ask. / 用 DOI、arXiv 号、链接或标题导入（仅开放版本），查询文献信息，点 `[12]` 查看参考文献，AI 论文速读。联网查询只在你操作时进行。
- **Citations** / 引用导出：GB/T 7714, APA, MLA, Chicago, IEEE, BibTeX, RIS and CSL-JSON. / 支持以上格式，可复制或存为文件。
- **Search the whole library** / 全库搜索：find words inside every book on your shelf. / 在书架上所有书的正文里搜索。
- **Read aloud** / 朗读：with your system's voices. / 使用系统语音。
- **Flashcards and dictionaries** / 闪卡与词典：spaced review, your own MDict and StarDict dictionaries, AI-made cards and quizzes. / 间隔复习，可导入自己的词典，AI 生成卡片和测验。
- **Reading stats and daily highlights** / 阅读统计与每日回顾。
- **Online catalogs** / 在线书库：browse and download from OPDS catalogs. / 浏览并下载 OPDS 书源里的书。
- **AI and notes** / AI 与笔记：spoiler-free questions, and notes export to Markdown, Obsidian, CSV and JSON. / 防剧透提问；笔记可导出为 Markdown、Obsidian、CSV 和 JSON。

**Upgrading** / 升级说明：your library is updated automatically on first launch. We suggest backing it up first under Settings → General. / 首次打开时书库会自动升级，建议先在「设置 → 通用」里备份。

**Known limits** / 已知限制：new features have had little testing on Windows and Linux; text recognition has only been run on an Intel Mac. / 新功能在 Windows 和 Linux 上测试较少；文字识别只在 Intel Mac 上实际运行过。

## Downloads / 下载

| System / 系统 | File / 文件 |
|---|---|
| macOS, Apple silicon (M1 and later) | `AiReader_*_aarch64.dmg` |
| macOS, Intel | `AiReader_*_x64.dmg` |
| Windows 10/11 | `AiReader_*_x64-setup.exe` or `AiReader_*_x64_en-US.msi` |
| Linux | `.AppImage`, `.deb` (Debian/Ubuntu) or `.rpm` (Fedora) |

## First launch / 首次打开

- **macOS** (not code-signed / 未签名): open the app once; when macOS blocks it, go to **System Settings → Privacy & Security** and click **Open Anyway**. Or run `xattr -dr com.apple.quarantine /Applications/AiReader.app` in Terminal.
  先打开一次，被拦截后到「系统设置 → 隐私与安全性」点击「仍要打开」；或在终端运行上面的命令。
- **Windows**: if SmartScreen appears, click **More info → Run anyway**.
  出现 SmartScreen 提示时，点击「更多信息 → 仍要运行」。
- **Linux AppImage**: `chmod +x AiReader_*.AppImage` before running. / 运行前先加上可执行权限。

## Verify downloads / 校验文件

`SHA256SUMS.txt` lists the SHA-256 checksum of every file. / `SHA256SUMS.txt` 列出了每个文件的 SHA-256 校验值。

```sh
sha256sum --ignore-missing -c SHA256SUMS.txt          # Linux
shasum -a 256 --ignore-missing -c SHA256SUMS.txt      # macOS
```

Windows (PowerShell): `Get-FileHash AiReader_*_x64-setup.exe` and compare with the file. / 用 `Get-FileHash` 计算后与文件中的值对比。
