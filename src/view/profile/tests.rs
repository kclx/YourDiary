use super::*;

fn shows(widget: &gtk::Widget, text: &str) -> bool {
    widget
        .downcast_ref::<Label>()
        .is_some_and(|label| label.text() == text)
        || std::iter::successors(widget.first_child(), WidgetExt::next_sibling)
            .any(|child| shows(&child, text))
}

fn switcher(widget: &gtk::Widget) -> Option<gtk::Button> {
    widget
        .clone()
        .downcast::<gtk::Button>()
        .ok()
        .filter(|b| b.has_css_class("profile-switcher"))
        .or_else(|| {
            std::iter::successors(widget.first_child(), WidgetExt::next_sibling)
                .find_map(|child| switcher(&child))
        })
}

#[test]
#[ignore = "需要 GTK 显示后端"]
fn profile_displays_synced_people_and_pair_states() {
    gtk::test_synced(|| {
        adw::init().unwrap();
        crate::view::install_styles();
        let (overlay, update) = build(None);
        let scroll = overlay.child().and_downcast::<ScrolledWindow>().unwrap();
        let mut profile: Profile = serde_json::from_value(serde_json::json!({
                "userid":42,"name":"山月","role":"girl","age_range":"25–30",
                "useremail":"sample@example.com","description":"记录沿途的风景，也记得每一次相遇。",
                "diary_count":128,"word_count":45862,"image_count":36,"is_member":false,"enable_treehole":true,
                "paired":true,"paired_user_config":{"userid":9,"name":"林间","avatar":"","role":"boy",
                "description":"在日记里分享生活。","diary_count":93,"word_count":23120,"image_count":18,
                "paired_time":1710000000,"last_login_time":1780000000,"ts":1}
            })).unwrap();
        update(Some(&profile));
        let window = gtk::Window::builder()
            .default_width(320)
            .default_height(480)
            .child(&overlay)
            .build();
        window.present();
        let context = glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while scroll.vadjustment().page_size() == 0.0 && std::time::Instant::now() < deadline {
            context.iteration(false);
        }
        assert!(scroll.width() > 0 && scroll.width() <= 320);
        assert!(scroll.hadjustment().upper() <= scroll.width() as f64 + 1.0);
        assert!(scroll.vadjustment().upper() > scroll.vadjustment().page_size());
        for text in ["山月", "林间", "45862", "sample@example.com"] {
            assert!(shows(scroll.upcast_ref(), text));
        }
        let toggle = switcher(scroll.upcast_ref()).unwrap();
        let deck = toggle.child().and_downcast::<gtk::Overlay>().unwrap();
        let back = deck.first_child().and_downcast::<gtk::Stack>().unwrap();
        let front = deck
            .last_child()
            .unwrap()
            .first_child()
            .and_downcast::<gtk::Stack>()
            .unwrap();
        assert_eq!(front.visible_child_name().as_deref(), Some("own"));
        assert_eq!(back.visible_child_name().as_deref(), Some("partner"));
        assert!(shows(front.visible_child().unwrap().as_ref(), "45862"));
        let bounds = front.compute_bounds(&deck).unwrap();
        let back_bounds = back.compute_bounds(&deck).unwrap();
        assert!(back_bounds.y() < bounds.y() + bounds.height());
        assert!(
            back_bounds.y() + back_bounds.height() > bounds.y() + bounds.height(),
            "front={bounds:?} back={back_bounds:?} gap={}",
            deck.last_child().unwrap().last_child().unwrap().height()
        );
        assert!(toggle.grab_focus());
        assert!(toggle.activate());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while front.visible_child_name().as_deref() == Some("own")
            && std::time::Instant::now() < deadline
        {
            context.iteration(false);
        }
        assert!(shows(front.visible_child().unwrap().as_ref(), "23120"));
        assert!(shows(front.visible_child().unwrap().as_ref(), "他"));
        assert!(shows(front.visible_child().unwrap().as_ref(), "林间"));
        let account = toggle.next_sibling().unwrap();
        assert!(
            account
                .next_sibling()
                .and_downcast::<Label>()
                .unwrap()
                .text()
                .contains("他最近登录于")
        );
        assert!(shows(&account, "账号信息") && shows(&account, "sample@example.com"));
        let rows = account.last_child().unwrap();
        let copy_rows: Vec<_> = std::iter::successors(rows.first_child(), WidgetExt::next_sibling)
            .filter_map(|row| row.downcast::<gtk::Button>().ok())
            .collect();
        assert_eq!(copy_rows.len(), 2);
        assert!(shows(copy_rows[0].upcast_ref(), "42"));
        assert!(shows(copy_rows[1].upcast_ref(), "sample@example.com"));
        assert!(!shows(&account, "搭档信息"));
        assert_eq!(back.visible_child_name().as_deref(), Some("own"));
        update(Some(&profile));
        let toggle = switcher(scroll.upcast_ref()).unwrap();
        assert!(toggle.tooltip_text().unwrap().contains("山月"));
        toggle.emit_clicked();
        if toggle.has_css_class("drawing-out") {
            toggle.emit_clicked(); // 抽牌期间重复点击仍只切换一次。
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while toggle.has_css_class("drawing-out") && std::time::Instant::now() < deadline {
            context.iteration(false);
        }
        assert!(toggle.tooltip_text().unwrap().contains("林间"));
        profile.partner = None;
        update(Some(&profile));
        assert!(shows(scroll.upcast_ref(), "已配对，搭档资料暂缺"));
        assert!(switcher(scroll.upcast_ref()).is_none());
        profile.paired = false;
        update(Some(&profile));
        assert!(shows(scroll.upcast_ref(), "尚未配对"));
        update(None);
        assert!(shows(scroll.upcast_ref(), "暂时无法获取个人资料"));
        window.close();
    });
}
