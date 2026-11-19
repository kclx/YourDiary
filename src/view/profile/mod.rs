mod cards;
#[cfg(test)]
mod tests;

use std::{cell::Cell, rc::Rc};

use crate::service::profile::Profile;
use adw::{Clamp, StatusPage};
use cards::{card_switcher, person_card, pronoun};
use gtk::{
    Align, Box, Label,
    Orientation::{Horizontal, Vertical},
    PolicyType, ScrolledWindow, Separator, glib,
    pango::WrapMode,
    prelude::*,
};

pub(super) fn build(
    initial: Option<&Profile>,
) -> (adw::ToastOverlay, impl Fn(Option<&Profile>) + 'static) {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .build();
    let view = scroll.downgrade();
    let selected_partner = Rc::new(Cell::new(None));
    let update = move |profile: Option<&Profile>| {
        if let Some(view) = view.upgrade() {
            if let Some(profile) = profile {
                view.set_child(Some(&page(profile, &selected_partner)));
            } else {
                view.set_child(Some(
                    &StatusPage::builder()
                        .icon_name("avatar-default-symbolic")
                        .title("暂时无法获取个人资料")
                        .description("请检查网络后按 Ctrl+R 刷新。")
                        .build(),
                ));
            }
        }
    };
    if initial.is_some() {
        update(initial);
    } else {
        scroll.set_child(Some(
            &StatusPage::builder().title("正在同步个人资料").build(),
        ));
    }
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&scroll));
    (overlay, update)
}

fn label(text: &str, class: &str) -> Label {
    Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(WrapMode::WordChar)
        .css_classes([class])
        .build()
}

fn page(profile: &Profile, selected_partner: &Rc<Cell<Option<(i64, i64)>>>) -> Clamp {
    let user = &profile.user;
    let column = Box::builder()
        .valign(Align::Start)
        .orientation(Vertical)
        .spacing(22)
        .margin_top(30)
        .margin_bottom(32)
        .margin_start(20)
        .margin_end(20)
        .build();
    let own = person_card(
        "profile-cover",
        "我",
        &user.name,
        user.avatar.as_deref(),
        user.description.as_deref(),
        user.age_range.as_deref(),
        [user.diary_count, user.word_count, user.image_count],
    );

    if let Some(p) = &profile.partner {
        let card = person_card(
            "profile-partner",
            pronoun(p),
            &p.name,
            Some(&p.avatar),
            p.description.as_deref(),
            None,
            [p.diary_count, p.word_count, p.image_count],
        );
        let pair = (user.userid, p.userid);
        let cards = card_switcher(&own, &card, &user.name, p, pair, selected_partner.clone());
        column.append(&cards);
    } else {
        selected_partner.set(None);
        column.append(&own);
        let paired = Box::new(Vertical, 10);
        paired.append(&label("日记搭档", "heading"));
        let card = Box::new(Vertical, 0);
        card.set_css_classes(&["card", "profile-partner"]);
        card.append(&label(
            if profile.paired {
                "已配对，搭档资料暂缺"
            } else {
                "尚未配对"
            },
            "body",
        ));
        paired.append(&card);
        column.append(&paired);
    }

    let member = if user.is_member {
        user.member_expiry_time
            .and_then(date)
            .map_or_else(|| "会员".into(), |d| format!("会员 · {d} 到期"))
    } else {
        "普通用户".into()
    };
    let own_details = account_section(
        "账号信息",
        [
            (
                "avatar-default-symbolic",
                "用户 ID",
                user.userid.to_string(),
                true,
            ),
            (
                "mail-unread-symbolic",
                "邮箱",
                user.useremail.clone().unwrap_or_default(),
                true,
            ),
            ("starred-symbolic", "会员", member, false),
            (
                "user-available-symbolic",
                "树洞",
                if user.enable_treehole {
                    "已开启"
                } else {
                    "已关闭"
                }
                .into(),
                false,
            ),
        ],
    );
    column.append(&own_details);
    if let Some(p) = &profile.partner {
        let mut dates = Vec::new();
        if let Some(day) = date(p.paired_time) {
            dates.push(format!("相识于 {day}"));
        }
        if let Some(day) = date(p.last_login_time) {
            dates.push(format!("{}最近登录于 {day}", pronoun(p)));
        }
        if !dates.is_empty() {
            let note = label(&dates.join("  ·  "), "caption");
            note.add_css_class("dim-label");
            column.append(&note);
        }
    }
    Clamp::builder()
        .maximum_size(740)
        .tightening_threshold(540)
        .child(&column)
        .build()
}

fn account_section(
    title: &str,
    fields: impl IntoIterator<Item = (&'static str, &'static str, String, bool)>,
) -> Box {
    let section = Box::new(Vertical, 10);
    section.append(&label(title, "heading"));
    let rows = Box::new(Vertical, 0);
    rows.set_css_classes(&["card", "profile-account"]);
    for (icon, key, value, can_copy) in fields {
        if value.trim().is_empty() {
            continue;
        }
        if rows.first_child().is_some() {
            rows.append(&Separator::new(Horizontal));
        }
        let row = Box::new(Horizontal, 12);
        row.append(&gtk::Image::from_icon_name(icon));
        row.append(&Label::new(Some(key)));
        let text = label(&value, "body");
        text.set_hexpand(true);
        text.set_xalign(1.0);
        row.append(&text);
        if can_copy {
            let button = gtk::Button::builder()
                .child(&row)
                .css_classes(["flat", "profile-row"])
                .tooltip_text(format!("点击复制{key}"))
                .build();
            button.set_cursor_from_name(Some("pointer"));
            let toast = adw::Toast::builder()
                .title(format!("已复制{key}"))
                .timeout(2)
                .build();
            button.connect_clicked(move |button| {
                button.clipboard().set_text(&value);
                if let Some(overlay) = button
                    .ancestor(adw::ToastOverlay::static_type())
                    .and_downcast::<adw::ToastOverlay>()
                {
                    overlay.add_toast(toast.clone());
                }
            });
            rows.append(&button);
        } else {
            row.add_css_class("profile-row");
            rows.append(&row);
        }
    }
    section.append(&rows);
    section
}

fn date(timestamp: i64) -> Option<String> {
    if timestamp <= 0 {
        return None;
    }
    glib::DateTime::from_unix_local(timestamp)
        .ok()?
        .format("%Y.%m.%d")
        .ok()
        .map(Into::into)
}
