use super::{list::show_entries, search};
use crate::service::{
    auth::{Session, SessionStore},
    diary::{SyncError, SyncEvent, start_sync},
};
use adw::prelude::{MessageDialogExt, NavigationPageExt};
use gtk::{
    Box, Label, ListView,
    Orientation::Vertical,
    PolicyType, ProgressBar, ScrolledWindow, SearchEntry,
    glib::{
        self,
        ControlFlow::{Break, Continue},
        timeout_add_local,
    },
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        mpsc::{Receiver, TryRecvError},
    },
    time::Duration,
};

/// 构建日记页，根据后台同步结果更新列表和进度。
pub(in crate::view) fn build(
    session: Arc<Session>,
    store: SessionStore,
    on_profile: impl Fn(Option<&crate::service::profile::Profile>) + 'static,
    on_unauthorized: impl Fn() + 'static,
) -> (adw::NavigationView, SearchEntry) {
    let sync_status = Label::builder()
        .label("正在同步日记…")
        .wrap(true)
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["diary-meta"])
        .build();
    let diary_list = ListView::builder().single_click_activate(true).build();
    let scroller = scrollable(&diary_list);
    let search = SearchEntry::builder()
        .placeholder_text("搜索日记标题或正文")
        .margin_start(28)
        .margin_end(28)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    let search_status = Label::builder()
        .xalign(0.0)
        .wrap(true)
        .margin_start(28)
        .margin_end(28)
        .css_classes(["diary-meta"])
        .visible(false)
        .build();
    let progress_bar = ProgressBar::builder()
        .margin_start(28)
        .margin_end(28)
        .build();
    let listen = watch_sync(
        &diary_list,
        &sync_status,
        &progress_bar,
        (&search, &search_status),
        Some(session.clone()),
        on_profile,
        on_unauthorized,
    );

    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("同步日记（Ctrl+R）")
        .action_name(crate::view::shortcuts::REFRESH)
        .css_classes(["flat", "circular"])
        .build();
    progress_bar
        .bind_property("visible", &refresh_button, "sensitive")
        .invert_boolean()
        .sync_create()
        .build();
    let summary = Box::builder()
        .spacing(8)
        .margin_start(28)
        .margin_end(24)
        .margin_top(8)
        .build();
    let grip = Box::new(Vertical, 0);
    grip.add_css_class("diary-compose-grip");
    let write = gtk::Button::builder()
        .child(&grip)
        .tooltip_text("写日记（Ctrl+N）")
        .action_name(crate::view::shortcuts::COMPOSE)
        .halign(gtk::Align::Start)
        .width_request(24)
        .margin_top(6)
        .margin_bottom(32)
        .overflow(gtk::Overflow::Hidden)
        .css_classes(["diary-compose-tab"])
        .build();
    summary.append(&sync_status);
    summary.append(&refresh_button);
    let page = Box::new(Vertical, 0);
    page.append(&summary);
    page.append(&progress_bar);
    page.append(&search);
    page.append(&search_status);
    let list_overlay = gtk::Overlay::new();
    list_overlay.set_child(Some(&scroller));
    list_overlay.add_overlay(&write);
    page.append(&list_overlay);
    let bar = progress_bar.downgrade();
    progress_bar.set_visible(false);
    let writer = session.clone();
    let refresh = move || {
        let Some(bar) = bar.upgrade() else { return };
        if !bar.get_visible() {
            bar.set_fraction(0.);
            bar.set_visible(true);
            listen(start_sync(session.clone(), store.clone()));
        }
    };
    refresh();
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&page));
    crate::view::shortcuts::on_refresh(&overlay, refresh);
    let queued = Rc::new(Cell::new(false));
    progress_bar.connect_visible_notify(gtk::glib::clone!(
        #[weak]
        overlay,
        #[strong]
        queued,
        move |bar| {
            if !bar.get_visible() && queued.replace(false) {
                gtk::glib::idle_add_local_once(move || {
                    let _ = overlay.activate_action(crate::view::shortcuts::REFRESH, None);
                });
            }
        }
    ));
    let nav = crate::view::navigation::section(&overlay, "日记");
    let overlay_weak = overlay.downgrade();
    install_compose(&nav, move |date, dirty| {
        let writer = writer.clone();
        let reader = writer.clone();
        let queued = queued.clone();
        let bar = progress_bar.downgrade();
        let overlay = overlay_weak.clone();
        super::compose::page(
            date,
            dirty,
            move |date| crate::service::diary::load_today(&reader, date),
            move |original, entry| crate::service::diary::save_today(&writer, original, entry),
            move || {
                if let Some(overlay) = overlay.upgrade() {
                    overlay.add_toast(adw::Toast::new("日记已保存"));
                    if bar.upgrade().is_some_and(|bar| bar.get_visible()) {
                        queued.set(true);
                    } else {
                        let _ = overlay.activate_action(crate::view::shortcuts::REFRESH, None);
                    }
                }
            },
        )
    });
    (nav, search)
}

