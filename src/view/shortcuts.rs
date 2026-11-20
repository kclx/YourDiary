use gtk::{
    CallbackAction, Shortcut, ShortcutAction, ShortcutController, ShortcutScope, ShortcutTrigger,
    glib::Propagation,
    prelude::{CastNone, GtkWindowExt, IsA, WidgetExt},
};

pub(super) const NAVIGATION: [(&str, &str, i32, &str); 5] = [
    ("<Control>1", "diaries", 0, "切换到日记"),
    ("<Control>2", "treehole", 1, "切换到树洞"),
    ("<Control>3", "profile", 2, "切换到我的"),
    ("<Control>f", "search", 0, "聚焦日记搜索框"),
    ("<Control>n", "compose", 0, "写今天的日记"),
];

pub(super) fn install_quit(application: &adw::Application) {
    use gtk::{
        glib,
        prelude::{ActionMapExt, ApplicationExt, GtkApplicationExt},
    };
    let quit = gtk::gio::SimpleAction::new("quit", None);
    quit.connect_activate(gtk::glib::clone!(
        #[weak]
        application,
        move |_, _| application.quit()
    ));
    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
}

pub(super) fn window_controller() -> ShortcutController {
    let controller = ShortcutController::new();
    controller.set_scope(ShortcutScope::Global);
    bind(
        &controller,
        "<Control>m",
        CallbackAction::new(|widget, _| {
            let Some(window) = widget.root().and_downcast::<gtk::Window>() else {
                return Propagation::Proceed;
            };
            window.minimize();
            Propagation::Stop
        }),
    );
    controller
}

pub(super) fn bind(controller: &ShortcutController, keys: &str, action: impl IsA<ShortcutAction>) {
    let trigger = ShortcutTrigger::parse_string(keys).expect("快捷键格式无效");
    controller.add_shortcut(Shortcut::new(Some(trigger), Some(action)));
}

pub(super) const REFRESH: &str = "page.refresh";
pub(super) const COMPOSE: &str = "diary.compose";

pub(super) fn on_refresh(widget: &impl IsA<gtk::Widget>, run: impl Fn() + 'static) {
    use gtk::gio::{SimpleAction, SimpleActionGroup, prelude::ActionMapExt};
    let group = SimpleActionGroup::new();
    let action = SimpleAction::new("refresh", None);
    action.connect_activate(move |_, _| run());
    group.add_action(&action);
    widget.insert_action_group("page", Some(&group));
}

#[cfg(test)]
pub(super) fn activate(widget: &impl IsA<gtk::Widget>, keys: &str) -> bool {
    use gtk::{
        ShortcutActionFlags,
        gio::prelude::ListModelExt,
        prelude::{ShortcutActionExt, ShortcutTriggerExt},
    };

    let expected = ShortcutTrigger::parse_string(keys).unwrap().to_str();
    let controllers = widget.observe_controllers();
    for idx in 0..controllers.n_items() {
        let Some(controller) = controllers.item(idx).and_downcast::<ShortcutController>() else {
            continue;
        };
        for idx in 0..controller.n_items() {
            let shortcut = controller.item(idx).and_downcast::<Shortcut>().unwrap();
            if shortcut
                .trigger()
                .is_some_and(|trigger| trigger.to_str() == expected)
            {
                return shortcut.action().unwrap().activate(
                    ShortcutActionFlags::EXCLUSIVE,
                    widget,
                    shortcut.arguments().as_ref(),
                );
            }
        }
    }
    panic!("快捷键没有注册：{keys}");
}
