## Downloads / 下载

| System / 系统 | File / 文件 |
|---|---|
| macOS, Apple silicon (M1 and later) | `AiReader_*_aarch64.dmg` |
| macOS, Intel | `AiReader_*_x64.dmg` |
| Windows 10/11 | `AiReader_*_x64-setup.exe` or `AiReader_*_x64_en-US.msi` |
| Linux | `.AppImage`, `.deb` (Debian/Ubuntu) or `.rpm` (Fedora) |

The installers are not code-signed yet. / 安装包暂未签名：

- **macOS**: if macOS says the app "is damaged" or "can't be opened", run `xattr -dr com.apple.quarantine /Applications/AiReader.app` in Terminal, or right-click the app and choose Open.
  若提示"已损坏"或"无法打开"，在终端运行上面的命令，或右键点击应用选择"打开"。
- **Windows**: if SmartScreen appears, click "More info" → "Run anyway".
  出现 SmartScreen 提示时，点击"更多信息" → "仍要运行"。
- **Linux AppImage**: `chmod +x AiReader_*.AppImage` before running. / 运行前先加上可执行权限。
