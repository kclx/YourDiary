use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

use gtk::{
    CustomFilter, FilterChange, Label, ListItemFactory, ListView, SearchEntry, StringObject, gio,
    glib::{MainContext, clone::Downgrade, markup_escape_text},
    prelude::{Cast, EditableExt, FilterExt, WidgetExt},
};

use crate::service::diary::{DiaryEntry, search_cached};

#[derive(Default)]
pub(super) struct MatchPreview {
    pub title: Option<String>,
    pub body: Option<String>,
}

type Matches = Rc<RefCell<Option<HashMap<usize, MatchPreview>>>>;

pub(super) fn connect(
    list: &ListView,
    entry: &SearchEntry,
    status: &Label,
    diaries: Rc<Vec<DiaryEntry>>,
    account_id: Option<i64>,
) -> (CustomFilter, Matches, gtk::glib::SignalHandlerId) {
    let matches = Matches::default();
    let filter = CustomFilter::new({
        let matches = matches.clone();
        move |item| {
            matches.borrow().as_ref().is_none_or(|matches| {
                item.downcast_ref::<StringObject>()
                    .and_then(|item| item.string().parse::<usize>().ok())
                    .is_some_and(|index| matches.contains_key(&index))
            })
        }
    });
    let filter_weak = filter.downgrade();
    let status_weak = status.downgrade();
    let list_weak = list.downgrade();
    let generation = Rc::new(Cell::new(0_u64));
    let state = matches.clone();
    // GTK 持有可变的已读/版本状态。文本在本次列表刷新周期内只快照一次。
    let texts = std::cell::OnceCell::new();
    let changed = entry.connect_search_changed(move |entry| {
        let (Some(filter), Some(status)) = (filter_weak.upgrade(), status_weak.upgrade()) else {
            return;
        };
        let current = generation.get() + 1;
        generation.set(current);
        let query = entry.text().trim().to_ascii_lowercase();
        if query.is_empty() {
            state.replace(None);
            filter.changed(FilterChange::Different);
            rebind_rows(list_weak.upgrade());
            status.set_visible(false);
            return;
        }
        status.set_text("正在搜索…");
        status.set_visible(true);

        let texts = Arc::clone(texts.get_or_init(|| {
            Arc::new(
                diaries
                    .iter()
                    .map(|d| {
                        (
                            d.author_id.map(|a| (a, d.id)),
                            d.title.clone(),
                            d.content.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }));

        let revisions: Vec<_> = diaries.iter().map(|d| d.revision.get()).collect();
        let generation = generation.clone();
        let state = state.clone();
        let list_weak = list_weak.clone();
        let filter_weak = filter.downgrade();
        let status_weak = status.downgrade();
        let entry_weak = entry.downgrade();
        MainContext::default().spawn_local(async move {
            let needle = query.clone();
            let result = gio::spawn_blocking(move || {
                let cached = match account_id {
                    Some(account) => search_cached(account, &needle).ok(),
                    None => Some(Default::default()),
                };
                let found: HashMap<_, _> = texts
                    .iter()
                    .zip(revisions)
                    .enumerate()
                    .filter_map(|(index, ((key, title, content), revision))| {
                        let cached_match = cached
                            .as_ref()
                            .and_then(|hits| hits.get(&(*key)?))
                            .filter(|hit| {
                                revision.is_none_or(|expected| hit.revision >= Some(expected))
                            });
                        let title = excerpt(title, &needle);
                        let body = cached_match
                            .map(|hit| highlight(&hit.text, &needle, hit.cut_before, hit.cut_after))
                            .or_else(|| excerpt(content, &needle));
                        (title.is_some() || body.is_some())
                            .then_some((index, MatchPreview { title, body }))
                    })
                    .collect();
                (found, cached.is_some())
            })
            .await;
            if generation.get() != current
                || entry_weak
                    .upgrade()
                    .is_none_or(|entry| !entry.text().trim().eq_ignore_ascii_case(&query))
            {
                return;
            }
            let (Some(filter), Some(status)) = (filter_weak.upgrade(), status_weak.upgrade())
            else {
                return;
            };
            let Ok((found, cache_available)) = result else {
                status.set_text("搜索中断，请重试");
                return;
            };
            let text = match (found.len(), cache_available) {
                (0, true) => "没有匹配的日记，试试其他关键词".into(),
                (count, true) => format!("{count} 篇匹配的日记"),
                (count, false) => format!("{count} 篇匹配的日记 · 缓存正文暂不可搜索"),
            };
            state.replace(Some(found));
            filter.changed(FilterChange::Different);
            rebind_rows(list_weak.upgrade());
            status.set_text(&text);
        });
    });
    (filter, matches, changed)
}

pub(super) fn rebind_rows(list: Option<ListView>) {
    if let Some(list) = list {
        if let Some(factory) = list.factory() {
            list.set_factory(None::<&ListItemFactory>);
            list.set_factory(Some(&factory));
        }
    }
}

fn excerpt(text: &str, query: &str) -> Option<String> {
    let hit = text.to_ascii_lowercase().find(query)?;
    let end = hit + query.len();
    let left = text[..hit]
        .char_indices()
        .rev()
        .nth(17)
        .map_or(0, |(index, _)| index);
    let right = text[end..]
        .char_indices()
        .nth(28)
        .map_or(text.len(), |(index, _)| end + index);
    Some(highlight(
        &text[left..right],
        query,
        left > 0,
        right < text.len(),
    ))
}

fn highlight(text: &str, query: &str, cut_before: bool, cut_after: bool) -> String {
    let hit = text.to_ascii_lowercase().find(query).unwrap_or(0);
    let start = text[..hit]
        .rsplit_once(super::LINE_BREAKS)
        .map_or(0, |(_, suffix)| hit - suffix.len());
    let end = text[hit..]
        .find(super::LINE_BREAKS)
        .map_or(text.len(), |i| hit + i);
    let cut_before = cut_before && start == 0;
    let cut_after = cut_after && end == text.len();
    let text = &text[start..end];
    let mut markup = String::new();
    let escape = |text: &str| markup_escape_text(&text.replace('\t', " "));
    if cut_before {
        markup.push('…');
    }
    let mut end = 0;
    for (start, _) in text.to_ascii_lowercase().match_indices(query) {
        markup.push_str(&escape(&text[end..start]));
        end = start + query.len();
        markup.push_str(r##"<span background="#f5d76e" foreground="#211900" weight="bold">"##);
        markup.push_str(&escape(&text[start..end]));
        markup.push_str("</span>");
    }
    markup.push_str(&escape(&text[end..]));
    if cut_after {
        markup.push('…');
    }
    markup
}
