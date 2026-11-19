use gtk::{
    Widget,
    prelude::{IsA, WidgetExt},
};

use crate::service::diary::{DiaryEntry, DiaryGender};

mod comments;
mod compose;
mod detail;
mod list;
mod page;
mod search;
#[cfg(test)]
mod tests;

const LINE_BREAKS: [char; 5] = ['\n', '\r', '\u{0085}', '\u{2028}', '\u{2029}'];

pub(in crate::view) use page::build;

fn metadata(entry: &DiaryEntry) -> String {
    let author = if entry.is_partner { "对方" } else { "本人" };
    let status = match (entry.is_partner, entry.read.get()) {
        (true, Some(true)) => "已读",
        (true, Some(false)) => "未读",
        (false, Some(true)) => "对方已读",
        (false, Some(false)) => "对方未读",
        (_, None) => "",
    };
    [&*entry.date, author, status, &*entry.weather, &*entry.mood]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn apply_gender_color(widget: &impl IsA<Widget>, gender: Option<DiaryGender>) {
    widget.remove_css_class("diary-boy");
    widget.remove_css_class("diary-girl");
    match gender {
        Some(DiaryGender::Boy) => widget.add_css_class("diary-boy"),
        Some(DiaryGender::Girl) => widget.add_css_class("diary-girl"),
        None => {}
    }
}
