use crate::service::treehole::{Comment, Pagination, comment_order, fetch_pages};
use gtk::{
    Align, Box, Button, Label, ListView, NoSelection, Orientation::Vertical, ScrolledWindow,
    SignalListItemFactory, gio, glib, pango, prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

fn reading_column(content: &impl IsA<gtk::Widget>) -> adw::Clamp {
    adw::Clamp::builder()
        .maximum_size(720)
        .child(content)
        .build()
}

const MAX_VISIBLE_DEPTH: usize = 5;

fn comment_row(
    comment: &Comment,
    depth: usize,
    post: i64,
    likes: &super::likes::Likes,
) -> adw::Clamp {
    let branch = Box::new(gtk::Orientation::Horizontal, 0);
    branch.set_margin_start(20);
    branch.set_margin_end(20);
    for _ in 0..depth.min(MAX_VISIBLE_DEPTH) {
        let guide = Box::new(Vertical, 0);
        guide.add_css_class("treehole-thread-guide");
        branch.append(&guide);
    }
    let row = Box::builder()
        .orientation(Vertical)
        .spacing(5)
        .hexpand(true)
        .css_classes(["treehole-comment"])
        .build();
    let header = Box::new(gtk::Orientation::Horizontal, 8);
    let name = comment
        .username
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("匿名");
    let author = Label::builder()
        .label(name)
        .xalign(0.)
        .ellipsize(pango::EllipsizeMode::End)
        .css_classes(["heading"])
        .build();
    header.append(&author);
    if comment.is_author == Some(true) {
        header.append(
            &Label::builder()
                .label("楼主")
                .css_classes(["caption", "accent"])
                .build(),
        );
    }
    if let Some(time) = super::card::date(comment.createdtime) {
        header.append(
            &Label::builder()
                .label(time)
                .xalign(1.)
                .hexpand(true)
                .css_classes(["caption", "dim-label"])
                .build(),
        );
    }
    row.append(&header);
    if let Some(reply) = &comment.reply_to {
        let target = reply
            .username
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("匿名");
        let quote = reply
            .content
            .as_deref()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");
        let text = if quote.is_empty() {
            format!("回复 {target}")
        } else {
            format!("回复 {target} · {quote}")
        };
        header.set_tooltip_text(Some(&text));
        if depth == 0 || depth > MAX_VISIBLE_DEPTH {
            row.append(
                &Label::builder()
                    .label(if depth == 0 {
                        text
                    } else {
                        format!("回复 {target}")
                    })
                    .xalign(0.)
                    .ellipsize(pango::EllipsizeMode::End)
                    .css_classes(["caption", "dim-label"])
                    .build(),
            );
        }
    }
    let content = Label::builder()
        .label(comment.content.as_deref().unwrap_or(""))
        .xalign(0.)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .build();
    row.append(&content);
    if comment.id > 0 {
        let reply = Button::builder()
            .icon_name("mail-reply-sender-symbolic")
            .halign(Align::Start)
            .css_classes(["flat", "circular"])
            .tooltip_text(format!("回复 {name}"))
            .action_name("thread.reply")
            .action_target(&(comment.id, name).to_variant())
            .build();
        let actions = Box::new(gtk::Orientation::Horizontal, 6);
        let like = super::likes::LikeButton::new();
        likes.bind(
            &like,
            post,
            Some(comment.id),
            comment.liked,
            comment.like_count,
        );
        actions.append(&like.button);
        actions.append(&reply);
        row.append(&actions);
    }
    if depth > 0 {
        row.add_css_class("treehole-reply");
    }
    branch.append(&row);
    reading_column(&branch)
}

/// 仅替换顺序、层级或内容有变化的区段，保留正文、页脚和未变的评论行。
fn update_comments(model: &gio::ListStore, comments: &[Rc<Comment>]) {
    // ponytail: 每次加载一页线性重排已加载评论；超长讨论可按父 ID 增量插入。
    let order = comment_order(comments);
    let old_len = (model.n_items() - 2) as usize;
    let matches = |old: usize, new: usize| {
        let item = model
            .item(old as u32 + 1)
            .and_downcast::<glib::BoxedAnyObject>()
            .unwrap();
        let row = item.borrow::<(Rc<Comment>, usize)>();
        let (index, depth) = order[new];
        row.1 == depth && Rc::ptr_eq(&row.0, &comments[index])
    };
    let shared = old_len.min(order.len());
    let prefix = (0..shared).take_while(|&i| matches(i, i)).count();
    let suffix = (0..shared - prefix)
        .take_while(|&i| matches(old_len - 1 - i, order.len() - 1 - i))
        .count();
    let rows: Vec<_> = order[prefix..order.len() - suffix]
        .iter()
        .map(|&(index, depth)| glib::BoxedAnyObject::new((comments[index].clone(), depth)))
        .collect();
    let removed = old_len - prefix - suffix;
    if removed > 0 || !rows.is_empty() {
        model.splice(1 + prefix as u32, removed as u32, &rows);
    }
}

#[derive(Default)]
struct PageScroll {
    armed: Cell<bool>,
    position: Cell<Option<f64>>,
}

impl PageScroll {
    fn begin(&self) {
        self.position.set(None);
        self.armed.set(true);
    }
}

/// 正文、评论、加载状态共用一条 GTK 虚拟列表和一个滚动位置。
pub(super) fn thread(
    article: &Box,
    post: i64,
    likes: Rc<super::likes::Likes>,
    fetch: impl Fn(u32, bool) -> Result<Vec<Comment>, &'static str> + Send + Sync + 'static,
    refresh_article: impl Fn() + 'static,
    send: impl Fn(String, Option<i64>) -> Result<(), String> + Send + Sync + 'static,
    published: impl Fn() + 'static,
) -> Box {
    let footer = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(16)
        .halign(Align::Center)
        .build();
    let status = Label::builder()
        .css_classes(["caption", "dim-label"])
        .build();
    let retry = Button::builder()
        .label("重试")
        .visible(false)
        .css_classes(["flat"])
        .build();
    footer.append(&status);
    footer.append(&retry);

    let model = gio::ListStore::new::<glib::Object>();
    model.append(&reading_column(article));
    model.append(&footer);
    let factory = SignalListItemFactory::new();
    let row_likes = likes.clone();
    factory.connect_bind(move |_, item| {
        let row = item.downcast_ref::<gtk::ListItem>().unwrap();
        row.set_activatable(false);
        row.set_selectable(false);
        let value = row.item().unwrap();
        if let Some(comment) = value.downcast_ref::<glib::BoxedAnyObject>() {
            let data = comment.borrow::<(Rc<Comment>, usize)>();
            row.set_child(Some(&comment_row(&data.0, data.1, post, &row_likes)));
        } else {
            row.set_child(value.downcast_ref::<gtk::Widget>());
        }
    });
    factory.connect_unbind(|_, item| {
        item.downcast_ref::<gtk::ListItem>()
            .unwrap()
            .set_child(gtk::Widget::NONE)
    });
    let list = ListView::new(Some(NoSelection::new(Some(model.clone()))), Some(factory));
    list.add_css_class("treehole-thread");
    let root = Box::new(Vertical, 0);
    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();

    root.append(&scroll);
    let pages = Rc::new(RefCell::new(Pagination::default()));
    let comments = Rc::new(RefCell::new(Vec::new()));
    let busy = Rc::new(Cell::new(None));
    let generation = Rc::new(Cell::new(0u64));
    let fetch = Arc::new(fetch);
    let view = scroll.downgrade();
    let retry_weak = retry.downgrade();
    let retry_refresh = Rc::new(Cell::new(false));
    let retry_mode = retry_refresh.clone();
    let recent = Rc::new(RefCell::new(Vec::<Rc<Comment>>::new()));
    let has_published = Rc::new(Cell::new(false));
    let include_recent = has_published.clone();
    let initial_pages = pages.clone();
    let visit = Rc::new(PageScroll::default());
    let load_visit = visit.clone();
    let load = Rc::new(move |refresh: bool, after_write: bool| {
        let (Some(scroll), Some(retry)) = (view.upgrade(), retry_weak.upgrade()) else {
            return false;
        };
        let through = pages.borrow().offset;
        let offset = if refresh { 0 } else { through };
        if (!scroll.is_mapped() && !after_write)
            || (!after_write && busy.get().is_some_and(|manual| manual || !refresh))
            || (!refresh && (retry.is_visible() || pages.borrow().finished))
        {
            return false;
        }
        if refresh {
            load_visit.position.set(None);
            load_visit.armed.set(false);
        }
        busy.set(Some(refresh));
        let generation = generation.clone();
        let request = generation.get() + 1;
        generation.set(request);
        retry_mode.set(refresh);
        if refresh && !after_write {
            refresh_article();
        }
        retry.set_visible(false);
        status.set_text("正在加载评论…");
        let comments = comments.clone();
        let (pages, busy, fetch, model, status) = (
            pages.clone(),
            busy.clone(),
            fetch.clone(),
            model.clone(),
            status.clone(),
        );
        let view = scroll.downgrade();
        let recent = recent.clone();
        let with_recent = refresh && include_recent.get();
        let started = std::time::Instant::now();
        let likes = likes.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                let pages = fetch_pages(offset, through, |start| fetch(start, false))?;
                let newest = if with_recent {
                    Some(fetch(0, true)?)
                } else {
                    None
                };
                Ok((pages, newest))
            })
            .await
            .unwrap_or(Err("评论加载中断"));
            let Some(scroll) = view.upgrade().filter(|_| generation.get() == request) else {
                return;
            };
            match result {
                Ok((batches, newest)) => {
                    for row in batches.iter().flatten().chain(newest.iter().flatten()) {
                        likes.observe(post, Some(row.id), row.liked, row.like_count, started);
                    }
                    let mut pagination = pages.borrow_mut();
                    let mut comments = comments.borrow_mut();
                    if refresh {
                        *pagination = Pagination::default();
                        comments.clear();
                    }
                    for page in batches {
                        comments.extend(
                            pagination
                                .new_items(page, |c| c.id)
                                .into_iter()
                                .map(Rc::new),
                        );
                    }
                    let mut tail = recent.borrow_mut();
                    if let Some(newest) = newest {
                        *tail = newest.into_iter().rev().map(Rc::new).collect();
                    }
                    // 最新页只补充展示，按旧到新的分页游标仍由原列表推进。
                    let mut ids = std::collections::HashSet::new();
                    tail.retain(|c| !pagination.contains(c.id) && ids.insert(c.id));
                    drop(pagination);
                    if tail.is_empty() {
                        update_comments(&model, &comments);
                    } else {
                        let merged: Vec<_> = comments.iter().chain(tail.iter()).cloned().collect();
                        update_comments(&model, &merged);
                    }
                    status.set_text(if !pages.borrow().finished {
                        ""
                    } else if model.n_items() == 2 {
                        "暂无评论"
                    } else {
                        "已显示全部评论"
                    });
                    scroll.queue_allocate();
                }
                Err(message) => {
                    status.set_text(message);
                    retry.set_visible(true);
                }
            }
            busy.set(None);
        });
        true
    });
    let weak = Rc::downgrade(&load);
    retry.connect_clicked(move |button| {
        if let Some(load) = weak.upgrade() {
            button.set_visible(false);
            load(retry_refresh.get(), false);
        }
    });
    let weak = Rc::downgrade(&load);
    let next = {
        let visit = visit.clone();
        move |adj: &gtk::Adjustment| {
            if visit.armed.replace(false) && weak.upgrade().is_some_and(|load| load(false, false)) {
                visit.position.set(Some(adj.value()));
            }
        }
    };
    let at_edge = next.clone();
    scroll.connect_edge_reached(move |scroll, edge| {
        if edge == gtk::PositionType::Bottom {
            at_edge(&scroll.vadjustment());
        }
    });
    // GTK 可能在追加、不等高行重测及惯性滚动时继续移动到新页底部。
    // 同一次手势只允许一次触底，下一次用户操作才释放原阅读位置。
    let pin = visit.clone();
    scroll.vadjustment().connect_value_changed(move |adj| {
        if let Some(value) = pin.position.get() {
            let value = value.clamp(
                adj.lower(),
                (adj.upper() - adj.page_size()).max(adj.lower()),
            );
            if (adj.value() - value).abs() > 0.5 {
                adj.set_value(value);
            }
        }
    });
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_name(Some("comment-pagination"));
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    let smooth = Rc::new(Cell::new(false));
    let touchpad = smooth.clone();
    let start = visit.clone();
    wheel.connect_scroll_begin(move |_| {
        touchpad.set(true);
        start.begin();
    });
    let touchpad = smooth.clone();
    wheel.connect_scroll_end(move |_| touchpad.set(false));
    let view = scroll.downgrade();
    let motion = visit.clone();
    let last_step = Cell::new(None::<std::time::Instant>);
    wheel.connect_scroll(move |_, _, dy| {
        let Some(scroll) = view.upgrade() else {
            return glib::Propagation::Proceed;
        };
        // 普通滚轮没有 begin/end；连续的刻度属于同一轮滚动。
        let now = std::time::Instant::now();
        if !smooth.get()
            && last_step.replace(Some(now)).is_none_or(|last| {
                now.duration_since(last) >= std::time::Duration::from_millis(160)
            })
        {
            motion.begin();
        }
        if dy < 0. {
            motion.begin();
        } else if dy > 0. {
            let adj = scroll.vadjustment();
            if adj.page_size() > 0. && adj.value() + adj.page_size() >= adj.upper() - 0.5 {
                next(&adj);
            }
            if motion.position.get().is_some() {
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    scroll.add_controller(wheel);
    let input = gtk::EventControllerLegacy::new();
    input.set_propagation_phase(gtk::PropagationPhase::Capture);
    let held = Cell::new(None);
    input.connect_event(move |_, event| {
        use gtk::gdk::{EventType, Key, KeyEvent};
        match event.event_type() {
            EventType::ButtonPress | EventType::TouchBegin => visit.begin(),
            EventType::KeyPress => {
                if let Some(key) = event.downcast_ref::<KeyEvent>().map(KeyEvent::keyval)
                    && held.replace(Some(key)) != Some(key)
                    && matches!(
                        key,
                        Key::Down
                            | Key::Up
                            | Key::Page_Down
                            | Key::Page_Up
                            | Key::End
                            | Key::Home
                            | Key::space
                            | Key::Tab
                            | Key::ISO_Left_Tab
                    )
                {
                    visit.begin();
                }
            }
            EventType::KeyRelease => held.set(None),
            _ => {}
        }
        glib::Propagation::Proceed
    });
    scroll.add_controller(input);
    let refresh = load.clone();
    crate::view::shortcuts::on_refresh(&root, move || {
        refresh(true, false);
    });
    let reload = load.clone();
    super::composer::append(&root, send, move || {
        published();
        has_published.set(true);
        reload(true, true);
    });
    scroll.connect_map(move |_| {
        if initial_pages.borrow().offset == 0 {
            load(false, false);
        }
    });
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };

    fn wait_for(ready: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        while !ready() && Instant::now() < deadline {
            glib::MainContext::default().iteration(false);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(ready());
    }

    fn paging_wheel(scroll: &ScrolledWindow) -> gtk::EventControllerScroll {
        scroll
            .observe_controllers()
            .iter::<glib::Object>()
            .filter_map(|item| item.ok()?.downcast::<gtk::EventControllerScroll>().ok())
            .find(|c| c.name().as_deref() == Some("comment-pagination"))
            .unwrap()
    }

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn one_page_per_bottom_visit_keeps_the_reading_position() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            let offsets = Arc::new(Mutex::new(Vec::new()));
            let requests = offsets.clone();
            let root = thread(
                &Box::new(Vertical, 0),
                1,
                super::super::likes::mock(),
                move |offset, _| {
                    requests.lock().unwrap().push(offset);
                    let len = if offset < 80 { 20 } else { 0 };
                    Ok((offset..offset + len)
                        .map(|id| {
                            serde_json::from_value(
                                serde_json::json!({"id": id + 1,
                                    "content": "文".repeat(if id == 20 { 2004 } else if id % 9 == 0 { 230 } else { 12 }),
                                    "reply_to": (id > 0 && id % 7 == 0).then(|| serde_json::json!({"id": 2}))
                                }),
                            )
                            .unwrap()
                        })
                        .collect())
                },
                || {},
                |_, _| Ok(()),
                || {},
            );
            let scroll = root.first_child().and_downcast::<ScrolledWindow>().unwrap();
            let adj = scroll.vadjustment();
            let win = gtk::Window::builder()
                .child(&root)
                .default_width(420)
                .default_height(400)
                .build();
            win.present();
            wait_for(|| adj.upper() > adj.page_size() + 400. && adj.page_size() > 0.);
            let bottom = adj.upper() - adj.page_size();
            let wheel = paging_wheel(&scroll);
            wheel.emit_by_name::<()>("scroll-begin", &[]);
            wheel.emit_by_name::<bool>("scroll", &[&0., &1.]);
            adj.set_value(bottom);
            wait_for(|| {
                offsets.lock().unwrap().len() >= 2 && adj.upper() - adj.page_size() > bottom + 10.
            });
            // 同一次手势的惯性仍在推进，GTK 也可能随新布局再次报告触底。
            wheel.emit_by_name::<bool>("scroll", &[&0., &1.]);
            adj.set_value(adj.upper() - adj.page_size());
            scroll.emit_by_name::<()>("edge-reached", &[&gtk::PositionType::Bottom]);
            wheel.emit_by_name::<()>("scroll-end", &[]);
            let settle = Instant::now() + Duration::from_millis(220);
            wait_for(|| Instant::now() > settle);
            assert_eq!(*offsets.lock().unwrap(), [0, 20]);
            assert!(
                (adj.value() - bottom).abs() <= 1.,
                "{} vs {bottom}",
                adj.value()
            );
            wheel.emit_by_name::<()>("scroll-begin", &[]);
            wheel.emit_by_name::<bool>("scroll", &[&0., &1.]);
            adj.set_value(adj.upper() - adj.page_size());
            wait_for(|| offsets.lock().unwrap().len() >= 3);
            assert_eq!(*offsets.lock().unwrap(), [0, 20, 40]);
            win.close();
        });
    }

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn writes_and_replies_to_the_selected_comment() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            let writes = Arc::new(Mutex::new(Vec::new()));
            let sent = writes.clone();
            let stored = writes.clone();
            let accepted = Rc::new(Cell::new(0));
            let count = accepted.clone();
            let root = thread(
                &Box::new(Vertical, 0),
                1,
                crate::view::treehole::likes::mock(),
                move |offset, latest| {
                    let comment = |id, text: &str, parent: Option<i64>| {
                        serde_json::from_value(
                    serde_json::json!({"id": id, "username": "小树", "content": text, "reply_to": parent.map(|id| serde_json::json!({"id":id}))})
                ).unwrap()
                    };
                    if latest {
                        let writes = stored.lock().unwrap();
                        Ok(writes
                            .iter()
                            .skip(1)
                            .enumerate()
                            .rev()
                            .map(|(i, (text, parent)): (_, &(String, Option<i64>))| {
                                comment(30 + i as i64, text, *parent)
                            })
                            .collect())
                    } else if offset == 0 {
                        Ok(vec![comment(12, "原评论", None)])
                    } else {
                        Ok(vec![])
                    }
                },
                || {},
                move |text, parent| {
                    let mut writes = sent.lock().unwrap();
                    writes.push((text, parent));
                    if writes.len() == 1 {
                        Err("模拟发送失败".into())
                    } else {
                        Ok(())
                    }
                },
                move || count.set(count.get() + 1),
            );
            let scroll = root.first_child().and_downcast::<ScrolledWindow>().unwrap();
            let list = scroll.child().and_downcast::<ListView>().unwrap();
            let model = list.model().unwrap();
            let panel = root
                .last_child()
                .and_downcast::<adw::Clamp>()
                .unwrap()
                .child()
                .unwrap();
            let line = panel.last_child().unwrap();
            let input = line.first_child().and_downcast::<gtk::Entry>().unwrap();
            let submit = line.last_child().and_downcast::<Button>().unwrap();
            let cancel = submit.prev_sibling().and_downcast::<Button>().unwrap();
            let win = gtk::Window::builder()
                .child(&root)
                .default_width(480)
                .default_height(500)
                .build();
            win.present();
            wait_for(|| model.n_items() == 3);
            fn reply_button(w: &gtk::Widget) -> Option<Button> {
                w.downcast_ref::<Button>()
                    .filter(|b| b.action_name().as_deref() == Some("thread.reply"))
                    .cloned()
                    .or_else(|| {
                        std::iter::successors(w.first_child(), WidgetExt::next_sibling)
                            .find_map(|w| reply_button(&w))
                    })
            }
            wait_for(|| reply_button(list.upcast_ref()).is_some());
            reply_button(list.upcast_ref()).unwrap().emit_clicked();
            assert!(cancel.get_visible());
            input.set_text("  给小树的回复  ");
            cancel.emit_clicked();
            assert_eq!(input.text(), "  给小树的回复  ");
            reply_button(list.upcast_ref()).unwrap().emit_clicked();
            submit.emit_clicked();
            submit.emit_clicked();
            wait_for(|| submit.is_sensitive());
            assert!(cancel.get_visible());
            assert_eq!(input.text(), "  给小树的回复  ");
            assert_eq!(writes.lock().unwrap().len(), 1);
            input.emit_activate();
            wait_for(|| model.n_items() == 4);
            assert_eq!(accepted.get(), 1);
            let row = model
                .item(2)
                .and_downcast::<glib::BoxedAnyObject>()
                .unwrap();
            let data = row.borrow::<(Rc<Comment>, usize)>();
            assert_eq!((data.0.id, data.1), (30, 1));
            assert!(!cancel.get_visible() && input.text().is_empty());
            input.set_text("顶层评论");
            submit.emit_clicked();
            wait_for(|| model.n_items() == 5);
            assert_eq!(
                &*writes.lock().unwrap(),
                &[
                    ("给小树的回复".into(), Some(12)),
                    ("给小树的回复".into(), Some(12)),
                    ("顶层评论".into(), None)
                ]
            );
            assert_eq!(accepted.get(), 2);
            win.close();
        });
    }

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn comments_append_below_article_and_retry_the_same_offset() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            let article = Box::new(Vertical, 0);
            article.append(&Label::new(Some("正文")));
            let offsets = Arc::new(Mutex::new(Vec::new()));
            let requests = offsets.clone();
            let root = thread(
                &article,
                1,
                crate::view::treehole::likes::mock(),
                move |offset, _| {
                    let mut requests = requests.lock().unwrap();
                    requests.push(offset);
                    let ids = match offset {
                        0 => 0..1,
                        1 => 1..61,
                        61 if requests.iter().filter(|&&n| n == 61).count() == 1 => {
                            return Err("评论暂时无法连接");
                        }
                        61 => 60..62,
                        63 => 0..0,
                        _ => panic!("unexpected offset {offset}"),
                    };
                    Ok(ids.map(|id| serde_json::from_value(serde_json::json!({
                        "id":id,"username":"读者","content":"一条评论","is_author":true,
                        "reply_to": (id == 61).then(|| serde_json::json!({"id":0,"username":"读者","content":"一条评论"}))
                    })).unwrap()).collect())
                },
                || {},
                |_, _| Ok(()),
                || {},
            );
            let scroll = root.first_child().and_downcast::<ScrolledWindow>().unwrap();
            let list = scroll.child().and_downcast::<ListView>().unwrap();
            let model = list.model().unwrap();
            let footer = model.item(1).and_downcast::<Box>().unwrap();
            let retry = footer.last_child().and_downcast::<Button>().unwrap();
            let status = footer.first_child().and_downcast::<Label>().unwrap();
            let win = gtk::Window::builder()
                .default_width(400)
                .default_height(450)
                .child(&root)
                .build();
            win.present();
            wait_for(|| model.n_items() == 3 && scroll.vadjustment().page_size() > 0.);
            let wheel = paging_wheel(&scroll);
            wheel.emit_by_name::<bool>("scroll", &[&0., &1.]);
            wait_for(|| model.n_items() == 63 && scroll.vadjustment().upper() > 900.);
            let adj = scroll.vadjustment();
            wheel.emit_by_name::<()>("scroll-begin", &[]);
            wheel.emit_by_name::<bool>("scroll", &[&0., &1.]);
            adj.set_value(adj.upper() - adj.page_size());
            wheel.emit_by_name::<()>("scroll-end", &[]);
            wait_for(|| retry.is_visible());
            retry.emit_clicked();
            wait_for(|| model.n_items() == 64);
            let reply = model
                .item(2)
                .and_downcast::<glib::BoxedAnyObject>()
                .unwrap();
            let data = reply.borrow::<(Rc<Comment>, usize)>();
            assert_eq!((data.0.id, data.1), (61, 1));
            let branch = comment_row(&data.0, data.1, 1, &crate::view::treehole::likes::mock())
                .child()
                .unwrap();
            assert!(
                branch
                    .first_child()
                    .unwrap()
                    .has_css_class("treehole-thread-guide")
            );
            drop(data);
            // 新回复插在父评论下方；等 GTK 定位后再次向下滚动。
            let settle = Instant::now() + Duration::from_millis(120);
            wait_for(|| Instant::now() >= settle);
            assert_eq!(*offsets.lock().unwrap(), [0, 1, 61, 61]);
            wheel.emit_by_name::<()>("scroll-begin", &[]);
            if adj.value() + adj.page_size() >= adj.upper() {
                wheel.emit_by_name::<bool>("scroll", &[&0., &1.]);
            } else {
                adj.set_value(adj.upper() - adj.page_size());
            }
            wait_for(|| status.text() == "已显示全部评论");
            assert_eq!(*offsets.lock().unwrap(), [0, 1, 61, 61, 63]);
            scroll
                .activate_action(crate::view::shortcuts::REFRESH, None)
                .unwrap();
            wait_for(|| offsets.lock().unwrap().len() >= 8 && model.n_items() == 64);
            assert_eq!(&offsets.lock().unwrap()[5..8], [0, 1, 61]);
            win.close();
        });
    }
}
