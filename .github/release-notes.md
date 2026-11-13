## What's new in 0.7.7 / 更新内容

- **PDF pages take the paper colour** / PDF 页面跟随纸张颜色：on light papers the white page takes the paper's colour; on dark papers the page turns dark with light text. Comics keep their colours. / 浅色纸张下，PDF 的白底变成纸色；深色纸张下，页面变成深底浅字。漫画保持原色。
- **The whole reader in the paper's colour** / 阅读窗口整体纸色：the toolbar, contents, AI panel and appearance panel take the paper's tint, as in Apple Books. / 工具栏、目录、AI 面板和外观面板都带上纸张的色调，和 Apple Books 一样。
- **Finer typesetting** / 更细致的排版：a little space between Chinese and Latin text or numbers, no doubled gaps around punctuation, roomier chapter titles, and restyled quotes, code and links. / 中文与英文、数字之间自动留空，标点不再出现双倍空隙，章节标题留白更大，引用、代码、链接样式更新。
- **Colours easier to tell apart** / 颜色更容易区分：ten accent colours spread around the colour wheel, more distinct interface themes and paper colours, and a new Rose paper. Earlier choices move to the closest new colour. / 强调色改为沿色环均匀分布的 10 种，界面主题和纸张颜色区分更明显，新增「淡粉」纸张。之前选的颜色会自动换成最接近的新颜色。
- **Shelf** / 书架：covers at a steady size whatever the window width, with a title and count above them. / 封面尺寸固定，不再随窗口变宽而变大；上方显示分类名和数量。
- **macOS fix** / macOS 修复：the app name in the sidebar no longer sits under the window buttons. / 侧栏的应用名不再挡住窗口左上角的三个按钮。

**Requirements** / 系统要求：macOS 11 or later (macOS 10.15 is no longer supported); iOS 16.4 or later. / 需要 macOS 11 及以上（不再支持 macOS 10.15）；iOS 16.4 及以上。

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
