use super::label;
use crate::service::profile::{PairedProfile, load_avatar};
use adw::Avatar;
use gtk::{
    Align, Box, Label,
    Orientation::{Horizontal, Vertical},
    Separator, gdk, gio, glib,
    prelude::*,
};
use std::{cell::Cell, rc::Rc};

fn portrait(name: &str, path: Option<&str>, size: i32) -> Avatar {
    let widget = Avatar::new(size, Some(name), true);
    widget.set_valign(Align::Center);
    if let Some(path) = path.map(str::trim).filter(|s| !s.is_empty()) {
        let path = path.to_owned();
        let avatar = widget.downgrade();
        glib::MainContext::default().spawn_local(async move {
            let Ok(Some(bytes)) = gio::spawn_blocking(move || load_avatar(&path)).await else {
                return;
            };
            if let Some(avatar) = avatar.upgrade() {
                let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
                if let Ok(image) = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(
                    &stream,
                    size * 2,
                    size * 2,
                    true,
                    gio::Cancellable::NONE,
                ) {
                    avatar.set_custom_image(Some(&gdk::Texture::for_pixbuf(&image)));
                }
            }
        });
    }
    widget
}

pub(super) fn card_switcher(
    own: &Box,
    partner: &Box,
    name: &str,
    partner_profile: &PairedProfile,
    pair: (i64, i64),
    selected_partner: Rc<Cell<Option<(i64, i64)>>>,
) -> gtk::Button {
    let front = gtk::Stack::builder()
        .css_classes(["profile-front-layer"])
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(180)
        .build();
    front.add_named(own, Some("own"));
    front.add_named(partner, Some("partner"));
    let back = gtk::Stack::builder()
        .css_classes(["profile-back-layer"])
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(180)
        .margin_start(12)
        .margin_end(12)
        .margin_top(20)
        .build();

    let strip_height = gtk::SizeGroup::new(gtk::SizeGroupMode::Vertical);
    for (id, role, who, class) in [
        ("own", "我", name, "profile-cover"),
        (
            "partner",
            pronoun(partner_profile),
            &*partner_profile.name,
            "profile-partner",
        ),
    ] {
        let background = Box::new(Vertical, 0);
        background.set_css_classes(&["card", class, "profile-back"]);
        let strip = Box::new(Horizontal, 10);
        strip.set_valign(Align::End);
        strip.set_vexpand(true);
        strip.add_css_class("profile-strip");
        strip.append(&label(role, "dim-label"));
        let nickname = Label::builder()
            .label(who)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["heading"])
            .build();
        strip.append(&nickname);
        strip.append(&gtk::Image::from_icon_name("pan-up-symbolic"));
        strip_height.add_widget(&strip);
        background.append(&strip);
        back.add_named(&background, Some(id));
    }
    let deck = gtk::Overlay::builder()
        .margin_top(24)
        .margin_bottom(12)
        .margin_start(18)
        .margin_end(28)
        .build();
    let layer = Box::new(Vertical, 0);
    front.set_vexpand(true);
    layer.append(&front);
    let gap = Box::new(Vertical, 0);
    strip_height.add_widget(&gap);
    layer.append(&gap);
    deck.set_child(Some(&layer));
    deck.add_overlay(&back);
    back.insert_before(&deck, Some(&layer));

    let button = gtk::Button::builder()
        .child(&deck)
        .css_classes(["flat", "profile-switcher"])
        .build();
    button.set_cursor_from_name(Some("pointer"));
    let own_hint = format!("查看我的资料 · {name}");
    let partner_hint = format!(
        "查看{}的资料 · {}",
        pronoun(partner_profile),
        partner_profile.name
    );
    let display = Rc::new(move |button: &gtk::Button, show_partner| {
        let (top, bottom, hint) = if show_partner {
            ("partner", "own", &own_hint)
        } else {
            ("own", "partner", &partner_hint)
        };
        front.set_visible_child_name(top);
        back.set_visible_child_name(bottom);
        button.set_tooltip_text(Some(hint));
    });
    let partner_in_front = selected_partner.get() == Some(pair);
    selected_partner.set(partner_in_front.then_some(pair));
    display(&button, partner_in_front);
    button.connect_clicked(move |button| {
        if button.has_css_class("drawing-out") {
            return;
        }
        let show_partner = selected_partner.get() != Some(pair);
        selected_partner.set(show_partner.then_some(pair));
        if !button.is_mapped() || !button.settings().is_gtk_enable_animations() {
            display(button, show_partner);
            return;
        }
        if button.width() < 440 {
            button.add_css_class("compact");
        } else {
            button.remove_css_class("compact");
        }
        button.add_css_class("drawing-out");
        let display = display.clone();
        let button = button.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(180), move || {
            if let Some(button) = button.upgrade() {
                display(&button, show_partner);
                button.remove_css_class("drawing-out");
            }
        });
    });
    button
}

pub(super) fn pronoun(partner: &PairedProfile) -> &'static str {
    match partner.role.as_str() {
        "girl" => "她",
        "boy" => "他",
        _ => "对方",
    }
}

pub(super) fn person_card(
    class: &str,
    person: &str,
    name: &str,
    avatar: Option<&str>,
    bio: Option<&str>,
    age: Option<&str>,
    counts: [i64; 3],
) -> Box {
    let card = Box::new(Vertical, 14);
    card.set_css_classes(&["card", class]);
    let row = Box::new(Horizontal, 16);
    row.append(&portrait(name, avatar, 68));
    row.set_vexpand(true);
    let text = Box::new(Vertical, 4);
    text.set_valign(Align::Center);
    let meta = age
        .map(str::trim)
        .filter(|age| !age.is_empty())
        .map_or_else(|| person.to_owned(), |age| format!("{person} · {age}"));
    text.append(&label(&meta, "caption-heading"));
    text.append(&label(name, "title-1"));
    if let Some(bio) = bio.map(str::trim).filter(|v| !v.is_empty()) {
        text.append(&label(bio, "body"));
    }
    row.append(&text);
    card.append(&row);
    card.append(&Separator::new(Horizontal));
    card.append(&stats(counts[0], counts[1], counts[2]));
    card
}

fn stats(diaries: i64, words: i64, images: i64) -> Box {
    let row = Box::new(Horizontal, 6);
    row.set_homogeneous(true);
    for (key, count) in [("日记", diaries), ("字数", words), ("图片", images)] {
        let cell = Box::new(Vertical, 2);
        cell.append(&label(&count.to_string(), "title-2"));
        cell.append(&label(key, "dim-label"));
        row.append(&cell);
    }
    row
}
