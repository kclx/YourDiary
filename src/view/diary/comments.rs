use crate::service::{
    auth::Session,
    diary::{DiaryEntry, Message, SyncError, messages, send_message},
};
use gtk::{
    Align, Box, Button, Entry, Label,
    Orientation::{Horizontal, Vertical},
    gio, glib, pango,
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

fn bubble(message: &Message, mine: Option<bool>) -> Box {
    let (align, who, class) = match mine {
        Some(true) => (Align::End, "我", "outgoing"),
        Some(false) => (Align::Start, "对方", "incoming"),
        None => (Align::Center, "未知发言人", "unknown"),
    };
    let row = Box::new(Vertical, 3);
    row.set_halign(align);
    // 为另一侧留白，同时允许窄窗口内的长文本换行。
    row.set_margin_start(if mine == Some(true) { 44 } else { 0 });
    row.set_margin_end(if mine == Some(false) { 44 } else { 0 });
    let text = Label::builder()
        .label(if message.text.is_empty() {
            "暂无内容"
        } else {
            &message.text
        })
        .halign(align)
        .xalign(0.)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .max_width_chars(46)
        .css_classes(["diary-bubble", class])
        .build();
    let time = message
        .time
        .and_then(|t| glib::DateTime::from_unix_local(t).ok())
        .and_then(|t| t.format("%m.%d %H:%M").ok());
    let caption = time.map_or_else(|| who.to_owned(), |t| format!("{who} · {t}"));
    row.append(&text);
    row.append(
        &Label::builder()
            .halign(align)
            .label(caption)
            .css_classes(["caption", "dim-label"])
            .build(),
    );
    row
}

pub(super) struct Comments {
    pub widget: Box,
    pub composer: Box,
    rows: Box,
    status: Label,
    retry: Button,
    input: Entry,
    send: Button,
    session: Arc<Session>,
    own: Option<i64>,
    diary: Cell<i64>,
    author: Cell<(Option<i64>, bool)>,
    generation: Cell<u64>,
    drafts: RefCell<HashMap<i64, String>>,
    sending: RefCell<HashSet<i64>>,
}

impl Comments {
    pub fn new(session: Arc<Session>, account: Option<i64>) -> Rc<Self> {
        let widget = Box::new(Vertical, 12);
        widget.append(&gtk::Separator::new(Horizontal));
        widget.append(
            &Label::builder()
                .label("留言")
                .xalign(0.)
                .css_classes(["title-3"])
                .build(),
        );
        let rows = Box::new(Vertical, 10);
        widget.append(&rows);
        let feedback = Box::new(Horizontal, 8);
        let status = Label::builder()
            .xalign(0.)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        let retry = Button::builder()
            .label("重新加载")
            .css_classes(["flat"])
            .visible(false)
            .build();
        feedback.append(&status);
        feedback.append(&retry);
        widget.append(&feedback);
        let composer = Box::new(Horizontal, 8);
        composer.set_hexpand(true);
        let input = Entry::builder()
            .hexpand(true)
            .width_chars(6)
            .placeholder_text("写一条留言…")
            .build();
        let send = Button::builder()
            .label("发送")
            .sensitive(false)
            .css_classes(["suggested-action", "pill"])
            .build();
        composer.append(&input);
        composer.append(&send);
        let own = account.or_else(|| session.profile().map(|p| p.user.userid));
        let view = Rc::new(Self {
            widget,
            composer,
            rows,
            status,
            retry,
            input,
            send,
            session,
            own,
            diary: Cell::new(0),
            author: Cell::default(),
            generation: Cell::default(),
            drafts: RefCell::default(),
            sending: RefCell::default(),
        });
        let weak = Rc::downgrade(&view);
        view.input.connect_changed(move |_| {
            if let Some(v) = weak.upgrade() {
                v.update_composer();
            }
        });
        let weak = Rc::downgrade(&view);
        view.input.connect_activate(move |_| {
            if let Some(v) = weak.upgrade() {
                v.submit();
            }
        });
        let weak = Rc::downgrade(&view);
        view.send.connect_clicked(move |_| {
            if let Some(v) = weak.upgrade() {
                v.submit();
            }
        });
        let weak = Rc::downgrade(&view);
        view.retry.connect_clicked(move |_| {
            if let Some(v) = weak.upgrade() {
                v.load();
            }
        });
        view
    }

    pub fn open(self: &Rc<Self>, entry: &DiaryEntry) {
        let old = self.diary.replace(entry.id);
        if old != 0 && old != entry.id {
            let draft = self.input.text();
            if !draft.is_empty() {
                self.drafts.borrow_mut().insert(old, draft.into());
            }
            let restored = self
                .drafts
                .borrow_mut()
                .remove(&entry.id)
                .unwrap_or_default();
            self.input.set_text(&restored);
        }
        self.author.set((entry.author_id, entry.is_partner));
        if old != entry.id {
            self.clear_rows();
        }
        self.update_composer();
        if self.sending.borrow().contains(&entry.id) {
            self.notice("正在发送…", false);
        } else {
            self.load();
        }
    }

    fn update_composer(&self) {
        let pending = self.sending.borrow().contains(&self.diary.get());
        self.input.set_editable(!pending);
        self.send
            .set_sensitive(!pending && !self.input.text().trim().is_empty());
    }
    fn clear_rows(&self) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
    }
    fn notice(&self, text: &str, retry: bool) {
        self.status.set_text(text);
        self.status.set_visible(!text.is_empty());
        self.retry.set_visible(retry);
        if let Some(feedback) = self.status.parent() {
            feedback.set_visible(!text.is_empty() || retry);
        }
    }
    fn load(self: &Rc<Self>) {
        let id = self.diary.get();
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.notice("正在加载留言…", false);
        let session = self.session.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || messages(&session, id)).await;
            let Some(v) = weak
                .upgrade()
                .filter(|v| v.diary.get() == id && v.generation.get() == generation)
            else {
                return;
            };
            match result {
                Ok(Ok(items)) => {
                    v.clear_rows();
                    for msg in &items {
                        let mine = msg.is_mine(v.own, v.author.get());
                        v.rows.append(&bubble(msg, mine));
                    }
                    v.notice(if items.is_empty() { "暂无留言" } else { "" }, false);
                }
                _ => v.notice("留言加载失败", true),
            }
        });
    }
    fn submit(self: &Rc<Self>) {
        let id = self.diary.get();
        let text = self.input.text().trim().to_owned();
        if id <= 0 || text.is_empty() || !self.sending.borrow_mut().insert(id) {
            return;
        }
        self.generation.set(self.generation.get() + 1);
        self.update_composer();
        self.notice("正在发送…", false);
        let session = self.session.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || send_message(&session, id, text)).await;
            let Some(v) = weak.upgrade() else { return };
            v.sending.borrow_mut().remove(&id);
            let success = matches!(result, Ok(Ok(())));
            if success {
                v.drafts.borrow_mut().remove(&id);
            }
            if v.diary.get() != id {
                return;
            }
            v.update_composer();
            if success {
                v.input.set_text("");
                v.load();
            } else if matches!(
                result,
                Ok(Err(SyncError::Rejected | SyncError::Unauthorized))
            ) {
                v.notice("留言未发送，草稿已保留", true);
            } else {
                v.notice("发送结果未确认，请重新加载后核对", true);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn conversation_wraps_and_aligns_to_both_edges() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            crate::view::install_styles();
            let rows = Box::new(Vertical, 10);
            let item = |s: &str| Message {
                sender: Some(1),
                time: None,
                text: s.to_owned(),
            };
            let mine = bubble(&item("你好"), Some(true));
            let partner = bubble(&item(&"这是一条很长的留言。".repeat(20)), Some(false));
            rows.append(&mine);
            rows.append(&partner);
            let win = gtk::Window::builder()
                .default_width(330)
                .child(&rows)
                .build();
            win.present();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while rows.width() <= 0 && std::time::Instant::now() < deadline {
                glib::MainContext::default().iteration(false);
            }
            let me = mine.first_child().unwrap();
            let other = partner.first_child().and_downcast::<Label>().unwrap();
            assert!(me.has_css_class("outgoing") && other.has_css_class("incoming"));
            let right = me.compute_bounds(&rows).unwrap();
            let left = other.compute_bounds(&rows).unwrap();
            assert!((right.x() + right.width() - rows.width() as f32).abs() < 2.);
            assert!(left.x() < 2. && left.width() < rows.width() as f32 - 35.);
            assert!(other.layout().line_count() > 1);
            win.close();
        });
    }
}
