use adw::{ActionRow, PreferencesGroup, prelude::*};
use gtk::{Image, Label, gdk, gdk_pixbuf::Pixbuf, glib};

pub(super) fn row() -> ActionRow {
    let row = ActionRow::builder()
        .title("你的日记")
        .subtitle(concat!("桌面客户端 · ", env!("CARGO_PKG_VERSION")))
        .activatable(true)
        .build();
    let bytes = glib::Bytes::from_static(include_bytes!("../../../resources/YourDiary.png"));
    let icon = Pixbuf::from_stream_at_scale(
        &gtk::gio::MemoryInputStream::from_bytes(&bytes),
        128,
        128,
        true,
        gtk::gio::Cancellable::NONE,
    )
    .expect("内置应用图标应为有效的 PNG 图片");
    let logo = gdk::Texture::for_pixbuf(&icon);
    row.add_prefix(&logo_image(&logo, 40));
    row.add_suffix(&Image::from_icon_name("go-next-symbolic"));

    let opened = glib::WeakRef::<adw::Window>::new();
    row.connect_activated(move |row| {
        if let Some(window) = opened.upgrade().filter(WidgetExt::get_visible) {
            window.present();
            return;
        }
        let Some(parent) = row.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let window = details_window(&parent, &logo);
        window.set_application(parent.application().as_ref());
        opened.set(Some(&window));
        window.present();
    });
    row
}

fn logo_image(logo: &gdk::Texture, size: i32) -> Image {
    Image::builder()
        .paintable(logo)
        .pixel_size(size)
        .valign(gtk::Align::Center)
        .build()
}

fn details_window(parent: &gtk::Window, logo: &gdk::Texture) -> adw::Window {
    let page = adw::PreferencesPage::new();
    let intro = PreferencesGroup::new();
    let cover = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    cover.add_css_class("about-cover");
    cover.append(&logo_image(logo, 60));
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 4);
    identity.set_valign(gtk::Align::Center);
    let name = Label::builder()
        .label("你的日记")
        .xalign(0.)
        .css_classes(["title-1"])
        .build();
    identity.append(&name);
    identity.append(
        &Label::builder()
            .label(concat!("桌面客户端 · ", env!("CARGO_PKG_VERSION")))
            .xalign(0.)
            .css_classes(["dim-label"])
            .build(),
    );
    cover.append(&identity);
    intro.add(&cover);
    page.add(&intro);

    let usage = PreferencesGroup::builder().title("关于这个客户端").build();
    for (icon, title, text) in [
        (
            "accessories-text-editor-symbolic",
            "日记与树洞",
            "按日期阅读自己与搭档的日记，搜索正文、互相留言，也可浏览树洞帖子和回复。",
        ),
        (
            "drive-harddisk-symbolic",
            "本地缓存",
            "日记全文、已浏览的树洞内容和外观偏好保存在本机。离线时可以阅读已缓存的内容。",
        ),
        (
            "dialog-information-symbolic",
            "API 与使用条款",
            "本客户端使用《你的日记》App 提供的 API，仅用于学习、展示和个人使用。任何使用或分发本项目代码的人，都必须自行了解并遵守该 API 提供方的服务条款。",
        ),
    ] {
        let row = ActionRow::builder()
            .title(title)
            .subtitle(text)
            .subtitle_lines(0)
            .css_classes(["about-prose"])
            .build();
        row.add_prefix(&Image::from_icon_name(icon));
        usage.add(&row);
    }
    page.add(&usage);

    let author = PreferencesGroup::builder().title("作者").build();
    let github = "https://github.com/kclx";
    author.add(&external_row(
        "Orlando Vale",
        "ljc2481534548@163.com",
        "mailto:ljc2481534548@163.com",
    ));
    author.add(&external_row("GitHub", github, github));
    page.add(&author);

    let software = PreferencesGroup::builder().title("软件信息").build();
    let license = external_row(
        "开源许可",
        "Copyright (c) 2026 YourDiary contributors",
        "https://www.gnu.org/licenses/gpl-3.0.html",
    );
    license.add_suffix(&Label::new(Some(env!("CARGO_PKG_LICENSE"))));
    software.add(&license);
    let system = adw::ExpanderRow::builder().title("运行环境").build();
    for (key, value) in [
        ("应用标识", crate::APP_ID.to_owned()),
        (
            "运行平台",
            format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH),
        ),
        (
            "界面组件",
            format!(
                "GTK {}.{}.{} · libadwaita {}.{}.{}",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version(),
                adw::major_version(),
                adw::minor_version(),
                adw::micro_version()
            ),
        ),
        ("本地存储", format!("SQLite {}", rusqlite::version())),
        ("数据库位置", crate::repo::DATABASE_PATH.to_owned()),
    ] {
        system.add_row(&ActionRow::builder().title(key).subtitle(value).build());
    }
    software.add(&system);
    page.add(&software);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    let window = adw::Window::builder()
        .title("关于你的日记")
        .default_width(510)
        .default_height(660)
        .modal(true)
        .transient_for(parent)
        .destroy_with_parent(true)
        .content(&toolbar)
        .build();
    let controller = super::super::shortcuts::window_controller();
    super::super::shortcuts::bind(&controller, "Escape", gtk::NamedAction::new("window.close"));
    window.add_controller(controller);
    window
}

fn external_row(title: &str, subtitle: &str, uri: &'static str) -> ActionRow {
    let row = ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .activatable(true)
        .build();
    row.add_suffix(&Image::from_icon_name("adw-external-link-symbolic"));
    row.connect_activated(move |row| {
        let parent = row.root().and_downcast::<gtk::Window>();
        gtk::show_uri(parent.as_ref(), uri, gdk::CURRENT_TIME);
    });
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn about_row_opens_one_window_with_build_details() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            crate::view::install_styles();
            let row = row();
            let list = gtk::ListBox::new();
            list.append(&row);
            let host = gtk::Window::builder().child(&list).build();
            host.present();
            ActionRowExt::activate(&row);
            ActionRowExt::activate(&row);
            let dialogs: Vec<_> = gtk::Window::list_toplevels()
                .into_iter()
                .filter_map(|w| w.downcast::<adw::Window>().ok())
                .filter(|w| w.transient_for().as_ref() == Some(&host))
                .collect();
            assert_eq!(dialogs.len(), 1);
            let about = &dialogs[0];
            assert!(about.measure(gtk::Orientation::Horizontal, -1).0 <= 360);
            let mut labels = Vec::new();
            fn collect(w: &gtk::Widget, labels: &mut Vec<glib::GString>) {
                if let Some(label) = w.downcast_ref::<Label>() {
                    assert!(!label.is_selectable(), "关于窗口中的文字应不可选中");
                    labels.push(label.text());
                }
                for child in std::iter::successors(w.first_child(), WidgetExt::next_sibling) {
                    collect(&child, labels);
                }
            }
            collect(about.upcast_ref(), &mut labels);
            for text in [
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_LICENSE"),
                crate::APP_ID,
                "Orlando Vale",
                "ljc2481534548@163.com",
                "https://github.com/kclx",
                "遵守该 API 提供方的服务条款",
            ] {
                assert!(
                    labels.iter().any(|label| label.contains(text)),
                    "缺少 {text}"
                );
            }
            assert!(crate::view::shortcuts::activate(about, "Escape"));
            assert!(host.get_visible());
            host.close();
        });
    }
}
