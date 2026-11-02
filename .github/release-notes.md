## What's new in 0.7.2 / 更新内容

- **Online voices for reading aloud** / 在线朗读语音：besides the system voices (still the default), read aloud with an OpenAI-compatible service (OpenAI, SiliconFlow, or Kokoro running on your computer), Azure Speech, Volcengine (Doubao) or ElevenLabs. Set it up in Settings → Reading and press Listen to try it; keys stay in the system keychain. / 除了系统语音（仍是默认），还可以用 OpenAI 兼容服务（OpenAI、硅基流动或本机运行的 Kokoro）、Azure 语音、火山引擎（豆包语音）或 ElevenLabs 朗读。在「设置 → 阅读」里配置，点「试听」检查；密钥保存在系统钥匙串。
- **Creating an account** / 注册账号：the AiReader server sync form now switches between signing in and creating an account. Accounts are only needed for syncing through an AiReader server. / 同步设置里的 AiReader 服务器可以在「登录」和「创建账号」之间切换。只有通过 AiReader 服务器同步时才需要账号。
- **AI organize fix** / AI 整理修复：large shelves no longer fail with "invalid JSON: EOF while parsing a list"; replies that stop part way are kept and the rest is asked again. / 书很多时不再报「invalid JSON: EOF while parsing a list」；回复中途断掉时保留已有结果，其余的再问一次。
- **Version number** / 版本号：shown at the bottom of Settings → General. / 显示在「设置 → 通用」底部。

**Phones (preview)** / 手机版（预览）：an Android APK and an iOS Simulator build are attached for testing; they haven't been tried on real phones yet. The APK is signed with a key made for this build, so uninstall an earlier test version before installing it. / 附带 Android APK 和 iOS 模拟器版供测试，尚未在真机上试过。APK 用本次构建生成的密钥签名，安装前请先卸载之前的测试版。

**Known limits** / 已知限制：the online voices have been tested with sample audio only, not yet against each service. / 在线语音只用示例音频测过，还没有逐一连接各服务实测。

## Downloads / 下载

| System / 系统 | File / 文件 |
|---|---|
| macOS, Apple silicon (M1 and later) | `AiReader_*_aarch64.dmg` |
| macOS, Intel | `AiReader_*_x64.dmg` |
| Windows 10/11 | `AiReader_*_x64-setup.exe` or `AiReader_*_x64_en-US.msi` |
| Linux | `.AppImage`, `.deb` (Debian/Ubuntu) or `.rpm` (Fedora) |
| Android 8+ (preview / 预览) | `AiReader_*_android.apk` |
| iOS Simulator (preview / 预览) | `AiReader_*_ios-simulator.zip` — unzip, then `xcrun simctl install booted AiReader.app` |

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
