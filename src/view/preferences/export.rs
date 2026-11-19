use crate::service::{auth::Session, diary};
use adw::{ApplicationWindow, MessageDialog, prelude::*};
use gtk::{FileChooserAction, FileChooserNative, FileFilter, ResponseType, gio, glib};
use std::sync::Arc;

pub(in crate::view) fn install(window: &ApplicationWindow, session: Arc<Session>) {
    let action = gio::SimpleAction::new("export-diaries", None);
    let parent = window.downgrade();
    action.connect_activate(move |action, _| {
        let Some(window) = parent.upgrade() else {
            return;
        };
        action.set_enabled(false);
        let action = action.clone();
        let session = session.clone();
        glib::spawn_future_local(async move {
            choose_and_export(&window, session).await;
            action.set_enabled(true);
        });
    });
    window.add_action(&action);
}

async fn choose_and_export(window: &ApplicationWindow, session: Arc<Session>) {
    let chooser = FileChooserNative::new(
        Some("导出我的日记"),
        Some(window),
        FileChooserAction::Save,
        Some("导出"),
        Some("取消"),
    );
    chooser.set_modal(true);
    chooser.set_current_name(&format!("我的日记-{}.zip", diary::today()));
    let filter = FileFilter::new();
    filter.set_name(Some("ZIP 压缩包 (*.zip)"));
    filter.add_pattern("*.zip");
    chooser.add_filter(&filter);
    let response = chooser.run_future().await;
    let file = chooser.file();
    chooser.destroy();
    let Some(file) = file.filter(|_| response == ResponseType::Accept && window.get_visible())
    else {
        return;
    };

    let status = MessageDialog::new(
        Some(window),
        Some("正在导出日记"),
        Some("正在整理本人的完整正文…"),
    );
    status.set_destroy_with_parent(true);
    status.add_response("close", "关闭");
    status.present();
    let result = gio::spawn_blocking(move || diary::export_own(&session))
        .await
        .unwrap_or(Err("读取日记失败，请重试。"));
    if !window.get_visible() {
        return;
    }
    let result = match result {
        Ok(export) => {
            let flags = gio::FileCreateFlags::PRIVATE | gio::FileCreateFlags::REPLACE_DESTINATION;
            file.replace_contents_future(export.zip, None, false, flags)
                .await
                .map(|_| {
                    let mut message = format!("已保存 {} 篇日记。", export.count);
                    if export.unverified > 0 {
                        message.push_str(&format!(
                            "另有 {} 篇缺少作者信息，已跳过。",
                            export.unverified
                        ));
                    }
                    message
                })
                .map_err(|_| "写入文件失败，请检查可用空间和保存位置。")
        }
        Err(msg) => Err(msg),
    };
    match result {
        Ok(message) => {
            status.set_heading(Some("导出完成"));
            status.set_body(&message);
        }
        Err(message) => {
            status.set_heading(Some("导出失败"));
            status.set_body(message);
        }
    }
}
