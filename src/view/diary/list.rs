use super::{apply_gender_color, detail, search};
use crate::service::{
    auth::Session,
    diary::{DiaryEntry, cached_body},
};
use crate::view::navigation;
use gtk::{
    Box, FilterListModel, Label, ListItem, ListView, NoSelection,
    Orientation::{Horizontal, Vertical},
    SearchEntry, SignalListItemFactory, StringList, StringObject, gio,
    glib::MainContext,
    pango::EllipsizeMode,
    prelude::*,
};
use std::{rc::Rc, sync::Arc};

pub(super) fn show_entries(
    list: &ListView,
    entries: Vec<DiaryEntry>,
    session: Option<Arc<Session>>,
    account_id: Option<i64>,
    search: (&SearchEntry, &Label),
) -> (gtk::glib::SignalHandlerId, gtk::glib::SignalHandlerId) {
    list.add_css_class("diary-list");
    let entries = Rc::new(entries);
    let positions: Vec<_> = (0..entries.len()).map(|index| index.to_string()).collect();
    let position_strings: Vec<_> = positions.iter().map(String::as_str).collect();
    let model = StringList::new(&position_strings);
    let (filter, matches, changed) =
        search::connect(list, search.0, search.1, entries.clone(), account_id);
    let filtered = FilterListModel::new(Some(model.clone()), Some(filter));
    let factory = SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let row = Box::builder()
            .orientation(Vertical)
            .spacing(5)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(4)
            .margin_end(6)
            .css_classes(["card", "activatable", "diary-card"])
            .build();
        let header = Box::new(Horizontal, 8);
        let subtitle = Label::builder()
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(EllipsizeMode::End)
            .css_classes(["diary-meta"])
            .build();
        let title = Label::builder()
            .xalign(0.0)
            .ellipsize(EllipsizeMode::End)
            .css_classes(["heading", "diary-card-title"])
            .build();
        let preview = Label::builder()
            .xalign(0.0)
            .margin_end(48)
            .ellipsize(EllipsizeMode::End)
            .css_classes(["diary-preview"])
            .build();
        let receipt = Label::builder()
            .css_classes(["diary-meta", "diary-receipt"])
            .build();
        header.append(&subtitle);
        header.append(&receipt);
        row.append(&header);
        row.append(&title);
        row.append(&preview);
        item.set_child(Some(&row));
    });
    let entries_for_rows = entries.clone();
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let row = item.child().unwrap();
        let header = row.first_child().unwrap();
        let subtitle = header.first_child().and_downcast::<Label>().unwrap();
        let receipt = header.last_child().and_downcast::<Label>().unwrap();
        let title = header.next_sibling().and_downcast::<Label>().unwrap();
        let preview = row.last_child().and_downcast::<Label>().unwrap();
        let index = item
            .item()
            .and_downcast::<StringObject>()
            .unwrap()
            .string()
            .parse::<usize>()
            .unwrap();
        let entry = &entries_for_rows[index];
        apply_gender_color(&row, entry.gender);
        let heading = entry.title.trim();
        title.set_visible(!heading.is_empty());
        title.set_text(&single_line_preview(heading, false));
        subtitle.set_text(
            &[entry.date.as_str(), &entry.weather, &entry.mood]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        );
        let read = entry.read.get();
        receipt.set_visible(read.is_some());
        if let Some(read) = read {
            receipt.set_text(if read { "已读" } else { "• 未读" });
            receipt.set_tooltip_text(Some(match (entry.is_partner, read) {
                (true, true) => "我已读",
                (true, false) => "我未读",
                (false, true) => "对方已读",
                (false, false) => "对方未读",
            }));
            if read {
                receipt.remove_css_class("unread");
            } else {
                receipt.add_css_class("unread");
            }
        }
        if let Some(hit) = matches
            .borrow()
            .as_ref()
            .and_then(|matches| matches.get(&index))
        {
            if let Some(markup) = &hit.title {
                title.set_markup(markup);
            }
            if let Some(markup) = &hit.body {
                preview.set_markup(markup);
                preview.set_visible(true);
                return;
            }
        }
        if entry.has_full_content {
            show_full_preview(&preview, &entry.content, heading.is_empty());
            return;
        }
        preview.set_visible(true);
        if entry.content.trim().is_empty() {
            preview.set_text("正文尚未缓存");
        } else {
            preview.set_text(&format!(
                "同步摘要 · {}",
                single_line_preview(&entry.content, true)
            ));
        }
        let Some((account, author)) = account_id.zip(entry.author_id) else {
            return;
        };
        let id = entry.id;
        let revision = entry.revision.get();
        let untitled = heading.is_empty();
        let item = item.downgrade();
        let preview = preview.downgrade();
        let matches = matches.clone();
        MainContext::default().spawn_local(async move {
            let Ok(Some(text)) =
                gio::spawn_blocking(move || cached_body(account, author, id, revision)).await
            else {
                return;
            };
            let Some(bound_index) = item
                .upgrade()
                .and_then(|item| item.item())
                .and_downcast::<StringObject>()
                .and_then(|item| item.string().parse::<usize>().ok())
            else {
                return;
            };
            if bound_index == index
                && matches
                    .borrow()
                    .as_ref()
                    .and_then(|matches| matches.get(&index))
                    .is_none_or(|hit| hit.body.is_none())
            {
                if let Some(preview) = preview.upgrade() {
                    show_full_preview(&preview, &text, untitled);
                }
            }
        });
    });
    list.set_model(gtk::SelectionModel::NONE);
    list.set_factory(Some(&factory));
    list.set_model(Some(&NoSelection::new(Some(filtered.clone()))));
    let activate = list.connect_activate(move |list, position| {
        navigation::open(list, || {
            let visible_count = filtered.n_items();
            let positions = if visible_count as usize == entries.len() {
                Some((0..entries.len()).collect())
            } else {
                (0..visible_count)
                    .map(|position| {
                        filtered
                            .item(position)
                            .and_downcast::<StringObject>()
                            .and_then(|item| item.string().parse::<usize>().ok())
                    })
                    .collect::<Option<Vec<_>>>()
            };
            let positions = positions?;
            let refresh_model = model.downgrade();
            detail::build(
                entries.clone(),
                positions,
                position as usize,
                session.clone(),
                account_id,
                move |index| {
                    if let Some(model) = refresh_model.upgrade() {
                        model.splice(index as u32, 1, &[&index.to_string()]);
                    }
                },
            )
        })
    });
    (activate, changed)
}

fn show_full_preview(label: &Label, content: &str, untitled: bool) {
    let line = single_line_preview(content, false);
    label.set_visible(!line.is_empty() || untitled);
    label.set_text(if line.is_empty() {
        "这篇日记没有正文"
    } else {
        &line
    });
}

fn single_line_preview(text: &str, partial: bool) -> String {
    let text = text.trim();
    let line = text.split(super::LINE_BREAKS).next().unwrap_or_default();
    let mut preview = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if partial && line.len() == text.len() {
        preview.push('…');
    }
    preview
}
