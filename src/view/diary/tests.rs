use super::page::{scrollable, watch_sync};
use crate::service::diary::{CacheProgress, DiaryEntry, DiaryGender, SyncEvent, SyncedDiaries};
use crate::view::shortcuts;
use adw::prelude::NavigationPageExt;
use gtk::{
    Box, Label, ListView,
    Orientation::Vertical,
    ProgressBar, SearchEntry,
    glib::{MainContext, variant::ToVariant},
    prelude::*,
};
use std::{
    cell::Cell,
    sync::mpsc::channel,
    thread::sleep,
    time::{Duration, Instant},
};

fn collect_labels(widget: &gtk::Widget, labels: &mut Vec<Label>) {
    if let Some(label) = widget.downcast_ref::<Label>() {
        labels.push(label.clone());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        collect_labels(&widget, labels);
        child = widget.next_sibling();
    }
}

fn find_button(widget: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button.clone());
        }
    }
    std::iter::successors(widget.first_child(), WidgetExt::next_sibling)
        .find_map(|child| find_button(&child, label))
}

#[test]
#[ignore = "需要 GTK 显示后端"]
fn large_diary_list_reuses_rows_and_scrolls_to_the_end() {
    gtk::test_synced(|| {
        adw::init().unwrap();
        crate::view::install_styles();
        let list = ListView::builder().single_click_activate(true).build();
        let scroller = scrollable(&list);
        let status = Label::new(Some("正在同步日记…"));
        let bar = ProgressBar::new();
        let search_entry = SearchEntry::new();
        let search_status = Label::new(None);
        let page = Box::new(Vertical, 0);
        page.append(&status);
        page.append(&bar);
        page.append(&search_entry);
        page.append(&search_status);
        page.append(&scroller);
        let nav = crate::view::navigation::section(&page, "日记");
        let root = nav.visible_page().unwrap();
        let window = gtk::Window::builder()
            .default_width(380)
            .default_height(500)
            .child(&nav)
            .build();
        let mut entries: Vec<_> = (0..10_000)
            .map(|index| DiaryEntry {
                id: index + 1,
                author_id: None,
                revision: Cell::new(None),
                read: Cell::new(Some(index % 2 == 0)),
                title: format!("日记 {index}").into(),
                date: String::new(),
                weather_code: None,
                mood_id: None,
                weather: String::new(),
                mood: String::new(),
                content: "".into(),
                has_full_content: true,
                is_partner: index % 2 == 1,
                gender: Some(if index % 2 == 1 {
                    DiaryGender::Girl
                } else {
                    DiaryGender::Boy
                }),
            })
            .collect();
        entries[1].title = "".into();
        entries[1].content = "\n \t\r\n无标题日记的摘要\n继续阅读".into();
        entries[2].content = " \n\t".into();
        entries[3].title = "".into();
        entries[3].content = " \n\t".into();
        entries[5].content = "共同关键词".into();
        entries[9].content = "共同关键词".into();
        let content = format!(
            "正。一段实际的日记内容。🧑🏽‍💻这段首行文字足够长，可以验证窗口边界的省略效果。\nhttps://example.com/{}\n你好&世界",
            "long".repeat(1000)
        );
        entries.last_mut().unwrap().content = content.clone().into();
        let (sender, receiver) = channel();
        let listen = watch_sync(
            &list,
            &status,
            &bar,
            (&search_entry, &search_status),
            None,
            |_| {},
            || panic!("模拟同步不应要求登录"),
        );
        listen(receiver);
        let mut replacement = entries[0].clone();
        replacement.title = "刷新后的日记".into();
        sender
            .send(SyncEvent::List(Ok(SyncedDiaries {
                moods: vec![],
                account_id: None,
                entries,
                profile: None,
            })))
            .unwrap();
        sender
            .send(SyncEvent::Progress(CacheProgress {
                completed: 1,
                total: 2,
            }))
            .unwrap();
        window.present();

        let context = MainContext::default();
        let wait_for = |message: &str, ready: &dyn Fn() -> bool| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready() && Instant::now() < deadline {
                context.iteration(false);
                sleep(Duration::from_millis(2));
            }
            assert!(ready(), "{message}");
        };
        wait_for("同步进度条未更新", &|| {
            bar.is_visible() && bar.fraction() == 0.5 && status.text().contains("1/2")
        });
        sender
            .send(SyncEvent::Complete(Some(CacheProgress {
                completed: 2,
                total: 2,
            })))
            .unwrap();
        wait_for("同步结束后未恢复日记计数", &|| {
            !bar.is_visible() && status.text() == "10000 篇日记"
        });
        let adjustment = scroller.vadjustment();
        wait_for("列表未获得可滚动的视口", &|| {
            adjustment.page_size() > 0.0 && adjustment.upper() > adjustment.page_size()
        });
        let viewport = scroller.compute_bounds(&page).unwrap();
        let cards = list.compute_bounds(&page).unwrap();
        assert_eq!(viewport.x() + viewport.width(), page.width() as f32);
        assert!(cards.x() + cards.width() <= page.width() as f32 - 24.);
        let mut labels = Vec::new();
        collect_labels(list.upcast_ref(), &mut labels);
        assert!(labels.iter().any(|title| {
            title.text() == "日记 0" && title.parent().unwrap().has_css_class("diary-boy")
        }));
        for (text, hint) in [("已读", "对方已读"), ("• 未读", "我未读")] {
            let badge = labels
                .iter()
                .find(|label| label.text() == text && label.has_css_class("diary-receipt"))
                .unwrap();
            assert_eq!(badge.tooltip_text().as_deref(), Some(hint));
            assert_eq!(badge.has_css_class("unread"), text == "• 未读");
            assert!(badge.prev_sibling().unwrap().hexpands());
        }
        assert!(labels.iter().any(|title| {
            title.text() == "无标题日记的摘要" && title.has_css_class("diary-preview")
        }));
        for (title, visible) in [("日记 2", false), ("", true)] {
            assert!(labels.iter().any(|label| {
                label.text() == "这篇日记没有正文"
                    && label.get_visible() == visible
                    && label
                        .prev_sibling()
                        .and_downcast::<Label>()
                        .is_some_and(|l| l.text() == title)
            }));
        }
        assert_eq!(list.model().unwrap().n_items(), 10_000);
        let rows = list.observe_children();
        assert!(rows.n_items() < 1_000);

        let last_position = 9999_u32.to_variant();
        list.activate_action("list.scroll-to-item", Some(&last_position))
            .unwrap();
        wait_for("最后一篇日记未绑定到可见行", &|| {
            let mut labels = Vec::new();
            collect_labels(list.upcast_ref(), &mut labels);
            labels.iter().any(|title| {
                title.text() == "日记 9999"
                    && title.parent().unwrap().has_css_class("diary-girl")
                    && !title.parent().unwrap().has_css_class("diary-boy")
            })
        });
        assert!(rows.n_items() < 1_000);
        let preview_text = content.lines().next().unwrap();
        wait_for("长正文预览未缩为带省略号的一行", &|| {
            let mut labels = Vec::new();
            collect_labels(list.upcast_ref(), &mut labels);
            labels.iter().any(|label| {
                label.text() == preview_text
                    && label.height() > 0
                    && label.layout().line_count() == 1
                    && label.layout().is_ellipsized()
            })
        });
        let mut labels = Vec::new();
        collect_labels(list.upcast_ref(), &mut labels);
        let preview = labels
            .iter()
            .find(|label| label.text() == preview_text)
            .unwrap();
        let (_, y) = preview.layout_offsets();
        let (ink, logical) = preview.layout().pixel_extents();
        assert!(
            y + ink.y() >= 3 && y + ink.y() + ink.height() <= preview.height() - 3,
            "正文字形的上下留白不足，预览高度 {}，布局偏移 {y}，字形范围 {ink:?}",
            preview.height()
        );
        assert!(preview.height() >= logical.height());

        search_entry.set_text("你好");
        wait_for("搜索结果未过滤到唯一日记", &|| {
            list.model().unwrap().n_items() == 1 && search_status.text() == "1 篇匹配的日记"
        });
        wait_for("正文命中片段未高亮", &|| {
            let mut labels = Vec::new();
            collect_labels(list.upcast_ref(), &mut labels);
            labels.iter().any(|label| {
                label.text() == "你好&世界"
                    && !label.text().contains("一段实际的日记内容")
                    && label.label().contains(">你好</span>&amp;世界")
            })
        });
        list.activate_action("list.activate-item", Some(&0_u32.to_variant()))
            .unwrap();
        let detail = nav.visible_page().unwrap();
        assert_eq!(detail.title(), "日记详情");
        assert_eq!(nav.navigation_stack().n_items(), 2);
        list.emit_by_name::<()>("activate", &[&0_u32]);
        assert_eq!(nav.visible_page().as_ref(), Some(&detail));
        let mut labels = Vec::new();
        collect_labels(detail.upcast_ref(), &mut labels);
        assert!(labels.iter().any(|title| {
            title.text() == "日记 9999" && title.parent().unwrap().has_css_class("diary-girl")
        }));
        let body = labels.iter().find(|label| label.text() == content).unwrap();
        wait_for("长正文未在可视宽度内自动换行", &|| {
            body.width() > 0
                && body.layout().line_count() > 2
                && body.layout().pixel_size().0 <= body.width()
        });
        assert!(
            !find_button(detail.upcast_ref(), "上一篇")
                .unwrap()
                .is_sensitive()
        );
        assert!(
            !find_button(detail.upcast_ref(), "下一篇")
                .unwrap()
                .is_sensitive()
        );

        find_button(detail.upcast_ref(), "返回日记")
            .unwrap()
            .emit_clicked();
        assert_eq!(nav.visible_page().as_ref(), Some(&root));
        assert_eq!(search_entry.text(), "你好");
        assert_eq!(list.model().unwrap().n_items(), 1);
        search_entry.set_text("共同关键词");
        wait_for("未过滤到两篇日记", &|| {
            list.model().unwrap().n_items() == 2
        });
        list.activate_action("list.activate-item", Some(&0_u32.to_variant()))
            .unwrap();
        let detail = nav.visible_page().unwrap();
        let previous = find_button(detail.upcast_ref(), "上一篇").unwrap();
        let next = find_button(detail.upcast_ref(), "下一篇").unwrap();
        assert!(!previous.is_sensitive());
        assert!(next.is_sensitive());
        assert!(!shortcuts::activate(&detail, "<Control>minus"));
        assert!(shortcuts::activate(&detail, "<Control>plus"));
        let mut labels = Vec::new();
        collect_labels(detail.upcast_ref(), &mut labels);
        assert!(labels.iter().any(|label| label.text() == "日记 9"));
        assert!(previous.is_sensitive());
        assert!(!next.is_sensitive());

        assert!(shortcuts::activate(&detail, "<Control>minus"));
        let mut labels = Vec::new();
        collect_labels(detail.upcast_ref(), &mut labels);
        assert!(labels.iter().any(|label| label.text() == "日记 5"));
        assert!(nav.pop());
        assert_eq!(search_entry.text(), "共同关键词");
        search_entry.set_text("");
        wait_for("清空搜索后未恢复完整列表", &|| {
            list.model().unwrap().n_items() == 10_000 && !search_status.is_visible()
        });
        list.activate_action("list.scroll-to-item", Some(&last_position))
            .unwrap();
        wait_for("未滚动至列表末尾", &|| {
            adjustment.value() + adjustment.page_size() >= adjustment.upper() - 1.
        });
        let offset = adjustment.value();
        list.emit_by_name::<()>("activate", &[&9999_u32]);
        let detail = nav.visible_page().unwrap();
        let mut labels = Vec::new();
        collect_labels(detail.upcast_ref(), &mut labels);
        assert!(labels.iter().any(|title| title.text() == "日记 9999"));
        find_button(detail.upcast_ref(), "返回日记")
            .unwrap()
            .emit_clicked();
        wait_for("返回后未恢复列表位置", &|| {
            list.is_mapped() && (adjustment.value() - offset).abs() < 1.
        });
        assert_eq!(nav.navigation_stack().n_items(), 1);
        search_entry.set_text("刷新后");
        let (sender, receiver) = channel();
        listen(receiver);
        sender
            .send(SyncEvent::List(Ok(SyncedDiaries {
                moods: vec![],
                account_id: None,
                entries: vec![replacement],
                profile: None,
            })))
            .unwrap();
        sender
            .send(SyncEvent::Complete(Some(CacheProgress {
                completed: 0,
                total: 0,
            })))
            .unwrap();
        wait_for("刷新后未沿用搜索词", &|| {
            status.text() == "1 篇日记" && search_status.text() == "1 篇匹配的日记"
        });
        list.emit_by_name::<()>("activate", &[&0_u32]);
        let mut labels = Vec::new();
        collect_labels(nav.visible_page().unwrap().upcast_ref(), &mut labels);
        assert!(labels.iter().any(|label| label.text() == "刷新后的日记"));
        window.close();
    });
}

