# AiReader 同步服务器

AiReader 的自建同步服务：账号登录，每个账号有一块私有存储，App 通过它在多台设备之间同步书架、阅读进度、书签、高亮、AI 对话、闪卡、阅读时长，以及（可选的）书籍文件。合并规则和 WebDAV 同步完全相同。

在 App 里：设置 → 同步 → AiReader 服务器，填服务器地址、用户名和密码，点「登录」。

## 部署

### Docker Compose（推荐，自动 HTTPS）

1. 把域名（例如 `sync.example.com`）解析到服务器，放行 80 和 443 端口。
2. 复制本目录到服务器，把 `Caddyfile` 里的 `sync.example.com` 改成你的域名。
3. 启动：

   ```bash
   docker compose up -d
   ```

4. 创建账号（会提示输入两次密码）：

   ```bash
   docker compose exec -it aireader aireader-server user add alice
   ```

Caddy 会自动申请和续期 HTTPS 证书。App 里的服务器地址填 `https://sync.example.com`。

### 只用 Docker

```bash
docker build -t aireader-server .
docker run -d --name aireader --restart unless-stopped \
  -p 127.0.0.1:8080:8080 -v aireader-data:/data aireader-server
docker exec -it aireader aireader-server user add alice
```

再用 Nginx 或 Caddy 把 HTTPS 反向代理到 `127.0.0.1:8080`。Nginx 需要放宽上传大小，例如 `client_max_body_size 1100m;`。

### 不用 Docker

```bash
cargo build --release
./target/release/aireader-server --data /var/lib/aireader --listen 127.0.0.1:8080
./target/release/aireader-server --data /var/lib/aireader user add alice
```

## 配置

命令行参数和环境变量二选一：

| 参数 | 环境变量 | 默认 | 说明 |
|---|---|---|---|
| `--data` | `AIREADER_DATA` | `data` | 账号数据库和文件的存放目录 |
| `--listen` | `AIREADER_LISTEN` | `0.0.0.0:8080` | 监听地址 |
| `--open-registration` | `AIREADER_OPEN_REGISTRATION` | 关 | 允许任何人在 App 里注册 |
| `--quota-mb` | `AIREADER_QUOTA_MB` | `0` | 每个账号的空间上限（MB），0 为不限 |
| `--max-upload-mb` | `AIREADER_MAX_UPLOAD_MB` | `1024` | 单个文件的大小上限（MB） |

开放注册默认关闭：服务一旦暴露在公网，任何人都能注册并占用空间。只给自己和家人用时，用下面的命令建账号即可。

## 账号管理

```bash
aireader-server user add <名字>      # 新建账号
aireader-server user passwd <名字>   # 重设密码，该账号在所有设备上退出登录
aireader-server user delete <名字>   # 删除账号和它的全部文件
aireader-server user list            # 列出账号
```

在 Docker 里运行时，在前面加上 `docker compose exec -it aireader`（或 `docker exec -it aireader`）。没有终端时，可以用环境变量 `AIREADER_PASSWORD` 传入密码。

## 安全

- 密码用 Argon2id 哈希保存。登录令牌是 32 字节的随机数，服务器只保存它的 SHA-256。App 只在系统钥匙串里保存令牌，不保存密码。
- 同一用户名连续输错 5 次密码后，每次再试都要等待，等待时间逐次加倍，最长 15 分钟。
- 每个账号只能访问自己的目录，路径里不允许出现 `..`、隐藏文件和绝对路径。
- 改密码会让该账号所有设备上的令牌失效。
- 数据在服务器上不加密：服务器管理员能读到笔记和对话。公网部署务必放在 HTTPS 后面。

## 备份

所有数据都在数据目录里：`accounts.db` 是账号，`files/<账号编号>/` 是同步的文件。备份时先停服务，或者直接复制整个卷：

```bash
docker run --rm -v aireader-data:/data -v "$PWD":/backup debian tar czf /backup/aireader.tgz -C /data .
```

## 接口

所有接口在 `/api/v1` 下，需要登录的请求带 `Authorization: Bearer <令牌>`，出错时返回 `{"error": 代码, "message": 说明}`。

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/info` | 服务信息，是否开放注册 |
| POST | `/register`、`/login` | `{username, password}` → `{token, username}` |
| POST | `/logout` | 让当前令牌失效 |
| POST | `/password` | `{old, new}`，返回新令牌 |
| GET | `/me` | 用户名和已用空间 |
| GET | `/list?path=` | 列出目录 |
| GET / PUT / DELETE | `/file?path=` | 读取 / 写入 / 删除文件 |
| POST | `/mkdir?path=` | 建目录 |
