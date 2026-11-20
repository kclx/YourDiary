use adw::{
    ActionRow, Application, ApplicationWindow, HeaderBar, NavigationPage, NavigationSplitView,
    NavigationView,
    prelude::{ActionRowExt, AdwApplicationWindowExt, MessageDialogExt, NavigationPageExt},
};
use gtk::{
    Box, ListBox, NamedAction,
    Orientation::Vertical,
    SearchEntry, Stack,
    gio::{
        SimpleAction, SimpleActionGroup,
        prelude::{ActionMapExt, ListModelExt},
    },
    glib::clone::Downgrade,
    prelude::{
        BoxExt, ButtonExt, Cast, CastNone, GtkWindowExt, IsA, ListBoxRowExt, StaticType, ToValue,
        WidgetExt,
    },
};

use crate::service::auth::{Session, SessionStore};
use std::sync::Arc;

use super::{diary, login, navigation, preferences, profile, shortcuts, treehole};

/// 构建主窗口及侧边栏，app 由调用方借出。
pub(super) fn build(app: &Application, session: Session, store: SessionStore) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("你的日记")
        .default_width(1100)
        .default_height(760)
        .width_request(360)
        .height_request(320)
        .build();
    let window_weak = window.downgrade();
    let login_store = store.clone();
    let (my_content, update_profile) = profile::build(session.profile());
    let session = Arc::new(session);
    install_logout(&window, session.clone(), store.clone());
    preferences::export::install(&window, session.clone());
    let (diary_content, diary_search) =
        diary::build(session.clone(), store, update_profile, move || {
            if let Some(window) = window_weak.upgrade() {
                return_to_login(&window, &login_store);
            }
        });
    let search = diary_search.downgrade();
    shortcuts::on_refresh(&my_content, move || {
        if let Some(search) = search.upgrade() {
            let _ = search.activate_action(shortcuts::REFRESH, None);
        }
    });
    let treehole_content = treehole::build(session);
    let page_stack = Stack::builder().vexpand(true).build();
    page_stack.add_named(&diary_content, Some("diary"));
    page_stack.add_named(&treehole_content, Some("treehole"));
    page_stack.add_named(&my_content, Some("my"));
    page_stack.set_visible_child_name("diary");

    install_navigation(&window, &page_stack, &diary_search);
    window.present();
}

fn return_to_login(window: &ApplicationWindow, store: &SessionStore) {
    if window.get_visible()
        && let Some(app) = window.application().and_downcast::<Application>()
    {
        login::build(&app, store.clone(), build);
        window.close();
    }
}

fn install_logout(window: &ApplicationWindow, session: Arc<Session>, store: SessionStore) {
    let logout = SimpleAction::new("logout", None);
    let parent = window.downgrade();
    logout.connect_activate(move |_, _| {
        let Some(window) = parent.upgrade() else {
            return;
        };
        match store.forget(session.token()) {
            Ok(()) => {
                session.end();
                return_to_login(&window, &store);
            }
            Err(error) => {
                eprintln!("无法清除登录会话：{error}");
                let dialog = adw::MessageDialog::new(
                    Some(&window),
                    Some("退出登录失败"),
                    Some("无法清除本机的登录凭证，请重试。"),
                );
                dialog.add_response("close", "关闭");
                dialog.present();
            }
        }
    });
    window.add_action(&logout);
}

