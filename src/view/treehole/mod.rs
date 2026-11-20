mod card;
mod comments;
mod composer;
mod likes;
mod notifications;
mod waterfall;

use crate::service::{
    auth::Session,
    treehole::{Feed, TreeholeClient, fetch_pages},
};
use gtk::{
    Align, Box, Button, Label,
    Orientation::{Horizontal, Vertical},
    ScrolledWindow, Stack, ToggleButton, gio, glib,
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::Arc,
};
use waterfall::Waterfall;

pub(super) fn build(session: Arc<Session>) -> adw::NavigationView {
    let body = Box::new(Vertical, 0);
    let tabs = Box::new(Horizontal, 6);
    tabs.set_margin_start(16);
    tabs.set_margin_end(16);
    tabs.set_margin_top(8);
    tabs.set_margin_bottom(8);
    let strip = ScrolledWindow::builder()
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&tabs)
        .build();
    let bar = Box::new(Horizontal, 0);
    bar.add_css_class("treehole-categories");
    bar.append(&strip);
    let pages = Stack::builder()
        .vexpand(true)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
    let status = adw::StatusPage::builder()
        .title("正在加载分类…")
        .vexpand(true)
        .build();
    let retry = Button::with_label("重试");
    retry.set_halign(Align::Center);
    retry.set_visible(false);
    status.set_child(Some(&retry));
    pages.set_visible(false);
    body.append(&bar);
    body.append(&status);
    body.append(&pages);
    let nav = crate::view::navigation::section(&body, "树洞");

    let client = TreeholeClient::new();
    let writer = client.clone();
    let account = session.clone();
    let parent = nav.downgrade();
    let likes = likes::Likes::new(
        move |post, comment, liked, count| writer.set_like(&account, post, comment, liked, count),
        move |message| {
            use adw::prelude::MessageDialogExt;
            if let Some(window) = parent
                .upgrade()
                .and_then(|nav| nav.root())
                .and_downcast::<gtk::Window>()
            {
                let dialog =
                    adw::MessageDialog::new(Some(&window), Some("点赞状态更新失败"), Some(message));
                dialog.add_response("close", "关闭");
                dialog.present();
            }
        },
    );
    let center =
        notifications::Center::install(&bar, &nav, session.clone(), client.clone(), likes.clone());
    let retry_button = retry.clone();
    let retry = retry.downgrade();
    let started = Rc::new(Cell::new(false));
    let visible_pages = pages.downgrade();
    let status = status.downgrade();
    let load_tabs = Rc::new(move || {
        if started.replace(true) {
            return;
        }
        let (Some(status), Some(retry)) = (status.upgrade(), retry.upgrade()) else {
            return;
        };
        retry.set_visible(false);
        let started = started.clone();
        status.set_icon_name(Some("content-loading-symbolic"));
        status.set_title("正在加载分类…");
        status.set_description(None);
        let client = client.clone();
        let session = session.clone();
        let (tabs, pages, status, retry) = (
            tabs.clone(),
            pages.clone(),
            status.clone(),
            retry.downgrade(),
        );
        let worker = {
            let client = client.clone();
            let session = session.clone();
            move || client.tabs(&session)
        };
        let likes = likes.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(worker)
                .await
                .unwrap_or(Err("树洞分类加载中断"));
            if retry.upgrade().is_none() {
                return;
            }
            let entries = match result {
                Ok(entries) => entries,
                Err(message) => {
                    started.set(false);
                    status.set_icon_name(Some("dialog-warning-symbolic"));
                    status.set_title("分类加载失败");
                    status.set_description(Some(message));
                    if let Some(r) = retry.upgrade() {
                        r.set_visible(true);
                    }
                    return;
                }
            };
            let mut group: Option<ToggleButton> = None;
            let mut seen = HashSet::new();
            for tab in entries {
                if tab.key.is_empty() || !seen.insert(tab.key.clone()) {
                    continue;
                }
                let toggle = ToggleButton::with_label(if tab.label.is_empty() {
                    &tab.key
                } else {
                    &tab.label
                });
                toggle.add_css_class("treehole-tab");
                toggle.add_css_class("pill");
                toggle.add_css_class("flat");
                toggle.set_group(group.as_ref());
                tabs.append(&toggle);
                let page = Page::new(
                    tab.key.clone(),
                    session.clone(),
                    client.clone(),
                    likes.clone(),
                );
                pages.add_named(&page.body, Some(&tab.key));
                let pages = pages.downgrade();
                toggle.connect_toggled(move |b| {
                    if b.is_active() {
                        if let Some(pages) = pages.upgrade() {
                            pages.set_visible_child_name(&page.key);
                        }
                        page.schedule();
                    }
                });
                if group.is_none() {
                    toggle.set_active(true);
                    group = Some(toggle);
                }
            }
            if group.is_none() {
                started.set(false);
                status.set_icon_name(Some("view-list-symbolic"));
                status.set_title("暂时没有可浏览的分类");
                if let Some(retry) = retry.upgrade() {
                    retry.set_visible(true);
                }
            } else {
                status.set_visible(false);
                pages.set_visible(true);
            }
        });
    });
    // 同一窗口首次显示树洞时才发起请求。重试复用同一入口。
    let first = load_tabs.clone();
    let inbox = center.clone();
    body.connect_map(move |_| {
        first();
        inbox.refresh();
    });
    let retry_tabs = load_tabs.clone();
    retry_button.connect_clicked(move |_| retry_tabs());
    crate::view::shortcuts::on_refresh(&body, move || {
        center.refresh();
        if let Some(page) = visible_pages
            .upgrade()
            .and_then(|pages| pages.visible_child())
        {
            let _ = page.activate_action(crate::view::shortcuts::REFRESH, None);
        } else {
            load_tabs();
        }
    });
    nav
}

