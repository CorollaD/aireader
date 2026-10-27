## What's new in 0.5.0 / 更新内容

- **Library in SQLite** / 书库改用 SQLite：books, notes and AI caches are kept in `aireader.db`; optional semantic search with an embedding model. / 书籍、笔记和 AI 缓存存进 `aireader.db`；可选语义检索。
- **Safer saving** / 更安全的保存：progress is saved when you close or quit; opening a book while AiReader runs uses the open window. / 关闭或退出前先保存进度；程序运行时再打开书会在当前窗口打开。
- **PDF & comics** / PDF 与漫画：two-page view, fit height, right-to-left comics, search in PDFs, faster page loading. / 双页、适应高度、漫画从右往左、PDF 全文搜索、翻页更快。
- **Reading** / 阅读：clickable EPUB footnotes and links, your own fonts (TTF/OTF/WOFF). / EPUB 脚注和链接可点击跳转，可导入自己的字体。
- **AI**：conversations saved per book, switch models in the panel, token usage and cost per answer, automatic retries. / 对话按书保存、面板内切换模型、显示用量和费用、出错自动重试。
- **Backup** / 备份：Settings → General → Back up library. / 设置 → 通用 → 备份书库。
- Open EPUB, MOBI/AZW3, FB2 and CBZ files with AiReader from your file manager. / 可在文件管理器中用 AiReader 打开电子书文件。

**Upgrading** / 升级说明：on first launch your library is moved into the database and the old JSON files go to `library/backup-json/`. To go back to 0.4, move them back into `library/` and delete `aireader.db`. / 第一次打开时会把书库导入数据库，旧的 JSON 文件移到 `library/backup-json/`。如需退回 0.4，把它们移回 `library/` 并删除 `aireader.db`。

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
