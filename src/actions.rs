use xkbcommon::xkb::Keysym;

use crate::{keyboard::{MODS_ALT, MODS_CTRL, MODS_SHIFT}, layout::Layout};

#[derive(Debug, Clone)]
pub enum ActionType {
    Quit,
    CycleLayout,
    CloseFocusedWindow,
    SwitchToLayout(Layout),
    Launch(&'static str),
}

#[derive(Debug, Clone)]
pub struct Action {
    pub key: Keysym,
    pub mods: u8,
    pub action: ActionType,
}

impl Action {
    pub const fn new(key: Keysym, mods: u8, action: ActionType) -> Self {
        Self { key, mods, action }
    }
}

pub static ACTIONS: &[Action] = &[
    Action::new(Keysym::q, MODS_CTRL | MODS_ALT, ActionType::Quit),
    Action::new(Keysym::q, MODS_SHIFT | MODS_ALT, ActionType::CloseFocusedWindow),
    Action::new(Keysym::l, MODS_ALT, ActionType::CycleLayout),
    Action::new(Keysym::p, MODS_ALT, ActionType::Launch("/usr/local/bin/dmenu_run")),
    Action::new(Keysym::Return, MODS_ALT, ActionType::Launch("/usr/local/bin/alacritty")),
];