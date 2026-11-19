use gtk::{Button, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::Instant,
};

type WriteLike = dyn Fn(i64, Option<i64>, bool, u64) -> Result<(), String> + Send + Sync;

pub(super) fn counter_row(icon: &impl IsA<gtk::Widget>) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_valign(gtk::Align::Center);
    row.add_css_class("treehole-counts");
    icon.set_size_request(16, 16);
    icon.set_valign(gtk::Align::Center);
    let number = gtk::Label::builder()
        .label("0")
        .xalign(0.)
        .valign(gtk::Align::Center)
        .build();
    row.append(icon);
    row.append(&number);
    (row, number)
}

pub(super) struct LikeButton {
    pub button: Button,
    pub number: gtk::Label,
}

impl LikeButton {
    pub fn new() -> Self {
        let icon = gtk::DrawingArea::new();
        icon.set_draw_func(|icon, cr, _, _| {
            cr.set_source_color(&icon.color());
            cr.set_line_width(1.4);
            cr.set_line_join(gtk::cairo::LineJoin::Round);
            cr.move_to(8., 14.);
            cr.curve_to(7., 13., 1.1, 9., 1.1, 5.6);
            cr.curve_to(1.1, 1.4, 5.7, 0.8, 8., 4.1);
            cr.curve_to(10.3, 0.8, 14.9, 1.4, 14.9, 5.6);
            cr.curve_to(14.9, 9., 9., 13., 8., 14.);
            cr.close_path();
            if icon
                .ancestor(Button::static_type())
                .is_some_and(|b| b.has_css_class("liked"))
            {
                let _ = cr.fill_preserve();
            }
            let _ = cr.stroke();
        });
        let (row, number) = counter_row(&icon);
        let button = Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["flat", "treehole-heart"])
            .child(&row)
            .build();
        Self { button, number }
    }
}

// 同一帖子可出现在多个分类，虚拟列表反复绑定行时也共用同一个操作。
pub(super) struct Likes {
    group: gio::SimpleActionGroup,
    changed: RefCell<HashMap<String, Rc<Cell<Option<Instant>>>>>,
    write: Arc<WriteLike>,
    error: Rc<dyn Fn(&str)>,
}

impl Likes {
    pub fn new(
        write: impl Fn(i64, Option<i64>, bool, u64) -> Result<(), String> + Send + Sync + 'static,
        error: impl Fn(&str) + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            group: gio::SimpleActionGroup::new(),
            changed: RefCell::default(),
            write: Arc::new(write),
            error: Rc::new(error),
        })
    }

    fn name(post: i64, comment: Option<i64>) -> String {
        match comment {
            Some(id) => format!("post-{post}-comment-{id}"),
            None => format!("post-{post}"),
        }
    }

    pub fn observe(
        &self,
        post: i64,
        comment: Option<i64>,
        liked: Option<bool>,
        count: Option<u64>,
        started: Instant,
    ) {
        let key = Self::name(post, comment);
        if let Some(action) = self
            .group
            .lookup_action(&key)
            .and_downcast::<gio::SimpleAction>()
            && action.is_enabled()
            && self.changed.borrow()[&key]
                .get()
                .is_none_or(|t| t <= started)
        {
            self.changed.borrow()[&key].set(Some(started));
            action.set_state(&(liked.unwrap_or(false), count.unwrap_or(0)).to_variant());
        }
    }

    pub fn bind(
        &self,
        like: &LikeButton,
        post: i64,
        comment: Option<i64>,
        liked: Option<bool>,
        count: Option<u64>,
    ) -> glib::Binding {
        let key = Self::name(post, comment);
        let action = self
            .group
            .lookup_action(&key)
            .and_downcast::<gio::SimpleAction>()
            .unwrap_or_else(|| {
                let action = gio::SimpleAction::new_stateful(
                    &key,
                    None,
                    &(liked.unwrap_or(false), count.unwrap_or(0)).to_variant(),
                );
                let changed = Rc::new(Cell::new(None));
                self.changed
                    .borrow_mut()
                    .insert(key.clone(), changed.clone());
                let write = self.write.clone();
                let error = self.error.clone();
                action.set_enabled(post > 0 && comment.is_none_or(|id| id > 0));
                action.connect_activate(move |action, _| {
                    if !action.is_enabled() {
                        return;
                    }
                    let (liked, count) = action.state().unwrap().get::<(bool, u64)>().unwrap();
                    let next = (
                        !liked,
                        if liked {
                            count.saturating_sub(1)
                        } else {
                            count.saturating_add(1)
                        },
                    );
                    action.set_enabled(false);
                    let (action, write, error, changed) = (
                        action.clone(),
                        write.clone(),
                        error.clone(),
                        changed.clone(),
                    );
                    glib::spawn_future_local(async move {
                        match gio::spawn_blocking(move || write(post, comment, next.0, next.1))
                            .await
                            .unwrap_or_else(|_| Err("点赞请求中断，请刷新后重试".into()))
                        {
                            Ok(()) => {
                                changed.set(Some(Instant::now()));
                                action.set_state(&next.to_variant());
                            }
                            Err(message) => error(&message),
                        }
                        action.set_enabled(true);
                    });
                });
                self.group.add_action(&action);
                action
            });
        let button = &like.button;
        let weak_button = glib::SendWeakRef::from(button.downgrade());
        button.insert_action_group("likes", Some(&self.group));
        button.set_action_name(Some(&format!("likes.{key}")));
        action
            .bind_property("state", &like.number, "label")
            .transform_to(move |_, state: glib::Variant| {
                let (liked, count) = state.get::<(bool, u64)>()?;
                let button = weak_button.upgrade()?;
                if liked {
                    button.add_css_class("liked");
                } else {
                    button.remove_css_class("liked");
                }
                let hint = format!("{} · {count} 个赞", if liked { "取消点赞" } else { "点赞" });
                button.set_tooltip_text(Some(&hint));
                button.update_property(&[gtk::accessible::Property::Label(&hint)]);
                Some(count.to_string())
            })
            .sync_create()
            .build()
    }
}

