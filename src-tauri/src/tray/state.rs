use tauri::{image::Image, menu::MenuItem, AppHandle};

pub struct TrayState {
    pub icons: TrayIconSet,
    settings_item: MenuItem<tauri::Wry>,
    quit_item: MenuItem<tauri::Wry>,
}

pub struct TrayIconSet {
    pub initializing: Image<'static>,
    pub idle: Image<'static>,
    pub ready: Image<'static>,
    pub listening: Image<'static>,
    pub processing: Image<'static>,
    pub error: Image<'static>,
}

impl TrayIconSet {
    pub fn from_app(app: &AppHandle) -> Self {
        let base = owned_icon(
            app.default_window_icon()
                .expect("default window icon"),
        );
        Self {
            initializing: recolor_icon(&base, [110, 110, 110]),
            idle: recolor_icon(&base, [130, 130, 130]),
            ready: recolor_icon(&base, [90, 210, 90]),
            listening: recolor_icon(&base, [70, 170, 255]),
            processing: recolor_icon(&base, [240, 75, 75]),
            error: recolor_icon(&base, [240, 170, 60]),
        }
    }
}

fn owned_icon(source: &Image<'_>) -> Image<'static> {
    Image::new_owned(
        source.rgba().to_vec(),
        source.width(),
        source.height(),
    )
}

impl TrayState {
    pub fn new(
        app: &AppHandle,
        settings_item: MenuItem<tauri::Wry>,
        quit_item: MenuItem<tauri::Wry>,
    ) -> Self {
        Self {
            icons: TrayIconSet::from_app(app),
            settings_item,
            quit_item,
        }
    }

    pub fn settings_item(&self) -> &MenuItem<tauri::Wry> {
        &self.settings_item
    }

    pub fn quit_item(&self) -> &MenuItem<tauri::Wry> {
        &self.quit_item
    }
}

/// Recolor non-transparent pixels toward a solid accent (visible in 16×16 tray).
fn recolor_icon(source: &Image, color: [u8; 3]) -> Image<'static> {
    let mut rgba = source.rgba().to_vec();
    for chunk in rgba.chunks_mut(4) {
        if chunk[3] < 20 {
            continue;
        }
        let lum = (0.299 * chunk[0] as f32
            + 0.587 * chunk[1] as f32
            + 0.114 * chunk[2] as f32)
            / 255.0;
        let intensity = lum.clamp(0.35, 1.0);
        chunk[0] = (color[0] as f32 * intensity) as u8;
        chunk[1] = (color[1] as f32 * intensity) as u8;
        chunk[2] = (color[2] as f32 * intensity) as u8;
    }
    Image::new_owned(rgba, source.width(), source.height())
}
