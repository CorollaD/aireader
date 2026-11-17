## What's new in 0.7.8 / 更新内容

- **Sharper PDF pages** / PDF 更清晰：pages render at exactly the screen's pixels, and "Firmer text" darkens the thin edges of letters, for fuller strokes like the system's own viewers. Scanned pages are encoded at higher quality. / 页面按屏幕像素一比一渲染；「文字增强」加深字母边缘，笔画更实，接近系统自带阅读器的效果。扫描页的图像质量也提高了。
- **Pages that stand out** / 页面更有层次：document pages sit on a neutral grey canvas with a soft shadow, and the toolbar and panels take only a touch of the paper colour. / PDF 页面放在中性灰背景上并带阴影，工具栏和侧栏只带一点纸色。
- **Colours kept** / 保留原色：on coloured paper only the white of the page takes the paper colour; figures and highlights keep their colours. On dark paper, text is a soft light grey instead of glaring white. / 彩色纸张下只有页面白底变成纸色，图片和彩色标注保持原色；深色纸张下文字是柔和的浅灰，不再刺眼。
- **Firmer text in books too** / 文字书同样增强：EPUB, TXT and other books get fuller strokes. "Firmer text" is in the appearance panel, on by default. / EPUB、TXT 等正文笔画更实。外观面板里的「文字增强」开关控制这些效果，默认打开。

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