#[cfg(test)]
pub(super) fn mock() -> Rc<Likes> {
    Likes::new(|_, _, _, _| Ok(()), |_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Mutex, time::Duration};

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn post_and_comment_likes_toggle_only_after_confirmation() {
        gtk::test_synced(|| {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let sent = calls.clone();
            let failure = Rc::new(Cell::new(false));
            let reported = failure.clone();
            let likes = Likes::new(
                move |post, comment, liked, count| {
                    let mut requests = sent.lock().unwrap();
                    requests.push((post, comment, liked, count));
                    if requests.len() == 3 {
                        Err("模拟失败".into())
                    } else {
                        Ok(())
                    }
                },
                move |_| reported.set(true),
            );
            let card = LikeButton::new();
            let detail = LikeButton::new();
            let comment = LikeButton::new();
            likes.bind(&card, 7, None, Some(false), Some(3));
            likes.bind(&detail, 7, None, Some(false), Some(3));
            likes.bind(&comment, 7, Some(9), Some(false), Some(0));
            let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
            for b in [&card, &detail, &comment] {
                root.append(&b.button);
            }
            let window = gtk::Window::builder().child(&root).build();
            window.present();
            let click = |button: &Button| {
                button.emit_clicked();
                assert!(!button.is_sensitive());
                button.emit_clicked();
                let deadline = Instant::now() + Duration::from_secs(3);
                while !button.is_sensitive() && Instant::now() < deadline {
                    glib::MainContext::default().iteration(false);
                    std::thread::sleep(Duration::from_millis(2));
                }
                assert!(button.is_sensitive());
            };
            let before = Instant::now();
            click(&card.button);
            likes.observe(7, None, Some(false), Some(3), before);
            assert!(detail.button.has_css_class("liked"));
            assert_eq!(detail.number.text(), "4");
            assert_eq!(
                detail.button.tooltip_text().as_deref(),
                Some("取消点赞 · 4 个赞")
            );
            click(&detail.button);
            assert!(!card.button.has_css_class("liked"));
            assert_eq!(card.number.text(), "3");
            assert_eq!(card.button.tooltip_text().as_deref(), Some("点赞 · 3 个赞"));
            click(&comment.button);
            assert!(failure.get());
            assert_eq!(comment.number.text(), "0");
            click(&comment.button);
            assert_eq!(comment.number.text(), "1");
            click(&comment.button);
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    (7, None, true, 4),
                    (7, None, false, 3),
                    (7, Some(9), true, 1),
                    (7, Some(9), true, 1),
                    (7, Some(9), false, 0)
                ]
            );
            window.close();
        });
    }
}
