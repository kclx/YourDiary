use adw::{NavigationPage, NavigationView};
use gtk::{Align, Button, Widget, prelude::*};

pub(super) fn section(root: &impl IsA<Widget>, title: &str) -> NavigationView {
    let nav = NavigationView::new();
    nav.add(&NavigationPage::new(root, title));
    nav
}

pub(super) fn open(from: &impl IsA<Widget>, build: impl FnOnce() -> Option<NavigationPage>) {
    if let Some(nav) = from
        .ancestor(NavigationView::static_type())
        .and_downcast::<NavigationView>()
        && nav.navigation_stack().n_items() == 1
        && let Some(detail) = build()
    {
        nav.push(&detail);
    }
}

pub(super) fn back_button(label: &str) -> Button {
    Button::builder()
        .label(label)
        .halign(Align::Start)
        .action_name("navigation.pop")
        .css_classes(["flat"])
        .build()
}