fn install_navigation(window: &ApplicationWindow, page_stack: &Stack, diary_search: &SearchEntry) {
    let split_view = NavigationSplitView::new();
    let compact =
        adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").unwrap());
    compact.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(compact);
    let content = Box::new(Vertical, 0);
    content.append(&HeaderBar::new());
    content.append(page_stack);
    let content_page = NavigationPage::new(&content, "日记");

    let navigation_list = ListBox::new();
    navigation_list.add_css_class("navigation-sidebar");
    let destinations = [
        ("diary", "日记", "accessories-text-editor-symbolic"),
        ("treehole", "树洞", "system-users-symbolic"),
        ("my", "我的", "avatar-default-symbolic"),
    ];
    for (_, title, icon) in destinations {
        let row = ActionRow::builder().title(title).activatable(true).build();
        row.add_css_class("heading");
        row.add_prefix(&gtk::Image::from_icon_name(icon));
        navigation_list.append(&row);
    }
    navigation_list.select_row(navigation_list.row_at_index(0).as_ref());
    let split = split_view.downgrade();
    navigation_list.connect_row_activated(move |_, _| {
        if let Some(split) = split.upgrade() {
            split.set_show_content(true);
        }
    });
    let stack_for_selection = page_stack.downgrade();
    let page_for_selection = content_page.downgrade();
    navigation_list.connect_row_selected(move |_, row| {
        if let Some((name, title, _)) = row.and_then(|row| destinations.get(row.index() as usize))
            && let (Some(stack), Some(page)) =
                (stack_for_selection.upgrade(), page_for_selection.upgrade())
        {
            stack.set_visible_child_name(name);
            page.set_title(title);
        }
    });
    let sidebar_header = HeaderBar::new();
    let sidebar = Box::new(Vertical, 0);
    sidebar.append(&sidebar_header);
    navigation_list.set_vexpand(true);
    sidebar.append(&navigation_list);
    let sections = navigation::section(&split_view, "你的日记");
    sidebar.append(&preferences_button(&sections));
    let sidebar_page = NavigationPage::new(&sidebar, "导航");
    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));
    split_view.set_show_content(true);
    window.set_content(Some(&sections));
    install_shortcuts(
        window,
        &navigation_list,
        &split_view,
        diary_search,
        page_stack,
        &sections,
    );
}

fn preferences_button(sections: &NavigationView) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("偏好设置")
        .css_classes(["flat"])
        .halign(gtk::Align::Start)
        .width_request(44)
        .height_request(44)
        .margin_start(8)
        .margin_bottom(8)
        .build();
    let sections = sections.downgrade();
    button.connect_clicked(move |_| {
        if let Some(nav) = sections.upgrade()
            && nav.navigation_stack().n_items() == 1
        {
            nav.push(&preferences::page());
        }
    });
    button
}

