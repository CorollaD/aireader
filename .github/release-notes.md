## What's new in 0.7.5 / 更新内容

- **Save AI answers as notes** / AI 回答存为笔记：every answer can now be saved, and a button at the top saves the whole chat as one note. Answers about a selected passage still become that passage's note; the rest go to the new **Notes on the book**, at the top of the Notes tab (for PDFs and comics too). They can be expanded, edited and deleted, sync to your other devices, and are included in exports. / 每条回答都能「存为笔记」，顶部按钮可把整段对话存成一条笔记。针对选中文字的回答仍存为那段文字的笔记；其余存进新的「整本书笔记」，显示在「笔记」栏顶部（PDF 和漫画也有）。可以展开、编辑、删除，会同步到其他设备，导出时一并带上。
- **Clear the chat** / 清空对话：the eraser button clears the AI chat; the conversation stays in the history. / 橡皮擦按钮清空 AI 对话，对话仍保留在历史记录里。
- **Crisper reading text** / 正文更清晰：default text is darker on every paper (close to black on light papers) and uses the system's own font smoothing, as in Preview. / 各种纸张颜色下默认文字都更深（浅色纸接近纯黑），并使用系统自带的字体平滑，接近「预览」的观感。
- **File sizes** / 文件大小：a Size column in the list view, the size in the cover tooltip, and readable sizes (KB/MB/GB) in book details. / 列表视图新增「大小」列，封面悬停提示显示文件大小，书籍信息里的大小按 KB/MB/GB 显示。

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