struct Page {
    key: String,
    client: TreeholeClient,
    likes: Rc<likes::Likes>,
    session: Arc<Session>,
    feed: RefCell<Feed>,
    flow: RefCell<Waterfall>,
    body: Box,
    status: Label,
    footer: Box,
    spinner: gtk::Spinner,
    retry: Button,
    busy: Cell<Option<bool>>,
    generation: Cell<u64>,
    error: Cell<Option<(&'static str, bool)>>,
    pending_render: Cell<bool>,
}

impl Page {
    fn new(
        key: String,
        session: Arc<Session>,
        client: TreeholeClient,
        likes: Rc<likes::Likes>,
    ) -> Rc<Self> {
        let page = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let weak = weak.clone();
            let body = Box::new(Vertical, 0);
            let footer = Box::new(Horizontal, 12);
            footer.set_halign(Align::Center);
            footer.set_margin_top(4);
            footer.set_margin_bottom(8);
            let status = Label::new(None);
            status.add_css_class("dim-label");
            status.add_css_class("caption");
            let retry = Button::with_label("重试");
            retry.add_css_class("flat");
            retry.set_visible(false);
            let spinner = gtk::Spinner::new();
            footer.append(&spinner);
            footer.append(&status);
            footer.append(&retry);
            footer.set_visible(false);
            let flow = Waterfall::new(
                Rc::new(move |index| {
                    if let Some(page) = weak.upgrade() {
                        crate::view::navigation::open(&page.body, || {
                            let feed = page.feed.borrow();
                            feed.posts.get(index).map(|post| {
                                let id = post.id;
                                let owner = Rc::downgrade(&page);
                                let posts = page.client.clone();
                                let reader = page.session.clone();
                                let tab = page.key.clone();
                                let offset = feed.page_offset(index);
                                card::post_detail(
                                    post,
                                    page.client.clone(),
                                    page.session.clone(),
                                    move || {
                                        posts
                                            .posts(&reader, &tab, offset)?
                                            .into_iter()
                                            .find(|p| p.id == id)
                                            .ok_or("帖子已移出原列表页，当前显示上次的正文")
                                    },
                                    page.likes.clone(),
                                    move || {
                                        if let Some(page) = owner.upgrade()
                                            && let Some((index, post)) = page
                                                .feed
                                                .borrow_mut()
                                                .posts
                                                .iter_mut()
                                                .enumerate()
                                                .find(|(_, p)| p.id == id)
                                        {
                                            post.comment_count = Some(
                                                post.comment_count.unwrap_or(0).saturating_add(1),
                                            );
                                            page.flow.borrow().update_card(index, post);
                                        }
                                    },
                                )
                            })
                        });
                    }
                }),
                &footer,
                likes.clone(),
            );
            body.append(&flow.scroll);
            Self {
                key,
                likes,
                session,
                client,
                body,
                status,
                footer,
                spinner,
                retry,
                flow: RefCell::new(flow),
                feed: RefCell::default(),
                busy: Cell::default(),
                generation: Cell::default(),
                error: Cell::default(),
                pending_render: Cell::default(),
            }
        });
        let weak = Rc::downgrade(&page);
        crate::view::shortcuts::on_refresh(&page.body, move || {
            if let Some(page) = weak.upgrade() {
                page.load(true);
            }
        });
        let weak = Rc::downgrade(&page);
        page.retry.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.load(p.error.get().is_some_and(|(_, refresh)| refresh))
            }
        });
        let scroll = page.flow.borrow().scroll.clone();
        let weak = Rc::downgrade(&page);
        scroll.connect_map(move |_| {
            if let Some(p) = weak.upgrade() {
                p.schedule();
            }
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&page);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            use gtk::gdk::{Key, ModifierType};
            if modifiers.intersects(
                ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            let offset = match key {
                Key::ISO_Left_Tab => -1,
                Key::Tab if modifiers.contains(ModifierType::SHIFT_MASK) => -1,
                Key::Tab => 1,
                _ => return glib::Propagation::Proceed,
            };
            weak.upgrade()
                .is_some_and(|p| {
                    p.flow
                        .borrow_mut()
                        .focus_relative(&p.feed.borrow().posts, offset)
                })
                .into()
        });
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        scroll.add_controller(keys);
        for adj in [scroll.vadjustment(), scroll.hadjustment()] {
            let weak = Rc::downgrade(&page);
            adj.connect_changed(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.schedule()
                }
            });
        }
        let weak = Rc::downgrade(&page);
        scroll.vadjustment().connect_value_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.schedule();
            }
        });
        page
    }

    fn load(self: &Rc<Self>, refresh: bool) {
        if (!refresh && self.feed.borrow().pages.finished)
            || self.busy.get().is_some_and(|manual| manual || !refresh)
        {
            return;
        }
        self.busy.set(Some(refresh));
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.error.set(None);
        self.footer();
        let session = self.session.clone();
        let key = self.key.clone();
        let client = self.client.clone();
        let through = self.feed.borrow().pages.offset;
        let started = std::time::Instant::now();
        let offset = if refresh { 0 } else { through };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                fetch_pages(offset, through, |start| client.posts(&session, &key, start))
            })
            .await
            .unwrap_or(Err("帖子加载中断"));
            let Some(p) = weak.upgrade().filter(|p| p.generation.get() == generation) else {
                return;
            };
            match result {
                Ok(pages) => {
                    let mut feed = p.feed.borrow_mut();
                    if refresh {
                        *feed = Feed::default();
                        p.flow.borrow_mut().invalidate();
                    }
                    for page in pages {
                        for post in &page {
                            p.likes
                                .observe(post.id, None, post.liked, post.like_count, started);
                        }
                        feed.append(page);
                    }
                }
                Err(error) => p.error.set(Some((error, refresh))),
            }
            p.busy.set(None);
            p.footer();
            p.schedule();
        });
    }

    fn footer(&self) {
        let feed = self.feed.borrow();
        let text = if self.busy.get().is_some() {
            "正在加载帖子…"
        } else if let Some((error, _)) = self.error.get() {
            error
        } else if feed.pages.finished && feed.posts.is_empty() {
            "这个分类暂时没有帖子"
        } else if feed.pages.finished {
            "已看到最后一条"
        } else {
            ""
        };
        self.status.set_text(text);
        self.spinner.set_spinning(self.busy.get().is_some());
        self.spinner.set_visible(self.busy.get().is_some());
        self.retry.set_visible(self.error.get().is_some());
        self.footer.set_visible(!text.is_empty());
    }

    fn schedule(self: &Rc<Self>) {
        if self.pending_render.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(p) = weak.upgrade() {
                p.pending_render.set(false);
                if p.body.is_mapped() {
                    let near_end = {
                        let mut flow = p.flow.borrow_mut();
                        flow.render(&p.feed.borrow().posts);
                        flow.near_end()
                    };
                    if near_end && p.error.get().is_none() {
                        p.load(false);
                    }
                }
            }
        });
    }
}
