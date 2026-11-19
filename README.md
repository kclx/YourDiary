# YourDiary 桌面客户端

本客户端使用《你的日记》App 提供的 API，可阅读与搜索日记、查看搭档留言，以及浏览树洞帖子和回复。

作者为 Orlando Vale，Codex 协助开发。

## 安装 v1.0.0

本次发行包适用于 Ubuntu 26.04 的 amd64 桌面系统。在安装包和校验文件所在目录执行：

```sh
sha256sum -c yourdiary_1.0.0_amd64.deb.sha256
sudo apt install ./yourdiary_1.0.0_amd64.deb
```

从应用菜单打开「你的日记」。安装版将数据库放在 `${XDG_DATA_HOME:-$HOME/.local/share}/com.orlando.yourdiary/data/yourdiary.sqlite3`。从源码启动的版本继续使用项目目录下的 `data/`。

在已配置 Rust、GTK 4、libadwaita 开发环境和 dpkg 打包工具的 Ubuntu 主机上，运行 `./packaging/build-deb.sh` 可以重新生成安装包与校验文件，输出目录为 `dist/`。

## Linux 桌面启动器

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

本项目仅用于学习、展示和个人使用。任何使用或分发本项目代码的人，都必须自行了解并遵守该 API 提供方的服务条款。

项目源代码采用 [GNU 通用公共许可证第 3 版（GPL-3.0-only）](LICENSE)。
