use crate::service::appearance;
use adw::{ComboRow, StyleManager, prelude::*};
use gtk::{CssProvider, gdk::Display};
use std::{cell::Cell, fmt::Write};

struct Palette {
    key: &'static str,
    name: &'static str,
    light: Colors,
    dark: Colors,
}

#[derive(Clone, Copy)]
struct Colors {
    paper: &'static str,
    sidebar: &'static str,
    card: &'static str,
    ink: &'static str,
    accent: &'static str,
}

const fn colors(
    paper: &'static str,
    sidebar: &'static str,
    card: &'static str,
    ink: &'static str,
    accent: &'static str,
) -> Colors {
    Colors {
        paper,
        sidebar,
        card,
        ink,
        accent,
    }
}

const NATIVE: &str = "系统原色";
const PALETTES: [Palette; 4] = [
    Palette {
        key: "pine",
        name: "松间青",
        light: colors("#f3f6f0", "#e2eadf", "#fbfdf8", "#263c32", "#32674f"),
        dark: colors("#19251f", "#202f27", "#293a30", "#e6f0e4", "#9cd4aa"),
    },
    Palette {
        key: "rose",
        name: "玫瑰笺",
        light: colors("#fbf3f3", "#f1e0e3", "#fffafa", "#482e39", "#95415f"),
        dark: colors("#291e24", "#35262e", "#40303a", "#f5e5ec", "#e9a5bd"),
    },
    Palette {
        key: "tide",
        name: "海盐蓝",
        light: colors("#f1f6fa", "#dfeaf2", "#fafdff", "#273d4b", "#306780"),
        dark: colors("#18252e", "#1e303b", "#293b48", "#e2eef5", "#95ccdf"),
    },
    Palette {
        key: "amber",
        name: "琥珀纸",
        light: colors("#faf5e9", "#efe4cd", "#fffcf3", "#45392b", "#865329"),
        dark: colors("#28231c", "#332c22", "#40372b", "#f4ead9", "#e6bc84"),
    },
];

thread_local! {
    static CURRENT: Cell<usize> = const { Cell::new(0) };
    static STYLES: CssProvider = CssProvider::new();
}

pub(in crate::view) fn restore() {
    let manager = StyleManager::default();
    match appearance::saved_palette() {
        Ok(key) => CURRENT.set(
            PALETTES
                .iter()
                .position(|p| Some(p.key) == key.as_deref())
                .map_or(0, |i| i + 1),
        ),
        Err(error) => eprintln!("无法读取配色：{error}"),
    }
    if let Some(display) = Display::default() {
        STYLES.with(|styles| {
            gtk::style_context_add_provider_for_display(
                &display,
                styles,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            )
        });
    }
    apply(&manager);
    manager.connect_dark_notify(apply);
    manager.connect_high_contrast_notify(apply);
}

fn select(index: u32) -> Result<(), Box<dyn std::error::Error>> {
    let index = index as usize;
    let key = match index {
        0 => "system",
        i => PALETTES.get(i - 1).ok_or("无效的配色")?.key,
    };
    if index != CURRENT.get() {
        appearance::save_palette(key)?;
        CURRENT.set(index);
        apply(&StyleManager::default());
    }
    Ok(())
}

fn apply(manager: &StyleManager) {
    let css = stylesheet(CURRENT.get(), manager.is_dark(), manager.is_high_contrast());
    STYLES.with(|styles| styles.load_from_data(&css));
}

fn stylesheet(selected: usize, dark: bool, high_contrast: bool) -> String {
    let mut css = String::from(
        ".palette-sample { min-width: 42px; min-height: 16px; border-radius: 6px; border: 1px solid alpha(currentColor, 0.25); }\n",
    );
    for (i, p) in PALETTES.iter().enumerate() {
        let Colors {
            paper,
            sidebar,
            card,
            ink,
            accent,
        } = if dark { p.dark } else { p.light };
        writeln!(css, ".palette-sample.{} {{ background-image: linear-gradient(to right, {sidebar} 33%, {paper} 33%, {paper} 66%, {accent} 66%); }}", p.key).unwrap();
        if i + 1 != selected || high_contrast {
            continue;
        }
        let on_accent = if dark { paper } else { "#ffffff" };
        let mut variables = String::new();
        for (name, value) in [
            ("window_bg_color", paper),
            ("window_fg_color", ink),
            ("view_bg_color", paper),
            ("view_fg_color", ink),
            ("sidebar_bg_color", sidebar),
            ("sidebar_backdrop_color", sidebar),
            ("sidebar_fg_color", ink),
            ("headerbar_bg_color", paper),
            ("headerbar_backdrop_color", paper),
            ("headerbar_fg_color", ink),
            ("card_bg_color", card),
            ("card_fg_color", ink),
            ("dialog_bg_color", paper),
            ("dialog_fg_color", ink),
            ("popover_bg_color", card),
            ("popover_fg_color", ink),
            ("accent_bg_color", accent),
            ("accent_color", accent),
            ("accent_fg_color", on_accent),
        ] {
            writeln!(css, "@define-color {name} {value};").unwrap();
            writeln!(variables, "--{}: {value};", name.replace('_', "-")).unwrap();
        }
        // GTK 4.16 起，libadwaita 原生控件也可从 CSS 变量读取配色。
        if gtk::minor_version() >= 16 {
            writeln!(css, ":root {{ {variables} }}").unwrap();
        }
        css.push_str(".diary-bubble.outgoing { background-color: @accent_bg_color; color: @accent_fg_color; }\n.profile-cover { background-image: linear-gradient(115deg, alpha(@accent_color, 0.20), transparent); }\n");
    }
    css
}

