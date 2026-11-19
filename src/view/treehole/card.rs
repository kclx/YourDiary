use crate::service::treehole::{Comment, Post};
use gtk::{Box, Button, Label, Orientation::Vertical, glib, pango, prelude::*};
use std::{cell::Cell, rc::Rc};

fn label(class: &str, lines: i32) -> Label {
    let label = Label::builder()
        .xalign(0.)
        .wrap(lines != 1)
        .wrap_mode(pango::WrapMode::WordChar)
        .max_width_chars(1)
        .lines(lines)
        .build();
    if lines > 0 {
        label.set_ellipsize(pango::EllipsizeMode::End);
    }
    label.add_css_class(class);
    label
}

fn author(post: &Post) -> &str {
    post.username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("匿名")
}

fn append_comment_indicator(row: &Box) -> Label {
    let bubble = gtk::Image::from_icon_name("chat-message-new-symbolic");
    bubble.set_pixel_size(16);
    let (group, number) = super::likes::counter_row(&bubble);
    row.append(&group);
    number
}

fn comment_count(number: &Label, count: u64) {
    number.set_text(&count.to_string());
    let text = format!("评论 {count}");
    number.set_tooltip_text(Some(&text));
    number.update_property(&[gtk::accessible::Property::Label(&text)]);
}