pub(super) fn install_compose(
    nav: &adw::NavigationView,
    create: impl Fn(String, Rc<Cell<bool>>) -> adw::NavigationPage + 'static,
) {
    let nav_weak = nav.downgrade();
    let editor = RefCell::new(None::<(adw::NavigationPage, Rc<Cell<bool>>)>);
    let compose = gtk::gio::SimpleAction::new("compose", None);
    compose.connect_activate(move |_, _| {
        let Some(nav) = nav_weak.upgrade() else {
            return;
        };
        let mut draft_page = editor.borrow_mut();
        let date = crate::service::diary::today();
        if let Some((page, dirty)) = draft_page.as_ref()
            && page.tag().as_deref() != Some(&date)
        {
            if dirty.get() {
                let parent = nav.root().and_downcast::<gtk::Window>();
                let dialog = adw::MessageDialog::new(
                    parent.as_ref(),
                    Some("保留未保存的日记？"),
                    Some("可以返回草稿复制内容，或放弃草稿后打开今天的日记。"),
                );
                dialog.add_response("keep", "查看草稿");
                dialog.add_response("discard", "放弃草稿");
                dialog.set_default_response(Some("keep"));
                dialog.set_close_response("keep");
                dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
                let (nav, page, dirty) = (nav.downgrade(), page.clone(), dirty.clone());
                dialog.connect_response(None, move |_, response| {
                    let Some(nav) = nav.upgrade() else { return };
                    if response == "discard" {
                        dirty.set(false);
                        let _ = nav.activate_action(crate::view::shortcuts::COMPOSE, None);
                    } else if nav.visible_page().as_ref() != Some(&page) {
                        nav.push(&page);
                    }
                });
                dialog.present();
                return;
            }
            if nav.visible_page().as_ref() == Some(page) {
                nav.pop();
            }
            draft_page.take();
        }
        let (page, _) = draft_page.get_or_insert_with(|| {
            let dirty = Rc::new(Cell::new(false));
            let page = create(date, dirty.clone());
            (page, dirty)
        });
        if nav.visible_page().as_ref() != Some(page) {
            nav.push(page);
        }
    });
    let actions = gtk::gio::SimpleActionGroup::new();
    actions.add_action(&compose);
    nav.insert_action_group("diary", Some(&actions));
}

pub(super) fn watch_sync<
    P: Fn(Option<&crate::service::profile::Profile>) + 'static,
    U: Fn() + 'static,
