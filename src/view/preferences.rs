use crate::service::appearance;
use adw::{ComboRow, NavigationPage, PreferencesGroup, PreferencesPage, ToolbarView, prelude::*};
use gtk::StringList;

mod about;
pub(super) mod export;
pub(super) mod palette;

pub(super) fn page() -> NavigationPage {
    let group = PreferencesGroup::builder().title("外观").build();
    group.add(&choice(
        "主题",
        &appearance::labels(),
        appearance::selected(),
        appearance::select,
    ));
    group.add(&palette::row());
    group.add(&font_row());
    let page = PreferencesPage::new();
    page.add(&group);
    let data = PreferencesGroup::builder().title("数据").build();
    let export = adw::ActionRow::builder()
        .title("导出我的日记")
        .activatable(true)
        .action_name("win.export-diaries")
        .build();
    export.add_prefix(&gtk::Image::from_icon_name("document-save-as-symbolic"));
    data.add(&export);
    page.add(&data);
    let account = PreferencesGroup::builder().title("账号").build();
    let logout = adw::ActionRow::builder()
        .title("退出登录")
        .subtitle("退出后需重新登录，未保存的草稿将丢失。")
        .activatable(true)
        .action_name("win.logout")
        .build();
    logout.add_prefix(&gtk::Image::from_icon_name("system-log-out-symbolic"));
    account.add(&logout);
    page.add(&account);
    let about = PreferencesGroup::builder().title("关于").build();
    about.add(&about::row());
    page.add(&about);
    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    NavigationPage::new(&toolbar, "偏好设置")
}

fn font_row() -> adw::ActionRow {
    let saved = appearance::saved_font();
    let row = adw::ActionRow::builder()
        .title("字体与字号")
        .subtitle(match &saved {
            Ok(font) => font.as_deref().unwrap_or("跟随系统"),
            Err(_) => "读取字体设置失败",
        })
        .use_markup(false)
        .activatable(true)
        .build();
    let reset = gtk::Button::builder()
        .label("跟随系统")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .visible(matches!(saved, Ok(Some(_))))
        .build();
    row.add_suffix(&reset);
    row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    let weak = row.downgrade();
    reset.connect_clicked(move |button| {
        if let Some(row) = weak.upgrade() {
            save_font(&row, button, None);
        }
    });
    row.connect_activated(move |row| {
        let Some(parent) = row.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let initial = row
            .settings()
            .gtk_font_name()
            .map(|name| gtk::pango::FontDescription::from_string(&name));
        let dialog = gtk::FontDialog::builder()
            .title("选择字体与字号")
            .modal(true)
            .build();
        let row = row.clone();
        let reset = reset.clone();
        dialog.choose_font(
            Some(&parent),
            initial.as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| match result {
                Ok(font) => save_font(&row, &reset, Some(font.to_string().as_str())),
                Err(err)
                    if err.matches(gtk::DialogError::Dismissed)
                        || err.matches(gtk::DialogError::Cancelled) => {}
                Err(err) => {
                    eprintln!("无法打开字体选择器：{err}");
                    row.set_subtitle("选择字体失败，请重试");
                }
            },
        );
    });
    row
}

fn save_font(row: &adw::ActionRow, reset: &gtk::Button, font: Option<&str>) {
    match appearance::select_font(font) {
        Ok(()) => {
            row.set_subtitle(font.unwrap_or("跟随系统"));
            reset.set_visible(font.is_some());
        }
        Err(error) => {
            eprintln!("无法保存字体设置：{error}");
            row.set_subtitle("保存失败，请重试");
        }
    }
}

fn choice(
    title: &str,
    names: &[&str],
    selected: u32,
    save: impl Fn(u32) -> Result<(), Box<dyn std::error::Error>> + 'static,
) -> ComboRow {
    let row = ComboRow::builder()
        .title(title)
        .model(&StringList::new(names))
        .selected(selected)
        .build();
    let last = std::cell::Cell::new(selected);
    row.connect_selected_notify(move |row| {
        let selected = row.selected();
        if selected != last.get() {
            match save(selected) {
                Ok(()) => {
                    last.set(selected);
                    row.set_subtitle("");
                }
                Err(error) => {
                    eprintln!("无法保存外观设置：{error}");
                    row.set_subtitle("保存失败，请重试");
                    row.set_selected(last.get());
                }
            }
        }
    });
    row
}
