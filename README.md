# YourDiary ·《你的日记》Linux 桌面客户端

YourDiary 是《你的日记》的非官方 Linux 桌面客户端，使用 Rust、GTK 4 和 libadwaita 开发。你可以在 Ubuntu 上续写今天的日记、搜索自己和搭档的记录，也可以**将自己的日记批量导出为 Markdown**，保存为 ZIP 压缩包。

YourDiary is an unofficial Linux desktop client for the Chinese shared diary app *你的日记*. Read and search shared entries, browse the Treehole community, and export your own journal as a ZIP archive with one Markdown file per entry.

[下载 Ubuntu 安装包](https://github.com/kclx/YourDiary/releases/latest)

## 将日记导出为 Markdown

在 **偏好设置 → 数据 → 导出我的日记** 中选择保存位置。客户端会核对当前账号、补齐日记全文，并把本人所写的日记打包。每篇对应一个 `.md` 文件，解压后就能用文本编辑器阅读或搜索。

文件名以日期开头。有标题的日记保存为 `2026-03-02 清晨.md`，无标题时保存为 `2026-03-01.md`。文件开头是 YAML 元信息，包含日期，以及有值的标题、天气和心情，之后是保留原有分段的正文。

## 日记与树洞

- 新建或续写今天的日记，在编辑器中按 `Ctrl+T` 插入当前时间。
- 按日期浏览自己和搭档的日记，按 `Ctrl+F` 搜索标题或正文。已缓存的全文支持离线阅读。
- 在树洞瀑布流中浏览帖子，发表评论、回复和点赞，通过通知查看相关互动。

## 在 Ubuntu 上安装 v1.0.0

从 [v1.0.0 发布页](https://github.com/kclx/YourDiary/releases/tag/v1.0.0) 下载安装包和 SHA-256 校验文件。本版适用于 Ubuntu 26.04 amd64，在下载目录执行：

```sh
sha256sum -c yourdiary_1.0.0_amd64.deb.sha256
sudo apt install ./yourdiary_1.0.0_amd64.deb
```

从应用菜单打开「你的日记」。安装版将数据库放在 `${XDG_DATA_HOME:-$HOME/.local/share}/com.orlando.yourdiary/data/yourdiary.sqlite3`。从源码启动的版本继续使用项目目录下的 `data/`。

在已配置 Rust、GTK 4、libadwaita 开发环境和 dpkg 打包工具的 Ubuntu 主机上，运行 `./packaging/build-deb.sh` 可以重新生成安装包与校验文件，输出目录为 `dist/`。

## 从源码安装 Linux 桌面启动器

在项目根目录执行以下命令，即可从应用菜单打开「你的日记」。任务栏使用 `resources/YourDiary.png`，从菜单启动时也会读取项目下的 `data/`。

```sh
cargo build --locked
desktop-file-install --dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications" \
  --set-key=Exec --set-value="\"$PWD/target/debug/YourDiary\"" \
  --set-key=Path --set-value="$PWD" \
  --set-key=Icon --set-value="$PWD/resources/YourDiary.png" \
  resources/com.orlando.yourdiary.desktop
```

## 项目用途与 API 服务条款

作者为 Orlando Vale，Codex 协助开发。

本客户端使用《你的日记》App 提供的 API。
本项目仅用于学习、展示和个人使用。任何使用或分发本项目代码的人，都必须自行了解并遵守该 API 提供方的服务条款。

项目源代码采用 [GNU 通用公共许可证第 3 版（GPL-3.0-only）](LICENSE)。