>(
    list: &ListView,
    status: &Label,
    progress: &ProgressBar,
    search: (&SearchEntry, &Label),
    session: Option<Arc<Session>>,
    on_profile: P,
    on_unauthorized: U,
) -> impl Fn(Receiver<SyncEvent>) + use<P, U> {
    let widgets = (
        status.downgrade(),
        list.downgrade(),
        progress.downgrade(),
        search.0.downgrade(),
        search.1.downgrade(),
    );
    search.0.connect_stop_search(|entry| entry.set_text(""));
    let handlers = std::rc::Rc::new(std::cell::RefCell::new(None));
    let on_profile = std::rc::Rc::new(on_profile);
    let on_unauthorized = std::rc::Rc::new(on_unauthorized);
    move |receiver| {
        let (status_weak, list_weak, progress_weak, search_weak, search_status_weak) =
            widgets.clone();
        if let Some(status) = status_weak.upgrade() {
            status.set_text("正在同步日记…");
        }
        let (session, handlers, on_profile, on_unauthorized) = (
            session.clone(),
            handlers.clone(),
            on_profile.clone(),
            on_unauthorized.clone(),
        );
        let mut diary_count = 0;
        let mut has_total = false;
        timeout_add_local(Duration::from_millis(100), move || {
            let (Some(list), Some(status), Some(bar), Some(search), Some(search_status)) = (
                list_weak.upgrade(),
                status_weak.upgrade(),
                progress_weak.upgrade(),
                search_weak.upgrade(),
                search_status_weak.upgrade(),
            ) else {
                return Break;
            };
            loop {
                let event = receiver.try_recv();
                if list.model().is_none()
                    && matches!(
                        event,
                        Ok(SyncEvent::List(Err(_))) | Err(TryRecvError::Disconnected)
                    )
                {
                    on_profile(session.as_ref().and_then(|s| s.profile()));
                }
                let status_text = match event {
                    Ok(SyncEvent::List(Ok(result))) => {
                        on_profile(result.profile.as_ref());
                        diary_count = result.entries.len();
                        let scroll = list.vadjustment().map(|adj| {
                            let y = adj.value();
                            (adj, y)
                        });
                        if let Some((activate, changed)) = handlers.take() {
                            list.disconnect(activate);
                            search.disconnect(changed);
                        }
                        handlers.replace(Some(show_entries(
                            &list,
                            result.entries,
                            session.clone(),
                            result.account_id,
                            (&search, &search_status),
                        )));
                        if !search.text().trim().is_empty() {
                            search.emit_by_name::<()>("search-changed", &[]);
                        }
                        if let Some((adjustment, y)) = scroll.filter(|(_, y)| *y > 0.) {
                            list.add_tick_callback(move |_, _| {
                                adjustment.set_value(y);
                                Break
                            });
                        }
                        status.set_text("正在检查日记缓存…");
                        continue;
                    }
                    Ok(SyncEvent::Cached) => format!("{diary_count} 篇日记 · 上次同步的缓存"),
                    Ok(SyncEvent::Progress(progress)) => {
                        has_total = true;
                        let fraction = if progress.total == 0 {
                            1.0
                        } else {
                            progress.completed as f64 / progress.total as f64
                        };
                        bar.set_fraction(fraction);
                        status.set_text(&format!(
                            "正在同步正文 · {}/{}",
                            progress.completed, progress.total
                        ));
                        continue;
                    }
                    Ok(SyncEvent::Complete(Some(progress)))
                        if progress.completed == progress.total =>
                    {
                        if diary_count == 0 {
                            "暂时没有日记".into()
                        } else {
                            format!("{diary_count} 篇日记")
                        }
                    }
                    Ok(SyncEvent::Complete(Some(progress))) => {
                        format!(
                            "{diary_count} 篇日记 · {} 篇全文待同步",
                            progress.total - progress.completed
                        )
                    }
                    Ok(SyncEvent::Complete(None)) => {
                        format!("{diary_count} 篇日记 · 全文缓存未完成")
                    }
                    Ok(SyncEvent::List(Err(SyncError::Rejected))) => "同步未成功".into(),
                    Ok(SyncEvent::List(Err(SyncError::MissingDiaries))) => {
                        "同步响应缺少日记列表".into()
                    }
                    Ok(SyncEvent::List(Err(SyncError::InvalidResponse))) => {
                        "同步响应解析失败".into()
                    }
                    Ok(SyncEvent::List(Err(SyncError::RequestFailed))) => "同步请求失败".into(),
                    Ok(SyncEvent::List(Err(SyncError::Unauthorized))) => {
                        bar.set_visible(false);
                        on_unauthorized();
                        return Break;
                    }
                    Err(TryRecvError::Empty) => {
                        if !has_total {
                            bar.pulse();
                        }
                        return Continue;
                    }
                    Err(TryRecvError::Disconnected) => "同步请求未完成".into(),
                };
                bar.set_visible(false);
                status.set_text(&status_text);
                if !search.text().trim().is_empty() {
                    search.emit_by_name::<()>("search-changed", &[]);
                } else {
                    search::rebind_rows(Some(list));
                }
                return Break;
            }
        });
    }
}

pub(super) fn scrollable(list: &ListView) -> ScrolledWindow {
    list.set_margin_start(24);
    list.set_margin_end(24);
    ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vexpand(true)
        .margin_top(4)
        .child(list)
        .build()
}
