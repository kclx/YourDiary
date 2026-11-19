use super::{card, comments, likes::Likes};
use crate::{
    service::{
        auth::Session,
        treehole::{Notification, TreeholeClient},
    },
    view::{navigation, shortcuts},
};
use adw::prelude::*;
use gtk::{
    Align, Box, Button, Label, ListBox,
    Orientation::{Horizontal, Vertical},
    gio, glib, pango,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub(super) struct Center {
    nav: glib::WeakRef<adw::NavigationView>,
    page: adw::NavigationPage,
    list: ListBox,
    status: Label,
    all: Button,
    bell: Button,
    dot: Box,
    client: TreeholeClient,
    session: Arc<Session>,
    likes: Rc<Likes>,
    rows: RefCell<Vec<Notification>>,
    request: Cell<u64>,
    fetching: Cell<bool>,
    marking: Cell<bool>,
    opening: Cell<bool>,
}

impl Center {
    pub(super) fn install(
        bar: &Box,
        nav: &adw::NavigationView,
        session: Arc<Session>,
        client: TreeholeClient,
        likes: Rc<Likes>,
    ) -> Rc<Self> {
        let bell = Button::builder()
            .icon_name("preferences-system-notifications-symbolic")
            .css_classes(["flat", "circular"])
            .valign(Align::Center)
            .tooltip_text("树洞通知")
            .build();
        let overlay = gtk::Overlay::builder()
            .child(&bell)
            .margin_end(12)
            .valign(Align::Center)
            .build();
        let dot = Box::builder()
            .halign(Align::End)
            .valign(Align::Start)
            .margin_top(4)
            .margin_end(3)
            .can_target(false)
            .visible(false)
            .css_classes(["treehole-unread-dot"])
            .build();
        overlay.add_overlay(&dot);
        bar.append(&overlay);

        let root = Box::new(Vertical, 0);
        let header = Box::builder()
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        let back = navigation::back_button("返回树洞");
        back.set_hexpand(true);
        header.append(&back);
        let all = Button::builder()
            .label("全部已读")
            .css_classes(["flat"])
            .sensitive(false)
            .build();
        header.append(&all);
        root.append(&header);
        let status = Label::builder()
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        root.append(&status);
        let list = ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .margin_start(16)
            .margin_end(16)
            .margin_top(12)
            .margin_bottom(16)
            .valign(Align::Start)
            .build();
        let clamp = adw::Clamp::builder().maximum_size(740).child(&list).build();
        root.append(
            &gtk::ScrolledWindow::builder()
                .vexpand(true)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(&clamp)
                .build(),
        );
        let center = Rc::new(Self {
            nav: nav.downgrade(),
            page: adw::NavigationPage::new(&root, "树洞通知"),
            list,
            status,
            all,
            bell,
            dot,
            client,
            session,
            likes,
            rows: Default::default(),
            request: Default::default(),
            fetching: Default::default(),
            marking: Default::default(),
            opening: Default::default(),
        });
        let weak = Rc::downgrade(&center);
        center.bell.connect_clicked(move |button| {
            if let Some(c) = weak.upgrade() {
                navigation::open(button, || Some(c.page.clone()));
            }
        });
        let weak = Rc::downgrade(&center);
        center.all.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                c.mark(None);
            }
        });
        let weak = Rc::downgrade(&center);
        center.list.connect_row_activated(move |_, row| {
            if let Some(c) = weak.upgrade() {
                let item = c.rows.borrow().get(row.index() as usize).cloned();
                if let Some(item) = item {
                    c.open(item);
                }
            }
        });
        let weak = Rc::downgrade(&center);
        root.connect_map(move |_| {
            if let Some(c) = weak.upgrade() {
                c.refresh();
            }
        });
        let weak = Rc::downgrade(&center);
        shortcuts::on_refresh(&root, move || {
            if let Some(c) = weak.upgrade() {
                c.refresh();
            }
        });
        center
    }

    pub(super) fn refresh(self: &Rc<Self>) {
        if self.marking.get() || self.fetching.replace(true) {
            return;
        }
        let request = self.request.get();
        if self.rows.borrow().is_empty() {
            self.message("正在读取通知…");
        }
        let weak = Rc::downgrade(self);
        let client = self.client.clone();
        let session = self.session.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || client.notifications(&session))
                .await
                .unwrap_or(Err("通知加载中断"));
            if let Some(c) = weak.upgrade()
                && c.session.is_active()
                && c.request.get() == request
            {
                c.fetching.set(false);
                match result {
                    Ok((rows, offline)) => c.show(rows, offline),
                    Err(msg) => c.message(msg),
                }
            }
        });
    }

    fn mark(self: &Rc<Self>, id: Option<i64>) {
        if self.marking.replace(true) {
            return;
        }
        self.request.set(self.request.get() + 1);
        self.fetching.set(false);
        self.all.set_sensitive(false);
        let client = self.client.clone();
        let session = self.session.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || client.mark_notifications(&session, id))
                .await
                .unwrap_or(Err("标记已读中断"));
            if let Some(c) = weak.upgrade()
                && c.session.is_active()
            {
                c.marking.set(false);
                match result {
                    Ok(rows) => c.show(rows, false),
                    Err(msg) => {
                        c.all.set_sensitive(c.rows.borrow().iter().any(|n| !n.read));
                        let parent = c.page.root().and_downcast::<gtk::Window>();
                        if let Some(parent) = parent {
                            let dialog = adw::MessageDialog::new(
                                Some(&parent),
                                Some("未能确认已读状态"),
                                Some(msg),
                            );
                            dialog.add_response("close", "关闭");
                            dialog.present();
                        }
                    }
                }
            }
        });
    }

    fn message(&self, text: &str) {
        self.status.set_text(text);
        self.status.set_visible(!text.is_empty());
    }

    fn show(&self, rows: Vec<Notification>, offline: bool) {
        let unread = rows.iter().any(|n| !n.read);
        self.dot.set_visible(unread);
        self.bell.set_tooltip_text(Some(if unread {
            "树洞通知（有未读）"
        } else {
            "树洞通知"
        }));
        self.all.set_sensitive(unread && !self.marking.get());
        self.message(match (offline, rows.is_empty()) {
            (true, true) => "离线 · 暂无缓存通知",
            (true, false) => "离线 · 显示上次的通知",
            (false, true) => "暂无树洞通知",
            _ => "",
        });
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for item in &rows {
            let row = gtk::ListBoxRow::builder()
                .selectable(false)
                .child(&summary(item, true))
                .build();
            if !item.read {
                row.set_tooltip_text(Some("未读通知"));
            }
            self.list.append(&row);
        }
        self.rows.replace(rows);
    }

    fn open(self: &Rc<Self>, item: Notification) {
        if self.marking.get() || self.opening.replace(true) {
            return;
        }
        if !item.read {
            self.mark(Some(item.id));
        }
        let weak = Rc::downgrade(self);
        let session = self.session.clone();
        let id = item.treehole_id.filter(|id| *id > 0);
        glib::spawn_future_local(async move {
            let cached = gio::spawn_blocking(move || TreeholeClient::cached_post(&session, id?))
                .await
                .ok()
                .flatten();
            let Some(c) = weak.upgrade() else { return };
            c.opening.set(false);
            let Some(nav) = c.nav.upgrade() else { return };
            if !c.session.is_active()
                || nav.visible_page().as_ref() != Some(&c.page)
                || !c.page.is_mapped()
            {
                return;
            }
            let detail = if let Some((post, tab, offset)) = cached {
                card::post_detail(
                    &post,
                    c.client.clone(),
                    c.session.clone(),
                    tab,
                    offset,
                    c.likes.clone(),
                    || {},
                )
            } else if let Some(id) = id {
                let article = notification_article(&item);
                article.append(
                    &Label::builder()
                        .label("评论")
                        .xalign(0.)
                        .css_classes(["title-3"])
                        .build(),
                );
                let client = c.client.clone();
                let session = c.session.clone();
                let sender = client.clone();
                let writer = session.clone();
                let thread = comments::thread(
                    &article,
                    id,
                    c.likes.clone(),
                    move |offset, latest| client.comments(&session, id, offset, latest),
                    || {},
                    move |text, reply| sender.write_comment(&writer, id, text, reply),
                    || {},
                );
                adw::NavigationPage::new(&thread, "树洞通知详情")
            } else {
                adw::NavigationPage::new(
                    &gtk::ScrolledWindow::builder()
                        .child(&notification_article(&item))
                        .build(),
                    "树洞通知详情",
                )
            };
            nav.push(&detail);
        });
    }
}

