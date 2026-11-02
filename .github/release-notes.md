## What's new in 0.7.4 / 更新内容

- **Better Markdown reading** / Markdown 阅读改进：front matter is no longer shown as a heading (its title and author name the book), task lists show ☐/☑, tables read row by row with a bold header, and GitHub alerts, `[[wiki links]]` and math read cleanly. Local images in a Markdown file are copied in on import, so they still show in the library. `.md` files can be opened with AiReader from the file manager. / 头信息（front matter）不再显示成标题，其中的 title 和 author 用作书名和作者；任务清单显示 ☐/☑；表格逐行显示、表头加粗；GitHub 提示框、`[[双链]]` 和公式都能正常显示。导入时会一起复制 Markdown 引用的本地图片，导入后图片照常显示。可以在文件管理器里用 AiReader 打开 `.md` 文件。
- **Sync fixes** / 同步修复：a highlight, bookmark or chat deleted on another device no longer comes back when the open book saves; notes that arrive before their book are no longer skipped for good. / 在另一台设备删除的高亮、书签或对话，不会再被正在打开的书保存回来；比书先到的笔记不会再被永久跳过。
- **Reading aloud fixes** / 朗读修复：a key for an OpenAI-compatible voice service is now kept for that address only, so it is never sent to another service (re-enter it once in Settings → Reading). Voices running on this computer (Kokoro) are reached without the system proxy. Audio that can't be played stops reading with a message, and Listen no longer sets a paused reading going again. / OpenAI 兼容语音服务的密钥现在只用于填写时的那个地址，不会再发给其他服务（需要在「设置 → 阅读」里重新填一次）。本机语音服务（Kokoro）不再走系统代理。音频无法播放时会停止并提示；「试听」后不会再接着读之前的内容。
- **Other fixes** / 其他修复：a book opened from the file manager is no longer hidden behind the Review, Stats or OPDS page; a bookmark added right after opening a book is kept; PDF pages no longer stall while another book is closed. Server: a new account never gets the id (and files) of a deleted one. / 从文件管理器打开的书不会再被复习、统计或 OPDS 页面挡住；刚打开书就加的书签不会丢；关闭另一本书时 PDF 翻页不再卡住。服务端：新账号不会再拿到已删除账号的编号和文件。

**Phones (preview)** / 手机版（预览）：an Android APK and an iOS Simulator build are attached for testing; they haven't been tried on real phones yet. The APK is signed with a key made for this build, so uninstall an earlier test version before installing it. / 附带 Android APK 和 iOS 模拟器版供测试，尚未在真机上试过。APK 用本次构建生成的密钥签名，安装前请先卸载之前的测试版。

**Known limits** / 已知限制：online reading voices have been tested with sample audio only, not yet against each service. / 在线朗读语音只用示例音频测过，还没有逐一连接各服务实测。

## Downloads / 下载

| System / 系统                       | File / 文件                                                                             |
| ----------------------------------- | --------------------------------------------------------------------------------------- |
| macOS, Apple silicon (M1 and later) | `AiReader_*_aarch64.dmg`                                                                |
| macOS, Intel                        | `AiReader_*_x64.dmg`                                                                    |
| Windows 10/11                       | `AiReader_*_x64-setup.exe` or `AiReader_*_x64_en-US.msi`                                |
| Linux                               | `.AppImage`, `.deb` (Debian/Ubuntu) or `.rpm` (Fedora)                                  |
| Android 8+ (preview / 预览)         | `AiReader_*_android.apk`                                                                |
| iOS Simulator (preview / 预览)      | `AiReader_*_ios-simulator.zip` — unzip, then `xcrun simctl install booted AiReader.app` |

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
