use std::{
    sync::mpsc::{TryRecvError, channel},
    thread::spawn,
    time::Duration,
};

use adw::{
    Application, ApplicationWindow, HeaderBar,
    prelude::{AdwApplicationWindowExt, EntryRowExt, PreferencesGroupExt},
};
use gtk::{
    Box, Button, Label,
    Orientation::Vertical,
    glib::{ControlFlow::Break, ControlFlow::Continue, clone::Downgrade, timeout_add_local},
    prelude::{BoxExt, ButtonExt, EditableExt, GtkWindowExt, WidgetExt},
};

use crate::service::auth::{LoginError, LoginRequest, Session, SessionStore, login_and_save};

/// 创建登录窗口，在后台请求登录并把结果交回界面线程。
pub(super) fn build(
    app: &Application,
    store: SessionStore,
    on_success: fn(&Application, Session, SessionStore),
) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("登录")
        .default_height(300)
        .default_width(400)
        .build();
    let header_bar = HeaderBar::new();
    let content = Box::new(Vertical, 12);
    let email_entry = adw::EntryRow::builder()
        .title("邮箱")
        .input_purpose(gtk::InputPurpose::Email)
        .build();
    let password_entry = adw::PasswordEntryRow::builder()
        .title("密码")
        .activates_default(true)
        .build();
    let login_button = Button::with_label("登录");
    login_button.add_css_class("suggested-action");
    window.set_default_widget(Some(&login_button));
    let password = password_entry.clone();
    email_entry.connect_entry_activated(move |_| {
        password.grab_focus();
    });
    let form = Box::new(Vertical, 12);
    let message_label = Label::new(None);
    message_label.set_wrap(true);
    message_label.set_visible(false);
    message_label.add_css_class("error");
    form.set_margin_start(24);
    form.set_margin_end(24);
    form.set_margin_top(12);
    form.set_margin_bottom(24);
    let email_for_login = email_entry.clone();
    let password_for_login = password_entry.clone();
    let message_for_login = message_label.clone();
    let app_weak = app.downgrade();
    let window_weak = window.downgrade();
    login_button.connect_clicked(move |button| {
        let email = email_for_login.text();
        let password = password_for_login.text();
        let Some(credentials) = LoginRequest::new(email.trim().to_owned(), password.to_string())
        else {
            message_for_login.set_text("请输入邮箱和密码");
            message_for_login.set_visible(true);
            if email.trim().is_empty() {
                email_for_login.grab_focus();
            } else {
                password_for_login.grab_focus();
            }
            return;
        };
        let (sender, receiver) = channel();
        button.set_sensitive(false);
        button.set_label("正在登录…");
        message_for_login.set_visible(false);
        let button_weak = button.downgrade();
        let message_weak = message_for_login.downgrade();
        let store_for_login = store.clone();
        spawn(move || {
            let result = login_and_save(credentials, &store_for_login);
            let _ = sender.send(result);
        });
        let app_for_result = app_weak.clone();
        let window_for_result = window_weak.clone();
        let store_for_result = store.clone();
        timeout_add_local(Duration::from_millis(100), move || {
            let Some(button) = button_weak.upgrade() else {
                return Break;
            };
            let Some(message) = message_weak.upgrade() else {
                return Break;
            };
            let result_text = match receiver.try_recv() {
                Ok(Ok(session)) => {
                    if let Some(app) = app_for_result.upgrade() {
                        on_success(&app, session, store_for_result.clone());
                        if let Some(window) = window_for_result.upgrade() {
                            window.close();
                        }
                    }
                    "登录成功".into()
                }
                Ok(Err(LoginError::Rejected(code))) => format!("登录未成功，业务状态{code}"),
                Ok(Err(LoginError::MissingToken)) => "登录响应缺少有效凭据".into(),
                Ok(Err(LoginError::RequestFailed)) => "登录请求失败，请重试".into(),
                Err(TryRecvError::Empty) => return Continue,
                Err(TryRecvError::Disconnected) => "登录请求未完成".into(),
            };
            button.set_sensitive(true);
            button.set_label("登录");
            message.set_text(&result_text);
            message.set_visible(true);
            Break
        });
    });
    content.append(&header_bar);
    let fields = adw::PreferencesGroup::new();
    fields.add(&email_entry);
    fields.add(&password_entry);
    form.append(&fields);
    form.append(&message_label);
    form.append(&login_button);
    content.append(&form);
    window.set_content(Some(&content));
    window.present();
}
