use adw::Application;
use gtk::{
    gio::prelude::{ApplicationExt, ApplicationExtManual},
    prelude::{GtkApplicationExt, GtkWindowExt},
};

use crate::service::auth::{Session, SessionStore};

mod diary;
mod login;
mod navigation;
mod preferences;
mod profile;
mod shortcuts;
mod treehole;
mod window;

pub(crate) fn run() {
    let application = Application::builder().application_id(crate::APP_ID).build();
    shortcuts::install_quit(&application);
    application.connect_startup(|_| {
        install_styles();
        crate::service::appearance::restore();
        preferences::palette::restore();
    });
    application.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let store = SessionStore::default();
        match Session::restore(&store) {
            Ok(Some(session)) => window::build(app, session, store),
            Ok(None) => login::build(app, store, window::build),
            Err(error) => {
                eprintln!("无法恢复登录会话：{error}");
                login::build(app, store, window::build);
            }
        }
    });
    application.run();
}

fn install_styles() {
    if let Some(display) = gtk::gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(concat!(
            include_str!("../../resources/diary.css"),
            "\n",
            include_str!("../../resources/profile.css"),
            "\n",
            include_str!("../../resources/treehole.css"),
            "\n",
            include_str!("../../resources/about.css")
        ));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