#[test]
#[ignore = "需要 GTK 显示后端"]
fn composer_preserves_failed_draft_and_submits_once() {
    gtk::test_synced(|| {
        adw::init().unwrap();
        let created = std::rc::Rc::new(Cell::new(0));
        let completed = created.clone();
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let requests = calls.clone();
        let dirty = std::rc::Rc::new(Cell::new(false));
        let page = super::compose::page(
            crate::service::diary::today(),
            dirty.clone(),
            |date| {
                Ok(crate::service::diary::TodayDiary {
                    entry: crate::service::diary::DiaryWrite {
                        id: Some(42),
                        date: date.into(),
                        title: "原标题".into(),
                        content: "原有全文".into(),
                        weather: Some("rainy".into()),
                        mood_id: Some(37),
                    },
                    moods: vec![(37, "期待".into())],
                    mood_name: "期待".into(),
                })
            },
            move |original, entry| {
                assert_eq!(original.id, Some(42));
                assert_eq!(entry.id, original.id);
                let mut calls = requests.lock().unwrap();
                calls.push(entry);
                if calls.len() == 1 {
                    Err(crate::service::diary::WriteError::Retry("暂时无法保存"))
                } else {
                    Ok(42)
                }
            },
            move || completed.set(completed.get() + 1),
        );
        let nav = crate::view::navigation::section(&Label::new(Some("日记列表")), "日记");
        let list_direction = nav.direction();
        nav.push(&page);
        assert_eq!(nav.direction(), gtk::TextDirection::Rtl);
        assert_eq!(page.direction(), list_direction);
        let window = gtk::Window::builder()
            .child(&nav)
            .default_width(380)
            .default_height(510)
            .build();
        window.present();
        fn widgets<T: IsA<gtk::Widget> + Clone>(root: &gtk::Widget) -> Vec<T> {
            let mut found: Vec<_> = root.downcast_ref::<T>().cloned().into_iter().collect();
            for child in std::iter::successors(root.first_child(), WidgetExt::next_sibling) {
                found.extend(widgets::<T>(&child));
            }
            found
        }
        let editor = page.upcast_ref();
        let wait_for = |ready: &dyn Fn() -> bool| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while !ready() && Instant::now() < deadline {
                MainContext::default().iteration(false);
                sleep(Duration::from_millis(2));
            }
            assert!(ready());
        };
        wait_for(&|| find_button(editor, "保存日记").is_some());
        let text = widgets::<gtk::TextView>(editor).pop().unwrap();
        let save = find_button(editor, "保存日记").unwrap();
        assert!(save.is_sensitive());
        assert!(widgets::<gtk::Calendar>(editor).is_empty());
        let initial = text.buffer();
        assert_eq!(
            initial.text(&initial.start_iter(), &initial.end_iter(), true),
            "原有全文"
        );
        let title = widgets::<adw::EntryRow>(editor).pop().unwrap();
        assert_eq!(title.text(), "原标题");
        title.set_text("日落");
        for row in widgets::<adw::ComboRow>(editor) {
            use adw::prelude::ComboRowExt;
            row.set_selected(1);
        }
        let buffer = text.buffer();
        buffer.set_text("前占位后");
        assert!(dirty.get());
        buffer.select_range(&buffer.iter_at_offset(1), &buffer.iter_at_offset(3));
        let local_time = || {
            gtk::glib::DateTime::now_local()
                .unwrap()
                .format("前记录于%Y年%m月%d日 %H:%M后")
                .unwrap()
        };
        let before = local_time();
        assert!(shortcuts::activate(&text, "<Control>t"));
        let stamped = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        assert!(stamped == before || stamped == local_time());
        assert_eq!(buffer.cursor_position(), buffer.char_count() - 1);
        buffer.undo();
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
            "前占位后"
        );
        buffer.set_text("  \n ");
        save.emit_clicked();
        assert!(calls.lock().unwrap().is_empty());
        buffer.set_text("第一行\n第二行");
        assert!(nav.pop());
        nav.push(&page);
        save.emit_clicked();
        save.emit_clicked();
        wait_for(&|| save.is_sensitive());
        assert_eq!(
            &*buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
            "第一行\n第二行"
        );
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(nav.visible_page(), Some(page.clone()));
        save.emit_clicked();
        wait_for(&|| created.get() == 1 && nav.direction() == list_direction);
        assert!(!dirty.get());
        assert_eq!(nav.navigation_stack().n_items(), 1);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let last = calls.last().unwrap();
        assert_eq!(
            (
                last.title.as_str(),
                last.content.as_str(),
                last.weather.as_deref(),
                last.mood_id
            ),
            ("日落", "第一行\n第二行", Some("sunny"), Some(37))
        );
        assert_eq!(
            last.date,
            gtk::glib::DateTime::now_local()
                .unwrap()
                .format("%Y-%m-%d")
                .unwrap()
                .as_str()
        );
        window.close();
    });
}