fn install_shortcuts(
    window: &impl IsA<gtk::Widget>,
    navigation: &ListBox,
    split_view: &NavigationSplitView,
    search: &SearchEntry,
    pages: &Stack,
    sections: &NavigationView,
) {
    let group = SimpleActionGroup::new();
    let controller = shortcuts::window_controller();
    for (key, back) in [("<Control>r", false), ("Escape", true)] {
        let pages = pages.downgrade();
        let sections = sections.downgrade();
        shortcuts::bind(
            &controller,
            key,
            gtk::CallbackAction::new(move |_, _| {
                if let Some(nav) = sections.upgrade()
                    && nav.navigation_stack().n_items() > 1
                {
                    return (back && nav.pop()).into();
                }
                let Some(section) = pages.upgrade().and_then(|p| p.visible_child()) else {
                    return gtk::glib::Propagation::Proceed;
                };
                let nav = section.downcast_ref::<adw::NavigationView>();
                if back {
                    return nav.is_some_and(|nav| nav.pop()).into();
                }
                let target = nav
                    .and_then(|nav| nav.visible_page())
                    .and_then(|page| page.child())
                    .unwrap_or(section);
                target
                    .activate_action(shortcuts::REFRESH, None)
                    .is_ok()
                    .into()
            }),
        );
    }
    for (keys, name, index, _) in shortcuts::NAVIGATION {
        let action = SimpleAction::new(name, None);
        let navigation = navigation.downgrade();
        let split_view = split_view.downgrade();
        let search = search.downgrade();
        let sections = sections.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(nav) = sections.upgrade()
                && nav.navigation_stack().n_items() > 1
            {
                nav.pop();
            }
            if let Some(navigation) = navigation.upgrade() {
                navigation.select_row(navigation.row_at_index(index).as_ref());
            }
            if let Some(split_view) = split_view.upgrade() {
                split_view.set_show_content(true);
            }
            if name == "compose" {
                if let Some(search) = search.upgrade() {
                    let _ = search.activate_action(shortcuts::COMPOSE, None);
                }
            } else if name == "search" {
                if let Some(search) = search.upgrade() {
                    if let Some(page) = search
                        .ancestor(NavigationPage::static_type())
                        .and_downcast::<NavigationPage>()
                        && let Some(nav) = page
                            .ancestor(adw::NavigationView::static_type())
                            .and_downcast::<adw::NavigationView>()
                        && nav.visible_page().as_ref() != Some(&page)
                    {
                        nav.pop_to_page(&page);
                    }
                    search.grab_focus();
                }
            }
        });
        group.add_action(&action);
        shortcuts::bind(
            &controller,
            keys,
            NamedAction::new(&format!("journal.{name}")),
        );
    }
    window.insert_action_group("journal", Some(&group));
    window.add_controller(controller);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn preferences_logout_clears_session_and_shows_login() {
        gtk::test_synced(|| {
            use gtk::prelude::{ActionableExt, ApplicationExt, GtkApplicationExt};
            adw::init().unwrap();
            let dir = std::env::temp_dir().join(format!("yourdiary-logout-{}", std::process::id()));
            let store = SessionStore::at(dir.join("yourdiary.sqlite3"));
            store.save("test-session", Some(7)).unwrap();
            let session = Session::restore(&store).unwrap().unwrap();
            let app = Application::builder()
                .application_id("com.orlando.yourdiary.LogoutTest")
                .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
                .build();
            app.register(gtk::gio::Cancellable::NONE).unwrap();
            let page = preferences::page();
            let window = ApplicationWindow::builder()
                .application(&app)
                .content(&page)
                .build();
            let session = Arc::new(session);
            let treehole = treehole::build(session.clone());
            drop(treehole);
            assert_eq!(
                Arc::strong_count(&session),
                1,
                "未打开的树洞应随窗口释放会话"
            );
            install_logout(&window, session.clone(), store.clone());
            window.present();
            fn logout_row(widget: gtk::Widget) -> Option<ActionRow> {
                if let Some(row) = widget.downcast_ref::<ActionRow>()
                    && row.action_name().as_deref() == Some("win.logout")
                {
                    return Some(row.clone());
                }
                std::iter::successors(widget.first_child(), WidgetExt::next_sibling)
                    .find_map(logout_row)
            }
            logout_row(page.upcast()).unwrap().emit_activate();
            assert!(!session.is_active());
            assert!(Session::restore(&store).unwrap().is_none());
            assert!(!window.get_visible());
            let windows = app.windows();
            assert_eq!(windows.len(), 1);
            assert_eq!(windows[0].title().as_deref(), Some("登录"));
            windows[0].close();
            std::fs::remove_dir_all(dir).unwrap();
        });
    }

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn window_shortcuts_select_sections_and_focus_search() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            let app = Application::builder()
                .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
                .build();
            shortcuts::install_quit(&app);
            use gtk::gio::prelude::{ActionGroupExt, ApplicationExt};
            use gtk::prelude::{ApplicationExtManual, GtkApplicationExt};
            app.connect_activate(|app| {
                assert!(
                    app.actions_for_accel("<Control>q")
                        .iter()
                        .any(|action| action == "app.quit")
                );
                app.activate_action("quit", None);
            });
            assert_eq!(app.run_with_args::<&str>(&[]), gtk::glib::ExitCode::SUCCESS);
            let search = SearchEntry::new();
            let nav = crate::view::navigation::section(&search, "日记");
            let compose = SimpleAction::new("compose", None);
            let compose_count = std::rc::Rc::new(std::cell::Cell::new(0));
            let count = compose_count.clone();
            compose.connect_activate(move |_, _| count.set(count.get() + 1));
            let diary_actions = SimpleActionGroup::new();
            diary_actions.add_action(&compose);
            nav.insert_action_group("diary", Some(&diary_actions));
            let pages = Stack::new();
            pages.add_named(&nav, Some("diary"));
            for name in ["treehole", "my"] {
                pages.add_named(&gtk::Label::new(Some(name)), Some(name));
            }
            let window = ApplicationWindow::builder()
                .width_request(360)
                .height_request(320)
                .default_width(480)
                .default_height(480)
                .build();
            install_navigation(&window, &pages, &search);
            window.present();
            let sections = window.content().and_downcast::<NavigationView>().unwrap();
            let split = sections
                .visible_page()
                .unwrap()
                .child()
                .and_downcast::<NavigationSplitView>()
                .unwrap();
            let navigation = split
                .sidebar()
                .unwrap()
                .child()
                .unwrap()
                .first_child()
                .unwrap()
                .next_sibling()
                .and_downcast::<ListBox>()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !split.is_collapsed() && std::time::Instant::now() < deadline {
                gtk::glib::MainContext::default().iteration(false);
            }
            assert!(split.is_collapsed() && split.shows_content());
            split.set_show_content(false);
            let row = navigation.row_at_index(0).unwrap();
            assert!(row.is_activatable());
            row.emit_activate();
            assert!(split.shows_content());
            assert!(shortcuts::activate(&window, "<Control>2"));
            assert!(shortcuts::activate(&window, "<Control>n"));
            assert_eq!(pages.visible_child_name().as_deref(), Some("diary"));
            assert_eq!(compose_count.get(), 1);
            for (keys, index) in [("<Control>3", 2), ("<Control>2", 1), ("<Control>1", 0)] {
                assert!(shortcuts::activate(&window, keys));
                assert_eq!(navigation.selected_row().unwrap().index(), index);
            }
            assert_eq!(pages.visible_child_name().as_deref(), Some("diary"));
            nav.push(&NavigationPage::new(
                &gtk::Button::with_label("返回日记"),
                "日记详情",
            ));
            let hit = std::rc::Rc::new(std::cell::Cell::new(0));
            let detail = nav.visible_page().unwrap();
            let value = hit.clone();
            shortcuts::on_refresh(&detail.child().unwrap(), move || value.set(1));
            let value = hit.clone();
            shortcuts::on_refresh(&search, move || value.set(2));
            assert!(shortcuts::activate(&window, "<Control>r"));
            assert_eq!(hit.get(), 1);
            let gear = preferences_button(&sections);
            gear.emit_clicked();
            gear.emit_clicked();
            assert_eq!(sections.navigation_stack().n_items(), 2);
            assert_eq!(sections.visible_page().unwrap().title(), "偏好设置");
            assert!(shortcuts::activate(&window, "Escape"));
            assert_eq!(sections.navigation_stack().n_items(), 1);
            assert_eq!(nav.visible_page().unwrap(), detail);

            assert!(shortcuts::activate(&window, "Escape"));
            assert_eq!(nav.visible_page().unwrap().title(), "日记");
            assert!(!shortcuts::activate(&window, "Escape"));
            assert!(shortcuts::activate(&window, "<Control>r"));
            assert_eq!(hit.get(), 2);
            nav.push(&detail);
            assert!(shortcuts::activate(&window, "<Control>f"));
            assert_eq!(nav.visible_page().unwrap().title(), "日记");
            assert_eq!(navigation.selected_row().unwrap().index(), 0);
            assert!(GtkWindowExt::focus(&window).is_some_and(|focus| focus.is_ancestor(&search)));
            assert!(shortcuts::activate(&window, "<Control>m"));
            window.close();
        });
    }
}
