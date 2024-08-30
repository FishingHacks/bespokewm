use std::{cell::{Cell, RefCell}, collections::HashMap};

use tracing::{error, info};
use xcb::{
    x::{GrabKey, KeyPressEvent, ModMask as XModMask, UngrabKey, Window},
    xkb::{EventType, MapPart, SelectEvents, StateNotifyEvent, UseExtension},
    Connection,
};
use xkbcommon::xkb::{
    x11::{get_core_keyboard_device_id, keymap_new_from_device, state_new_from_device}, Context, Keycode, Keymap, Keysym, LayoutIndex, ModMask, State, CONTEXT_NO_FLAGS, KEYMAP_COMPILE_NO_FLAGS
};

use crate::{actions::Action, events::Event};

pub const MODS_CTRL: u8 = 0x01 << 0;
pub const MODS_SHIFT: u8 = 0x01 << 1;
pub const MODS_ALT: u8 = 0x01 << 2;
pub const MODS_SUPER: u8 = 0x01 << 3;
pub const MODS_MASK: u8 = MODS_CTRL | MODS_SHIFT | MODS_ALT | MODS_SUPER;

#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    pub key: Keysym,
    pub characters: Box<str>,
    pub mods: u8,
    pub keycode: Keycode,
}

macro_rules! is_mod {
    ($($name: ident = $mod: ident;)*) => {
        $(
            pub fn $name(&self) -> bool {
                (self.mods & $mod) > 0
            }
        )*
    };
}

impl KeyboardEvent {
    is_mod! {
        is_ctrl = MODS_CTRL;
        is_shift = MODS_SHIFT;
        is_alt = MODS_ALT;
        is_super = MODS_SUPER;
    }
}

pub struct Keyboard {
    _context: Context,
    _keymap: Keymap,
    device_id: i32,
    state: RefCell<State>,
    mods: Cell<u8>,
}

#[derive(Debug)]
pub struct BoundAction {
    pub key: Keycode,
    xmodifiers: XModMask,
    pub modifiers: u8,
    pub action_index: usize,
}

impl Keyboard {
    pub fn bind_actions(&self, actions: &[Action], conn: &Connection, root_window: Window) -> Vec<BoundAction> {
        let mut keycode_map = HashMap::<Keysym, Keycode>::new();
        
        let state = self.state.borrow();
        state.get_keymap().key_for_each(|_, keycode| {
            keycode_map.insert(state.key_get_one_sym(keycode), keycode);
        });

        let mut bound_actions = vec![];
        let mut cookies = vec![];

        for i in 0..actions.len() {
            if let Some(key) = keycode_map.get(&actions[i].key) {
                let mut modifiers = XModMask::empty();
                if actions[i].mods & MODS_CTRL > 0 {
                    modifiers |= XModMask::CONTROL;
                }
                if actions[i].mods & MODS_SHIFT > 0 {
                    modifiers |= XModMask::SHIFT;
                }
                if actions[i].mods & MODS_ALT > 0 {
                    modifiers |= XModMask::N1;
                }
                if actions[i].mods & MODS_SUPER > 0 {
                    modifiers |= XModMask::N4;
                }

                cookies.push(conn.send_request_checked(&GrabKey {
                    grab_window: root_window,
                    key: (*key).into(),
                    modifiers,
                    keyboard_mode: xcb::x::GrabMode::Async,
                    pointer_mode: xcb::x::GrabMode::Async,
                    owner_events: false,
                }));
                bound_actions.push(BoundAction { key: *key, xmodifiers: modifiers, action_index: i, modifiers: actions[i].mods });
            }
        }

        for (i, cookie) in cookies.into_iter().enumerate() {
            if let Err(e) = conn.check_request(cookie) {
                error!("Failed to bind action #{i} ({:?}):\n{e:?}", actions[i]);
            }
        }

        println!("Bound Actions");

        bound_actions
    }
    
