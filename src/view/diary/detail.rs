use std::{cell::Cell, rc::Rc, sync::Arc};

use adw::{Clamp, NavigationPage, ToolbarView};
use gtk::{
    ActionBar, Box, Button, CallbackAction, Label,
    Orientation::Vertical,
    PolicyType, ScrolledWindow, gio,
    glib::{MainContext, Propagation, clone::Downgrade},
    pango::WrapMode,
    prelude::{AdjustmentExt, BoxExt, ButtonExt, WidgetExt},
};

use crate::service::{
    auth::Session,
    diary::{DiaryEntry, adjacent_index, mark_read, open_diary},
};

use super::super::shortcuts;
use super::{apply_gender_color, metadata};

/// 用打开详情时的日记列表创建界面。
pub(super) fn build(
    entries: Rc<Vec<DiaryEntry>>,
    positions: Vec<usize>,
    position: usize,
    session: Option<Arc<Session>>,
    account_id: Option<i64>,
    refresh_row: impl Fn(usize) + 'static,
) -> Option<NavigationPage> {
    entries.get(*positions.get(position)?)?;
    let total = positions.len();
    let positions: Rc<[usize]> = positions.into();
    let current_position = Rc::new(Cell::new(position));
    let load_generation = Rc::new(Cell::new(0_u64));
    let refresh_row = Rc::new(refresh_row);

    let title = Label::new(None);
    title.add_css_class("title-2");

    let metadata_label = Label::new(None);
    metadata_label.add_css_class("diary-meta");

    let detail_context = Label::new(None);
    detail_context.add_css_class("diary-prose");
    detail_context.set_margin_start(8);
    detail_context.set_margin_end(8);
    for label in [&title, &metadata_label, &detail_context] {
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(WrapMode::WordChar);
    }

    let heading = Box::new(Vertical, 8);
    heading.add_css_class("card");
    heading.add_css_class("diary-detail-heading");
    heading.append(&metadata_label);
    heading.append(&title);

    let body = Box::new(Vertical, 24);
    body.set_margin_top(24);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);
    body.append(&crate::view::navigation::back_button("返回日记"));
    body.append(&heading);
    body.append(&detail_context);
    let comments = session
        .as_ref()
        .map(|s| super::comments::Comments::new(s.clone(), account_id));
    if let Some(panel) = &comments {
        body.append(&panel.widget);
    }

    let reading_column = Clamp::builder().maximum_size(680).child(&body).build();
    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&reading_column)
        .build();
    let scroll_position = scroller.vadjustment();

    let previous_button = Button::with_label("上一篇");
    let next_button = Button::with_label("下一篇");
    previous_button.add_css_class("pill");
    next_button.add_css_class("pill");
    let action_bar = ActionBar::new();
    action_bar.pack_start(&previous_button);
    action_bar.pack_end(&next_button);
    if let Some(panel) = &comments {
        action_bar.set_center_widget(Some(&panel.composer));
    }

    let previous_weak = previous_button.downgrade();
    let next_weak = next_button.downgrade();
    let render = move |position: usize, refresh: bool| {
        let index = positions[position];
        let entry = &entries[index];
        let generation = load_generation.get() + 1;
        load_generation.set(generation);
        let text = entry.title.trim();
        title.set_text(if text.is_empty() {
            "无标题日记"
        } else {
            text
        });
        if !refresh {
            detail_context.set_text(if entry.content.trim().is_empty() {
                "这篇日记没有正文"
            } else {
                &entry.content
            });
        }
        metadata_label.set_text(&metadata(entry));
        apply_gender_color(&heading, entry.gender);
        if !refresh {
            scroll_position.set_value(0.0);
        }
        if let Some(previous) = previous_weak.upgrade() {
            previous.set_sensitive(adjacent_index(position, -1, total).is_some());
        }
        if let Some(next) = next_weak.upgrade() {
            next.set_sensitive(adjacent_index(position, 1, total).is_some());
        }

        let Some(session) = session.clone() else {
            return;
        };
        if let Some(panel) = &comments {
            panel.open(entry);
        }
        let diary_id = entry.id;
        if !entry.has_full_content && !refresh {
            detail_context.set_text("正在加载日记全文…");
        }
        let content_weak = detail_context.downgrade();
        let metadata_weak = metadata_label.downgrade();
        let load_generation = load_generation.clone();
        let entries = entries.clone();
        let refresh_row = refresh_row.clone();
        let snapshot = entry.clone();
        MainContext::default().spawn_local(async move {
            let fetch_session = session.clone();
            let result = gio::spawn_blocking(move || {
                open_diary(&fetch_session, account_id, &snapshot, refresh)
            })
            .await;
            let entry = &entries[index];
            let is_current = || load_generation.get() == generation;
            let Some(body) = content_weak.upgrade().filter(|_| is_current()) else {
                return;
            };
            let Ok(Some(opened)) = result else {
                body.set_text(&format!("全文加载失败\n\n{}", entry.content).trim_end());
                return;
            };
            let full = opened.full;
            if !opened.current {
                body.set_text(
                    format!("全文未更新 · 以下可能是旧版本\n\n{}", full.content).trim_end(),
                );
                return;
            }
            body.set_text(if full.content.trim().is_empty() {
                "这篇日记没有正文"
            } else {
                &full.content
            });
            drop(body);
            if !entry.has_full_content {
                refresh_row(index);
            }

            let previous_read = entry.read.get();
            if !entry.needs_read_mark(full.revision) {
                return;
            }
            if entry.read.get() != previous_read {
                refresh_row(index);
                if let Some(label) = metadata_weak.upgrade() {
                    label.set_text(&metadata(entry));
                }
            }
            let revision = entry.revision.get();
            let author = entry.author_id;
            let result = gio::spawn_blocking(move || {
                mark_read(&session, diary_id, account_id, author, revision)
            })
            .await;
            if entry.revision.get() != revision {
                return;
            }
            let confirmed = matches!(result, Ok(Ok(())));
            if confirmed {
                entry.read.set(Some(true));
                refresh_row(index);
            }
            if let Some(label) = metadata_weak.upgrade().filter(|_| is_current()) {
                let mut status = metadata(entry);
                if !confirmed {
                    status.push_str(" · 已读标记失败");
                }
                label.set_text(&status);
            }
        });
    };
    render(position, false);
    for (button, offset) in [(&previous_button, -1), (&next_button, 1)] {
        let current_position = current_position.clone();
        let render = render.clone();
        button.connect_clicked(move |_| {
            if let Some(position) = adjacent_index(current_position.get(), offset, total) {
                current_position.set(position);
                render(position, false);
            }
        });
    }

    // ToolbarView 管理正文与底部操作栏；垂直扩展使操作栏位于窗口底部。
    let toolbar_view = ToolbarView::new();
    toolbar_view.set_content(Some(&scroller));
    toolbar_view.add_bottom_bar(&action_bar);
    toolbar_view.set_vexpand(true);

    shortcuts::on_refresh(&toolbar_view, move || render(current_position.get(), true));
    let detail = NavigationPage::new(&toolbar_view, "日记详情");
    let controller = gtk::ShortcutController::new();
    for (keys, button) in [
        (
            &["<Control>minus", "<Control>KP_Subtract"][..],
            &previous_button,
        ),
        (
            &[
                "<Control>plus",
                "<Control>equal",
                "<Control><Shift>equal",
                "<Control>KP_Add",
            ][..],
            &next_button,
        ),
    ] {
        let button = button.downgrade();
        let action = CallbackAction::new(move |_, _| {
            let Some(button) = button.upgrade().filter(WidgetExt::is_sensitive) else {
                return Propagation::Proceed;
            };
            button.emit_clicked();
            Propagation::Stop
        });
        for key in keys {
            shortcuts::bind(&controller, key, action.clone());
        }
    }
    detail.add_controller(controller);
    Some(detail)
}
