use gtk::{Box, Button, Entry, Label, Orientation, gio, glib, prelude::*};
use std::{cell::Cell, rc::Rc, sync::Arc};

pub(super) fn append(
    root: &Box,
    send: impl Fn(String, Option<i64>) -> Result<(), String> + Send + Sync + 'static,
    published: impl Fn() + 'static,
) {
    let panel = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    let line = Box::new(Orientation::Horizontal, 8);
    let input = Entry::builder()
        .placeholder_text("写评论…")
        .width_chars(6)
        .hexpand(true)
        .build();
    let cancel = Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("取消回复")
        .css_classes(["flat", "circular"])
        .visible(false)
        .build();
    let submit = Button::builder()
        .icon_name("mail-send-symbolic")
        .tooltip_text("发送评论")
        .sensitive(false)
        .css_classes(["suggested-action", "circular"])
        .build();
    let status = Label::builder()
        .xalign(0.)
        .wrap(true)
        .visible(false)
        .css_classes(["caption"])
        .build();
    line.append(&input);
    line.append(&cancel);
    line.append(&submit);
    panel.append(&status);
    panel.append(&line);
    root.append(&gtk::Separator::new(Orientation::Horizontal));
    root.append(
        &adw::Clamp::builder()
            .maximum_size(720)
            .child(&panel)
            .build(),
    );

    let target = Rc::new(Cell::new(None));
    let reply = gio::SimpleAction::new("reply", Some(&<(i64, String)>::static_variant_type()));
    let group = gio::SimpleActionGroup::new();
    group.add_action(&reply);
    root.insert_action_group("thread", Some(&group));
    let select = {
        let (input, cancel, status, target) = (
            input.downgrade(),
            cancel.downgrade(),
            status.downgrade(),
            target.clone(),
        );
        move |comment: Option<(i64, String)>| {
            let (Some(input), Some(cancel), Some(status)) =
                (input.upgrade(), cancel.upgrade(), status.upgrade())
            else {
                return;
            };
            target.set(comment.as_ref().map(|(id, _)| *id));
            let hint =
                comment.map_or_else(|| "写评论…".into(), |(_, name)| format!("回复 {name}…"));
            input.set_placeholder_text(Some(&hint));
            status.remove_css_class("error");
            status.set_text(hint.trim_end_matches('…'));
            status.set_visible(target.get().is_some());
            cancel.set_visible(target.get().is_some());
            input.grab_focus();
        }
    };
    let choose = select.clone();
    reply.connect_activate(move |_, value| {
        if let Some(pair) = value.and_then(|v| v.get::<(i64, String)>()) {
            choose(Some(pair));
        }
    });
    cancel.connect_clicked(move |_| select(None));
    input.connect_changed(glib::clone!(
        #[weak]
        submit,
        move |input| {
            submit.set_sensitive(input.is_editable() && !input.text().trim().is_empty());
        }
    ));
    input.connect_activate(glib::clone!(
        #[weak]
        submit,
        move |_| {
            if submit.is_sensitive() {
                submit.emit_clicked();
            }
        }
    ));
    let send = Arc::new(send);
    let published = Rc::new(published);
    submit.connect_clicked(move |submit| {
        let text = input.text().trim().to_owned();
        if text.is_empty() || !input.is_editable() {
            return;
        }
        let parent = target.get();
        input.set_editable(false);
        reply.set_enabled(false);
        cancel.set_sensitive(false);
        submit.set_sensitive(false);
        submit.set_label("发送中…");
        let (send, published) = (send.clone(), published.clone());
        let (submit, input, cancel, status, reply) = (
            submit.downgrade(),
            input.clone(),
            cancel.clone(),
            status.clone(),
            reply.clone(),
        );
        let target = target.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || send(text, parent))
                .await
                .unwrap_or_else(|_| Err("未能确认发送结果，请先刷新评论再重试".into()));
            if result.is_ok() {
                published();
            }
            let Some(submit) = submit.upgrade() else {
                return;
            };
            input.set_editable(true);
            reply.set_enabled(true);
            cancel.set_sensitive(true);
            submit.set_label("发送");
            match result {
                Ok(()) => {
                    input.set_text("");
                    input.set_placeholder_text(Some("写评论…"));
                    target.set(None);
                    cancel.set_visible(false);
                    status.remove_css_class("error");
                    status.set_text(if parent.is_some() {
                        "回复已发送"
                    } else {
                        "评论已发送"
                    });
                }
                Err(error) => {
                    status.add_css_class("error");
                    let message = if parent.is_some() {
                        format!(
                            "{} · {error}",
                            input
                                .placeholder_text()
                                .unwrap_or_default()
                                .trim_end_matches('…')
                        )
                    } else {
                        error
                    };
                    status.set_text(&message);
                    submit.set_sensitive(true);
                }
            }
            status.set_visible(true);
        });
    });
}
