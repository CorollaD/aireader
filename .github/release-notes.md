## What's new in 0.6.1 / 更新内容

- **OPDS 2** / 支持 OPDS 2：catalogs in the JSON format work alongside OPDS 1, and catalogs can log in with Digest (used by some Calibre servers). / JSON 格式的书源和 OPDS 1 一样可用，并支持 Digest 登录（部分 Calibre 服务器使用）。
- **Open sources** / 开放书源：add catalogs in one click from Online catalogs → Open sources: Project Gutenberg, Internet Archive, OpenEdition and more. / 在「在线书库 → 开放书源」里一键添加 Project Gutenberg、Internet Archive、OpenEdition 等。
- **Wikisource** / 维基文库：search seven languages, including Chinese classics, and download as EPUB. Very long works may export only their contents page; you'll be told. / 搜索七种语言，含中文古籍，并下载为 EPUB。大部头可能只导出目录，届时会有提示。
- **Gutenberg by language** / Gutenberg 分语言：browse Chinese, German, French, Spanish and Italian books. / 浏览中、德、法、西、意语的书。
- **Library websites** / 图书馆网站：21 libraries without an app interface open in your browser, such as CBETA, Gallica and Aozora Bunko. / 21 个没有应用接口的图书馆可在浏览器打开，如 CBETA、Gallica、青空文库。
- **New formats** / 新格式：DjVu (including text recognition for scans), ODT, RTF and saved HTML pages. / DjVu（扫描件可做文字识别）、ODT、RTF 和保存的 HTML 网页。

**Known limits** / 已知限制：the new formats have had limited testing with real-world files. Internet Archive downloads can be slow or fail on some networks, and its search mostly lists books for loan; use its Open Access section. Text recognition has only been run on an Intel Mac. / 新格式用真实文件测试得还不多。Internet Archive 在部分网络下可能很慢或下载失败，搜索结果多为借阅书，请从 Open Access 分组进入。文字识别只在 Intel Mac 上实际运行过。

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
