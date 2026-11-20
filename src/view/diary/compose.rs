use crate::service::diary::{
    DiaryWrite, TodayDiary, WEATHER, WriteError, default_mood_name, today, weather_name,
};
use adw::{ActionRow, ComboRow, NavigationPage, PreferencesGroup, prelude::*};
use gtk::{Box, Button, Label, Orientation::Vertical, StringList, gio, glib};
use std::{cell::Cell, rc::Rc, sync::Arc};

pub(super) fn page(
    date: String,
    dirty: Rc<Cell<bool>>,
    load: impl FnOnce(&str) -> Result<TodayDiary, &'static str> + Send + 'static,
    write: impl Fn(&DiaryWrite, DiaryWrite) -> Result<i64, WriteError> + Send + Sync + 'static,
    saved: impl Fn() + 'static,
) -> NavigationPage {
    let loading = Box::new(Vertical, 8);
    loading.append(&crate::view::navigation::back_button("返回日记"));
    let message = Label::new(Some("正在读取今天的日记…"));
    loading.append(&message);
    let page = NavigationPage::new(&loading, "写日记");
    page.set_tag(Some(&date));
    // NavigationView uses its own direction for slides. Keep the form in its reading direction.
    page.connect_showing(|page| {
        if let Some(nav) = page.parent().and_downcast::<adw::NavigationView>() {
            nav.set_direction(gtk::TextDirection::Rtl);
        }
    });
    page.connect_hidden(|page| {
        if let Some(nav) = page.parent().and_downcast::<adw::NavigationView>() {
            nav.set_direction(gtk::TextDirection::None);
        }
    });
    let weak = page.downgrade();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || load(&date))
            .await
            .unwrap_or(Err("读取失败，请返回后重试"));
        let Some(page) = weak.upgrade() else { return };
        match result {
            Ok(diary) if diary.entry.date == today() => {
                install_form(&page, diary, dirty, write, saved)
            }
            result => {
                page.set_tag(None);
                message.set_text(result.err().unwrap_or("日期已变更，请返回后重新打开"));
            }
        }
    });
    page
}

