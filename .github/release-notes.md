## What's new in 0.7.6 / 更新内容

- **A new look, after Apple Books** / 全新界面，参照 Apple Books：the interface is rebuilt on Tailwind and daisyUI, with clearer, higher-contrast text and a heavier interface font. Books on the shelf stand like real ones, with a spine, page edges and a shadow. / 界面基于 Tailwind 和 daisyUI 重做，文字更清晰、对比度更高，界面字体加粗；书架上的书有书脊、书页厚度和阴影，看起来像真书。
- **More colours** / 更多配色：interface themes Light, Warm, Dark, Slate and Black (Settings → General), and eleven accent colours. / 界面主题新增暖米、灰蓝、纯黑（设置 → 通用），强调色增加到 11 种。
- **Reading themes** / 阅读主题：Original, Quiet, Paper, Bold, Calm and Focus set paper, ink, typeface and weight together, and book text has its own weight setting: regular, medium or bold. / 「原始、安静、纸张、粗体、平静、专注」一键设定纸张、文字颜色、字体和粗细；正文字重可选常规、中等、粗。
- **System menus** / 系统下拉菜单：drop-down choices now use the system's own menus. / 下拉选择改用系统自带的菜单。

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
