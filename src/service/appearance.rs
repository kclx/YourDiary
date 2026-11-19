use crate::repo::preferences;
use adw::{ColorScheme, StyleManager};

const THEMES: [(&str, &str, ColorScheme); 3] = [
    ("system", "跟随系统", ColorScheme::Default),
    ("light", "浅色", ColorScheme::ForceLight),
    ("dark", "深色", ColorScheme::ForceDark),
];

pub(crate) fn restore() {
    match preferences::read("appearance") {
        Ok(value) => {
            let scheme = THEMES
                .iter()
                .find(|(key, _, _)| Some(*key) == value.as_deref())
                .map_or(ColorScheme::Default, |theme| theme.2);
            StyleManager::default().set_color_scheme(scheme);
        }
        Err(error) => eprintln!("无法读取外观设置：{error}"),
    }
    match saved_font() {
        Ok(Some(font)) => apply_font(Some(&font)),
        Ok(None) => {}
        Err(error) => eprintln!("无法读取字体设置：{error}"),
    }
}

pub(crate) fn saved_font() -> Result<Option<String>, Box<dyn std::error::Error>> {
    Ok(preferences::read("font")?.filter(|value| !value.is_empty()))
}

pub(crate) fn select_font(font: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    preferences::save("font", font.unwrap_or_default())?;
    apply_font(font);
    Ok(())
}

fn apply_font(font: Option<&str>) {
    if let Some(settings) = gtk::Settings::default() {
        match font {
            Some(name) => settings.set_gtk_font_name(Some(name)),
            None => settings.reset_property("gtk-font-name"),
        }
    }
}

pub(crate) fn labels() -> [&'static str; 3] {
    THEMES.map(|theme| theme.1)
}

pub(crate) fn selected() -> u32 {
    let current = StyleManager::default().color_scheme();
    THEMES
        .iter()
        .position(|theme| theme.2 == current)
        .unwrap_or(0) as u32
}

pub(crate) fn select(index: u32) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(&(key, _, scheme)) = THEMES.get(index as usize) {
        let manager = StyleManager::default();
        if manager.color_scheme() != scheme {
            preferences::save("appearance", key)?;
            manager.set_color_scheme(scheme);
        }
    }
    Ok(())
}

pub(crate) fn saved_palette() -> Result<Option<String>, Box<dyn std::error::Error>> {
    preferences::read("palette")
}

pub(crate) fn save_palette(key: &str) -> Result<(), Box<dyn std::error::Error>> {
    preferences::save("palette", key)
}