#[test]
#[ignore = "需要 GTK 显示后端"]
fn reopening_stale_draft_requires_explicit_discard() {
    gtk::test_synced(|| {
        use adw::prelude::MessageDialogExt;
        adw::init().unwrap();
        let nav = crate::view::navigation::section(&Label::new(None), "日记");
        let window = gtk::Window::builder().child(&nav).build();
        let count = std::rc::Rc::new(Cell::new(0));
        let created = count.clone();
        super::page::install_compose(&nav, move |date, dirty| {
            let entry = gtk::Entry::new();
            entry.connect_changed(move |_| dirty.set(true));
            let page = adw::NavigationPage::new(&entry, "写日记");
            page.set_tag(Some(&date));
            created.set(created.get() + 1);
            page
        });
        window.present();
        let open = || nav.activate_action(shortcuts::COMPOSE, None).unwrap();
        let prompt = || {
            gtk::Window::list_toplevels()
                .into_iter()
                .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                .find(|w| w.transient_for().as_ref() == Some(&window) && w.get_visible())
                .unwrap()
        };
        open();
        let old = nav.visible_page().unwrap();
        let entry = old.child().and_downcast::<gtk::Entry>().unwrap();
        entry.set_text("留到午夜的草稿");
        old.set_tag(Some("2000-01-01"));
        nav.pop();
        open();
        assert_eq!(count.get(), 1);
        prompt().response("keep");
        assert_eq!(nav.visible_page(), Some(old.clone()));
        assert_eq!(entry.text(), "留到午夜的草稿");
        nav.pop();
        open();
        prompt().response("discard");
        assert_eq!(count.get(), 2);
        assert_ne!(nav.visible_page(), Some(old));
        window.close();
    });
}