pub(super) fn row() -> ComboRow {
    let names: Vec<_> = std::iter::once(NATIVE)
        .chain(PALETTES.iter().map(|p| p.name))
        .collect();
    let row = super::choice("配色", &names, CURRENT.get() as u32, select);
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let name = item
            .item()
            .and_downcast::<gtk::StringObject>()
            .unwrap()
            .string();
        let option = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        if let Some(p) = PALETTES.iter().find(|p| p.name == name.as_str()) {
            let sample = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            sample.set_valign(gtk::Align::Center);
            sample.set_css_classes(&["palette-sample", p.key]);
            option.append(&sample);
        } else {
            option.append(&gtk::Image::from_icon_name("preferences-color-symbolic"));
        }
        option.append(&gtk::Label::new(Some(&name)));
        item.set_child(Some(&option));
    });
    row.set_factory(Some(&factory));
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, process::Command};

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn palette_switches_and_restores_after_restart() {
        // 在独立工作目录启动两个进程，验证重启恢复，也避免动到用户的偏好数据库。
        let Ok(stage) = env::var("YOURDIARY_PALETTE_TEST_STAGE") else {
            let dir = env::temp_dir().join(format!("yourdiary-palette-{}", std::process::id()));
            fs::create_dir(&dir).unwrap();
            for stage in ["select", "restore"] {
                let status = Command::new(env::current_exe().unwrap())
                    .args(["--ignored", "--exact", "view::preferences::palette::tests::palette_switches_and_restores_after_restart"])
                    .env("YOURDIARY_PALETTE_TEST_STAGE", stage)
                    .env("XDG_DATA_HOME", dir.join("xdg"))
                    .current_dir(&dir).status().unwrap();
                assert!(status.success());
            }
            fs::remove_dir_all(dir).unwrap();
            return;
        };
        gtk::test_synced(move || {
            adw::init().unwrap();
            appearance::restore();
            restore();
            let choice = row();
            let group = adw::PreferencesGroup::new();
            group.add(&choice);
            let font_row = super::super::font_row();
            group.add(&font_row);
            let window = adw::Window::builder().content(&group).build();
            window.present();
            let ctx = window.style_context();
            let manager = StyleManager::default();
            if stage == "select" {
                assert_eq!(choice.selected(), 0);
                assert_eq!(font_row.subtitle().as_deref(), Some("跟随系统"));
                appearance::select_font(Some("Serif 19")).unwrap();
                assert_eq!(
                    group.settings().gtk_font_name().as_deref(),
                    Some("Serif 19")
                );
                for (i, p) in PALETTES.iter().enumerate() {
                    choice.set_selected(i as u32 + 1);
                    for (scheme, colors) in [
                        (adw::ColorScheme::ForceLight, p.light),
                        (adw::ColorScheme::ForceDark, p.dark),
                    ] {
                        manager.set_color_scheme(scheme);
                        for (token, value) in [
                            ("accent_color", colors.accent),
                            ("card_bg_color", colors.card),
                            ("window_bg_color", colors.paper),
                        ] {
                            assert_eq!(
                                ctx.lookup_color(token).unwrap(),
                                gtk::gdk::RGBA::parse(value).unwrap()
                            );
                        }
                        assert_eq!(ctx.color(), gtk::gdk::RGBA::parse(colors.ink).unwrap());
                    }
                }
                appearance::select(1).unwrap();
            } else {
                assert_eq!(font_row.subtitle().as_deref(), Some("Serif 19"));
                let font = group.pango_context().font_description().unwrap();
                assert_eq!(font.family().as_deref(), Some("Serif"));
                assert!(font.size() >= 19 * gtk::pango::SCALE);
                appearance::select_font(None).unwrap();
                assert_eq!(appearance::saved_font().unwrap(), None);
                assert_ne!(
                    group.settings().gtk_font_name().as_deref(),
                    Some("Serif 19")
                );
                assert_eq!(choice.selected(), 4);
                assert!(!manager.is_dark());
                assert_eq!(
                    ctx.lookup_color("window_bg_color").unwrap(),
                    gtk::gdk::RGBA::parse(PALETTES[3].light.paper).unwrap()
                );
                choice.set_selected(0);
                assert_ne!(
                    ctx.lookup_color("window_bg_color").unwrap(),
                    gtk::gdk::RGBA::parse(PALETTES[3].light.paper).unwrap()
                );
                assert_eq!(
                    appearance::saved_palette().unwrap().as_deref(),
                    Some("system")
                );
            }
            window.close();
        });
    }
}