    pub fn unbind_actions(&self, bound_actions: &[BoundAction], conn: &Connection, root_window: Window) {
        let cookies = bound_actions.iter().map(|bound_action| conn.send_request_checked(&UngrabKey {
            grab_window: root_window,
            key: bound_action.key.into(),
            modifiers: bound_action.xmodifiers,
        })).collect::<Vec<_>>();

        for cookie in cookies.into_iter() {
            if let Err(e) = conn.check_request(cookie) {
                error!("Failed to unbind action: {e:?}");
            }
        }

        println!("Unbound Actions");
    }

    pub fn new(conn: &Connection) -> anyhow::Result<Self> {
        let xkb_version = request_sync!(conn => UseExtension {
            wanted_major: xkbcommon::xkb::x11::MIN_MAJOR_XKB_VERSION,
            wanted_minor: xkbcommon::xkb::x11::MIN_MINOR_XKB_VERSION,
        });

        if !xkb_version.supported() {
            anyhow::bail!(
                "required xkb-xcb-{}-{}, but found xkb-xcb-{}-{}",
                xkbcommon::xkb::x11::MIN_MAJOR_XKB_VERSION,
                xkbcommon::xkb::x11::MIN_MINOR_XKB_VERSION,
                xkb_version.server_major(),
                xkb_version.server_minor(),
            );
        }

        let events =
            EventType::NEW_KEYBOARD_NOTIFY | EventType::MAP_NOTIFY | EventType::STATE_NOTIFY;
        let map_parts = MapPart::KEY_TYPES
            | MapPart::KEY_SYMS
            | MapPart::MODIFIER_MAP
            | MapPart::EXPLICIT_COMPONENTS
            | MapPart::KEY_ACTIONS
            | MapPart::KEY_BEHAVIORS
            | MapPart::VIRTUAL_MODS
            | MapPart::VIRTUAL_MOD_MAP;

        conn.send_and_check_request(&SelectEvents {
            device_spec: xcb::xkb::Id::UseCoreKbd as u32 as xcb::xkb::DeviceSpec,
            affect_map: map_parts,
            map: map_parts,
            select_all: events,
            affect_which: events,
            clear: EventType::empty(),
            details: &[],
        })?;

        let context = Context::new(CONTEXT_NO_FLAGS);
        let device_id = get_core_keyboard_device_id(conn);
        let keymap = keymap_new_from_device(&context, conn, device_id, KEYMAP_COMPILE_NO_FLAGS);
        let state = state_new_from_device(&keymap, conn, device_id);

        Ok(Keyboard {
            _context: context,
            _keymap: keymap,
            device_id,
            state: RefCell::new(state),
            mods: Cell::new(0),
        })
    }

    pub fn device_id(&self) -> i32 {
        self.device_id
    }

    pub fn update_state(&self, event: StateNotifyEvent) {
        self.state.borrow_mut().update_mask(
            event.base_mods().bits() as ModMask,
            event.latched_mods().bits() as ModMask,
            event.locked_mods().bits() as ModMask,
            event.base_group() as LayoutIndex,
            event.latched_group() as LayoutIndex,
            event.locked_group() as LayoutIndex,
        );
    }

    pub fn translate_event(&self, event: KeyPressEvent, press: bool) -> Event {
        let keycode = Keycode::from(event.detail());
        let keysym = self.state.borrow().key_get_one_sym(keycode);

        let modmask = match keysym {
            Keysym::Control_L | Keysym::Control_R => MODS_CTRL,
            Keysym::Alt_L | Keysym::Alt_R => MODS_ALT,
            Keysym::Shift_L | Keysym::Shift_R => MODS_SHIFT,
            Keysym::Super_L | Keysym::Super_R => MODS_SUPER,

            _ => 0u8,
        };

        if modmask != 0 {
            let mods = self.mods.get();
            self.mods.set(if press {
                mods | modmask
            } else {
                mods & !modmask
            });
        }

        if press {
            Event::KeyPress(KeyboardEvent {
                key: keysym,
                characters: self.state.borrow().key_get_utf8(keycode).into_boxed_str(),
                mods: self.mods.get(),
                keycode,
            })
        } else {
            Event::KeyRelease(KeyboardEvent {
                key: keysym,
                characters: Box::<str>::default(),
                mods: self.mods.get(),
                keycode,
            })
        }
    }
}