fn notification_article(item: &Notification) -> Box {
    let article = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_start(16)
        .margin_end(16)
        .margin_top(12)
        .build();
    article.append(&navigation::back_button("返回通知"));
    article.append(&summary(item, false));
    if let Some(text) = item.treehole.as_deref().filter(|s| !s.trim().is_empty()) {
        article.append(
            &Label::builder()
                .label(text)
                .xalign(0.)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .build(),
        );
    }
    article
}

fn summary(item: &Notification, compact: bool) -> Box {
    let body = Box::builder()
        .orientation(Vertical)
        .spacing(5)
        .margin_start(14)
        .margin_end(14)
        .margin_top(10)
        .margin_bottom(10)
        .build();
    let header = Box::new(Horizontal, 8);
    if !item.read && compact {
        header.append(
            &Box::builder()
                .valign(Align::Center)
                .css_classes(["treehole-unread-dot"])
                .build(),
        );
    }
    let name = item
        .user
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("匿名");
    header.append(
        &Label::builder()
            .label(format!("{name} · {}", item.action()))
            .xalign(0.)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::End)
            .css_classes(["heading"])
            .build(),
    );
    if let Some(time) = card::date(item.createdtime) {
        header.append(
            &Label::builder()
                .label(time)
                .css_classes(["caption", "dim-label"])
                .build(),
        );
    }
    body.append(&header);
    for (text, quote) in [(&item.comment_added, false), (&item.comment_replied, true)] {
        if let Some(text) = text.as_deref().filter(|s| !s.trim().is_empty()) {
            let label = Label::builder()
                .label(text)
                .xalign(0.)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .build();
            if compact {
                label.set_lines(2);
                label.set_ellipsize(pango::EllipsizeMode::End);
            }
            if quote {
                label.set_css_classes(&["treehole-notification-quote", "dim-label"]);
            }
            body.append(&label);
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::auth::SessionStore;

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn bell_opens_one_inbox_and_preserves_the_return_path() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            let dir =
                std::env::temp_dir().join(format!("yourdiary-inbox-view-{}", std::process::id()));
            let store = SessionStore::at(dir.join("test.sqlite3"));
            store.save("local-inbox-test", Some(1)).unwrap();
            let session = Arc::new(Session::restore(&store).unwrap().unwrap());
            let bar = Box::new(Horizontal, 0);
            let nav = navigation::section(&bar, "树洞");
            nav.set_animate_transitions(false);
            let c = Center::install(
                &bar,
                &nav,
                session,
                TreeholeClient::new(),
                super::super::likes::mock(),
            );
            c.fetching.set(true); // 使用本地列表，阻止 map 触发真实请求。
            let window = adw::Window::builder().content(&nav).build();
            window.present();
            c.show(serde_json::from_str(r#"[{"id":1,"type":"NEW_COMMENT","read":false,"user":"甲","comment_added":"回复内容"}]"#).unwrap(), false);
            assert!(c.dot.is_visible() && c.all.is_sensitive());
            c.bell.emit_clicked();
            c.bell.emit_clicked();
            assert_eq!(nav.navigation_stack().n_items(), 2);
            assert_eq!(nav.visible_page(), Some(c.page.clone()));
            assert!(!c.rows.borrow()[0].read);
            c.show(serde_json::from_str(r#"[{"id":1,"type":"NEW_COMMENT","read":true,"user":"甲","comment_added":"回复内容"}]"#).unwrap(),true);
            assert!(!c.dot.is_visible() && !c.all.is_sensitive());
            c.list.row_at_index(0).unwrap().emit_activate();
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while nav.navigation_stack().n_items() < 3 && std::time::Instant::now() < end {
                glib::MainContext::default().iteration(false);
            }
            assert_eq!(nav.visible_page().unwrap().title(), "树洞通知详情");
            assert!(nav.pop());
            assert_eq!(nav.visible_page(), Some(c.page.clone()));
            assert!(nav.pop());
            assert_eq!(nav.visible_page().unwrap().title(), "树洞");
            window.close();
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
}
