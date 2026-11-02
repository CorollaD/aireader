## What's new in 0.7.1 / 更新内容

- **Accent colours** / 强调色：pick one of seven in Settings → General: indigo, blue, teal, violet, rose, orange or graphite. / 在「设置 → 通用」里选择七种之一：靛蓝、蓝、青、紫、玫红、橙、石墨。
- **New icon and finer details** / 新图标与细节：a new app icon; frosted menus and toolbars; AI features marked with a gradient. / 新的应用图标；菜单和工具栏改为毛玻璃；AI 功能用渐变标出。
- **Your own sync server** / 自建同步服务器：sync through an AiReader server with an account, alongside WebDAV and folders. The server is in the `server` folder of the project, with Docker and HTTPS setup; the app keeps only a sign-in token in the system keychain. / 除了 WebDAV 和文件夹，还可以登录自建的 AiReader 服务器同步。服务端在项目的 `server` 目录，附 Docker 和 HTTPS 部署说明；App 只在系统钥匙串里保存登录令牌。
- **Sync fix** / 同步修复：two devices set up at the same moment could get the same id and skip each other's data. / 两台在同一时刻配置的设备可能得到相同的编号，互相跳过对方的数据。

**Phones** / 手机版：Android and iOS builds are in progress and not part of this release yet. / Android 和 iOS 版还在开发中，不在本次发布中。

**Known limits** / 已知限制：the sync server has been tested on macOS and in Docker, not yet on a public server. / 同步服务器在 macOS 和 Docker 里测试过，还没有在公网服务器上实际运行。

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
