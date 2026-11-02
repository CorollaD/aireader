## What's new in 0.7.0 / 更新内容

- **New look** / 全新界面：a cleaner, neutral design with one accent colour, across the shelf, reader, AI panel and every dialog. / 更简洁的中性设计，只用一种强调色，覆盖书架、阅读、AI 面板和所有对话框。
- **Light and dark** / 浅色和深色：choose System, Light or Dark in Settings → General. The interface theme is now separate from the reading background. / 在「设置 → 通用」里选择跟随系统、浅色或深色。界面主题现在和阅读背景分开设置。
- **Reading themes** / 阅读主题：eight redesigned backgrounds. With a dark background, the toolbar and sidebar turn dark too. / 重新设计的 8 种背景；背景为深色时，工具栏和侧栏也会变深。
- **macOS title bar** / macOS 标题栏：the window buttons now sit on the top bar, and the top bar drags the window. / 红绿灯融进顶栏，顶栏可以拖动窗口。
- **Small windows** / 小窗口：the text no longer gets squeezed. AI and search panels float over it, the shelf toolbar wraps to two rows, and PDF tools move to the More menu. / 正文不再被挤压：AI 和搜索面板浮在正文上，书架工具栏折成两行，PDF 工具收进「更多」菜单。
- **Subgroups** / 子分组：a group can have subgroups, named like `Parent-Child`. Opening the parent includes the books in its subgroups. / 分组可以建子分组，名字形如 `父-子`，点父分组时包含子分组的书。

**Known limits** / 已知限制：the new look has only been run on macOS (Intel); Windows and Linux builds have not been tried by hand. Reading background presets went from twelve to eight; a colour you already chose from a removed preset keeps working as a custom colour. / 新界面只在 macOS（Intel）上实际运行过，Windows 和 Linux 版本未手动试用。阅读背景预设从 12 种减为 8 种；已选用的被移除预设会作为自定义颜色继续生效。

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