// Pango 按段落限制行数。列表把段落合成摘要，详情保留原文的分段。
fn preview(text: &str) -> String {
    text.split(['\r', '\n', '\u{0085}', '\u{2028}', '\u{2029}'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) struct Card {
    pub tile: Box,
    pub button: Button,
    pub like: super::likes::LikeButton,
    binding: std::cell::RefCell<Option<glib::Binding>>,
    index: Rc<Cell<usize>>,
    title: Label,
    body: Label,
    avatar: adw::Avatar,
    author: Label,
    pub counts: Label,
}

impl Card {
    pub fn new(open: Rc<dyn Fn(usize)>) -> Self {
        let button = Button::new();
        let tile = Box::new(Vertical, 8);
        tile.add_css_class("card");
        tile.add_css_class("treehole-card");
        button.add_css_class("flat");
        button.add_css_class("treehole-open");
        let content = Box::new(Vertical, 9);
        let title = label("heading", 2);
        let body = label("body", 6);
        let footer = Box::new(gtk::Orientation::Horizontal, 8);
        footer.add_css_class("treehole-footer");
        let avatar = adw::Avatar::new(24, None, true);
        let author = label("caption", 1);
        author.set_hexpand(true);
        footer.append(&avatar);
        footer.append(&author);
        let like = super::likes::LikeButton::new();
        footer.append(&like.button);
        let counts = append_comment_indicator(&footer);
        content.append(&title);
        content.append(&body);
        tile.append(&button);
        tile.append(&footer);
        button.set_child(Some(&content));
        let index = Rc::new(Cell::new(0));
        let current = index.clone();
        button.connect_clicked(move |_| open(current.get()));
        let tap = gtk::GestureClick::new();
        tap.set_button(gtk::gdk::BUTTON_PRIMARY);
        let opener = button.downgrade();
        tap.connect_released(move |tap, _, x, y| {
            if let Some(target) = tap
                .widget()
                .and_then(|tile| tile.pick(x, y, gtk::PickFlags::INSENSITIVE))
                && !target.is::<Button>()
                && target.ancestor(Button::static_type()).is_none()
                && let Some(button) = opener.upgrade()
            {
                button.emit_clicked();
            }
        });
        tile.add_controller(tap);
        Self {
            tile,
            like,
            binding: Default::default(),
            button,
            index,
            title,
            body,
            avatar,
            author,
            counts,
        }
    }

    pub fn bind(&self, index: usize, post: &Post, likes: Option<&super::likes::Likes>) {
        self.index.set(index);
        let title = post.title.as_deref().unwrap_or("").trim();
        self.title.set_visible(!title.is_empty());
        self.title.set_text(&preview(title));
        let content = post.content.as_deref().unwrap_or("").trim();
        // GTK 只展示六行；限制交给 Pango 的预览长度，详情保留原文。
        let mut chars = content.chars();
        let mut excerpt: String = chars.by_ref().take(360).collect();
        if chars.next().is_some() {
            excerpt.push('…');
        }
        self.body
            .set_visible(!excerpt.is_empty() || title.is_empty());
        self.body.set_text(&preview(if excerpt.is_empty() {
            "暂无正文"
        } else {
            &excerpt
        }));
        let name = author(post);
        self.avatar.set_text(Some(name));
        self.author.set_text(name);
        comment_count(&self.counts, post.comment_count.unwrap_or(0));
        if let Some(binding) = self.binding.take() {
            binding.unbind();
        }
        if let Some(likes) = likes {
            self.binding.replace(Some(likes.bind(
                &self.like,
                post.id,
                None,
                post.liked,
                post.like_count,
            )));
        } else {
            self.like
                .number
                .set_text(&post.like_count.unwrap_or(0).to_string());
        }
    }
}

pub(super) fn post_detail(
    post: &Post,
    client: crate::service::treehole::TreeholeClient,
    session: std::sync::Arc<crate::service::auth::Session>,
    tab: String,
    offset: u32,
    likes: Rc<super::likes::Likes>,
    published: impl Fn() + 'static,
) -> adw::NavigationPage {
    let id = post.id;
    let posts = client.clone();
    let reader = session.clone();
    let sender = client.clone();
    let writer = session.clone();
    detail(
        post,
        likes,
        move || {
            posts
                .posts(&reader, &tab, offset)?
                .into_iter()
                .find(|post| post.id == id)
                .ok_or("帖子已移出原列表页，当前显示上次的正文")
        },
        move |offset, latest| client.comments(&session, id, offset, latest),
        move |text, reply| sender.write_comment(&writer, id, text, reply),
        published,
    )
}

pub(super) fn detail(
    post: &Post,
    likes: Rc<super::likes::Likes>,
    fetch_post: impl Fn() -> Result<Post, &'static str> + Send + Sync + 'static,
    fetch: impl Fn(u32, bool) -> Result<Vec<Comment>, &'static str> + Send + Sync + 'static,
    send: impl Fn(String, Option<i64>) -> Result<(), String> + Send + Sync + 'static,
    published: impl Fn() + 'static,
) -> adw::NavigationPage {
    let content = Box::builder()
        .orientation(Vertical)
        .spacing(16)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();
    content.append(&crate::view::navigation::back_button("返回"));
    let title = label("title-2", -1);
    let meta = label("dim-label", -1);
    let body = label("diary-prose", -1);
    let notice = label("dim-label", -1);
    notice.set_visible(false);
    for row in [&title, &meta, &body] {
        content.append(row);
    }
    let actions = Box::new(gtk::Orientation::Horizontal, 10);
    let like = super::likes::LikeButton::new();
    likes.bind(&like, post.id, None, post.liked, post.like_count);
    actions.append(&like.button);
    let stats = append_comment_indicator(&actions);
    content.append(&actions);
    content.append(&notice);
    let count = Rc::new(Cell::new(0u64));
    let current_count = count.clone();
    let stats_after_send = stats.clone();
    let update = move |post: &Post| {
        let value = post.title.as_deref().unwrap_or("").trim();
        title.set_text(value);
        title.set_visible(!value.is_empty());
        meta.set_text(
            &[Some(author(post)), date(post.createdtime).as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("   ·   "),
        );
        body.set_text(
            post.content
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("暂无正文"),
        );
        let comments = post.comment_count.unwrap_or(0);
        current_count.set(comments);
        comment_count(&stats, comments);
    };
    update(post);
    let heading = label("title-3", -1);
    heading.set_text("评论");
    heading.set_margin_top(12);
    content.append(&heading);
    let fetch_post = std::sync::Arc::new(fetch_post);
    let loading = Rc::new(Cell::new(false));
    let revision = Rc::new(Cell::new(0u64));
    let refresh_revision = revision.clone();
    let article_likes = likes.clone();
    let scroll = super::comments::thread(
        &content,
        post.id,
        likes,
        fetch,
        move || {
            if loading.replace(true) {
                return;
            }
            let loading = loading.clone();
            let fetch_post = fetch_post.clone();
            let update = update.clone();
            let notice = notice.clone();
            let revision = refresh_revision.clone();
            let started = revision.get();
            let requested = std::time::Instant::now();
            let article_likes = article_likes.clone();
            glib::spawn_future_local(async move {
                match gtk::gio::spawn_blocking(move || fetch_post())
                    .await
                    .unwrap_or(Err("帖子刷新中断"))
                {
                    Ok(post) if revision.get() == started => {
                        article_likes.observe(
                            post.id,
                            None,
                            post.liked,
                            post.like_count,
                            requested,
                        );
                        update(&post);
                        notice.set_visible(false);
                    }
                    Ok(_) => {}
                    Err(message) => {
                        notice.set_text(message);
                        notice.set_visible(true);
                    }
                }
                loading.set(false);
            });
        },
        send,
        move || {
            published();
            revision.set(revision.get() + 1);
            let comments = count.get().saturating_add(1);
            count.set(comments);
            comment_count(&stats_after_send, comments);
        },
    );
    adw::NavigationPage::new(&scroll, "树洞详情")
}

pub(super) fn date(timestamp: Option<i64>) -> Option<glib::GString> {
    timestamp
        .and_then(|ts| {
            glib::DateTime::from_unix_local(if ts > 100_000_000_000 { ts / 1000 } else { ts }).ok()
        })
        .and_then(|d| d.format("%Y.%m.%d %H:%M").ok())
}
