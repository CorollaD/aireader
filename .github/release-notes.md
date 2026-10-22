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