fn install_form(
    page: &NavigationPage,
    diary: TodayDiary,
    dirty: Rc<Cell<bool>>,
    write: impl Fn(&DiaryWrite, DiaryWrite) -> Result<i64, WriteError> + Send + Sync + 'static,
    saved: impl Fn() + 'static,
) {
    let TodayDiary {
        entry: original,
        mut moods,
        mood_name,
    } = diary;
    let column = Box::builder()
        .orientation(Vertical)
        .spacing(14)
        .margin_top(12)
        .margin_bottom(20)
        .margin_start(18)
        .margin_end(18)
        .build();
    let title = adw::EntryRow::builder().title("标题（选填）").build();
    title.set_text(&original.title);
    let date_row = ActionRow::builder().title("日期").build();
    date_row.add_suffix(&Label::new(Some(&original.date)));
    let fields = PreferencesGroup::new();
    fields.add(&title);
    fields.add(&date_row);
    let mut weather_keys: Vec<_> = WEATHER.iter().copied().map(str::to_owned).collect();
    if let Some(key) = &original.weather
        && !weather_keys.contains(key)
    {
        weather_keys.push(key.clone());
    }
    let weather_names: Vec<_> = std::iter::once("未选择")
        .chain(weather_keys.iter().map(|key| weather_name(key)))
        .collect();
    let weather = ComboRow::builder()
        .title("天气")
        .model(&StringList::new(&weather_names))
        .build();
    if moods.is_empty() {
        moods.extend((10006..=10010).map(|id| (id, default_mood_name(id).to_owned())));
    }
    if let Some(id) = original.mood_id
        && !moods.iter().any(|(key, _)| *key == id)
    {
        moods.push((id, mood_name.clone()));
    }
    let no_mood = if original.mood_id.is_none() && !mood_name.is_empty() {
        &mood_name
    } else {
        "未选择"
    };
    let mood_names: Vec<_> = std::iter::once(no_mood)
        .chain(moods.iter().map(|(_, name)| name.as_str()))
        .collect();
    let mood = ComboRow::builder()
        .title("心情")
        .model(&StringList::new(&mood_names))
        .build();
    if let Some(index) = weather_keys
        .iter()
        .position(|key| Some(key) == original.weather.as_ref())
    {
        weather.set_selected(index as u32 + 1);
    }
    if let Some(index) = moods
        .iter()
        .position(|(id, _)| Some(*id) == original.mood_id)
    {
        mood.set_selected(index as u32 + 1);
    }
    fields.add(&weather);
    fields.add(&mood);
    column.append(&fields);
    let body = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .accepts_tab(false)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(14)
        .right_margin(14)
        .vexpand(true)
        .css_classes(["card", "diary-prose", "diary-compose-paper"])
        .build();
    let shortcuts = gtk::ShortcutController::new();
    shortcuts.set_propagation_phase(gtk::PropagationPhase::Capture);
    crate::view::shortcuts::bind(
        &shortcuts,
        "<Control>t",
        gtk::CallbackAction::new(|widget, _| {
            let body = widget.downcast_ref::<gtk::TextView>().unwrap();
            let Ok(time) =
                glib::DateTime::now_local().and_then(|now| now.format("记录于%Y年%m月%d日 %H:%M"))
            else {
                return glib::Propagation::Proceed;
            };
            let buffer = body.buffer();
            buffer.begin_user_action();
            buffer.delete_selection(true, body.is_editable());
            body.emit_insert_at_cursor(&time);
            buffer.end_user_action();
            glib::Propagation::Stop
        }),
    );
    body.add_controller(shortcuts);
    let heading = Label::builder()
        .label("正文")
        .tooltip_text("Ctrl+T 插入当前时间")
        .xalign(0.)
        .css_classes(["heading"])
        .build();
    heading.set_mnemonic_widget(Some(&body));
    column.append(&heading);
    column.append(
        &gtk::ScrolledWindow::builder()
            .min_content_height(180)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .build(),
    );
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(
            &adw::Clamp::builder()
                .maximum_size(700)
                .child(&column)
                .build(),
        )
        .build();

    let root = Box::new(Vertical, 0);
    let actions = Box::builder()
        .spacing(10)
        .margin_start(16)
        .margin_end(16)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    actions.append(&crate::view::navigation::back_button("返回日记"));
    let status = Label::builder()
        .hexpand(true)
        .wrap(true)
        .xalign(1.)
        .css_classes(["caption", "error"])
        .build();
    actions.append(&status);
    let save = Button::builder()
        .label("保存日记")
        .css_classes(["suggested-action", "pill"])
        .sensitive(false)
        .valign(gtk::Align::Center)
        .build();
    actions.append(&save);
    root.append(&actions);
    root.append(&scroller);
    page.set_child(Some(&root));
    let buffer = body.buffer();
    buffer.connect_changed(glib::clone!(
        #[weak]
        save,
        move |b| {
            save.set_sensitive(b.char_count() > 0);
        }
    ));
    let page_weak = page.downgrade();
    buffer.set_text(&original.content);
    let changed = dirty.clone();
    buffer.connect_changed(move |_| changed.set(true));
    let changed = dirty.clone();
    title.connect_changed(move |_| changed.set(true));
    for field in [&weather, &mood] {
        let changed = dirty.clone();
        field.connect_selected_notify(move |_| changed.set(true));
    }
    let original = Arc::new(original);
    let write = Arc::new(write);
    let saved = Rc::new(saved);
    save.connect_clicked(move |save| {
        if !save.is_sensitive() {
            return;
        }
        let content = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        if content.trim().is_empty() {
            status.set_text("请先填写正文");
            body.grab_focus();
            return;
        }
        let entry = DiaryWrite {
            title: title.text().into(),
            content: content.into(),
            extras: original.extras.clone(),
            id: original.id,
            date: original.date.clone(),
            weather: weather
                .selected()
                .checked_sub(1)
                .and_then(|i| weather_keys.get(i as usize))
                .cloned(),
            mood_id: mood
                .selected()
                .checked_sub(1)
                .and_then(|i| moods.get(i as usize))
                .map(|(id, _)| *id),
        };
        status.set_text("");
        save.set_sensitive(false);
        save.set_label("保存中…");
        column.set_sensitive(false);
        let (write, original) = (write.clone(), original.clone());
        let (saved, page) = (saved.clone(), page_weak.clone());
        let (save, status, column) = (save.downgrade(), status.downgrade(), column.downgrade());
        let (body, title) = (body.downgrade(), title.downgrade());
        let dirty = dirty.clone();
        glib::spawn_future_local(async move {
            match gio::spawn_blocking(move || write(&original, entry))
                .await
                .unwrap_or(Err(WriteError::Retry("未能确认保存结果，请先刷新日记列表")))
            {
                Ok(_) => {
                    dirty.set(false);
                    if let Some(page) = page.upgrade() {
                        page.set_tag(None);
                        if let Some(nav) = page
                            .ancestor(adw::NavigationView::static_type())
                            .and_downcast::<adw::NavigationView>()
                            && nav.visible_page().as_ref() == Some(&page)
                        {
                            nav.pop();
                        }
                    }
                    saved();
                }
                Err(error) => {
                    let retry = matches!(error, WriteError::Retry(_));
                    let message = match error {
                        WriteError::Retry(msg) => msg,
                        WriteError::Reload(msg) => {
                            if let Some(page) = page.upgrade() {
                                page.set_tag(None);
                            }
                            if let Some(body) = body.upgrade() {
                                body.set_editable(false);
                            }
                            if let Some(title) = title.upgrade() {
                                title.set_editable(false);
                            }
                            msg
                        }
                    };
                    if let (Some(save), Some(status), Some(column)) =
                        (save.upgrade(), status.upgrade(), column.upgrade())
                    {
                        status.set_text(message);
                        save.set_label("保存日记");
                        save.set_sensitive(retry);
                        column.set_sensitive(true);
                    }
                }
            }
        });
    });
}
