pub mod homemaker;
pub mod homemaker_hotkeys;

pub use homemaker::{
    apply_homemaker_local_recommendations, get_homemaker_local_setup, HomemakerLocalSetup,
};
pub use homemaker_hotkeys::{homemaker_hotkey_presets, normalize_homemaker_hotkey};
