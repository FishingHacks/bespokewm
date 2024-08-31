use std::{cell::RefCell, collections::HashMap};

use tracing::error;
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
    pub mods: XModMask,
    pub keycode: Keycode,
}

macro_rules! is_mod {
    ($($name: ident = $mod: expr;)*) => {
        $(
            pub fn $name(&self) -> bool {
                self.mods.contains($mod)
            }
        )*
    };
}

impl KeyboardEvent {
    is_mod! {
        is_shift = XModMask::SHIFT;
        is_caps_lock = XModMask::LOCK;
        is_ctrl = XModMask::CONTROL;
        is_alt = XModMask::N1;
        is_num_lock = XModMask::N2;
        is_scroll_locl = XModMask::N3;
        is_super = XModMask::N4;
    }
}

pub struct Keyboard {
    _context: Context,
    _keymap: Keymap,
    device_id: i32,
    state: RefCell<State>,
}

#[derive(Debug)]
pub struct BoundAction {
    pub key: Keycode,
    pub modifiers: XModMask,
    pub action_index: usize,
}

impl Keyboard {
    pub fn bind_actions(
        &self,
        actions: &[Action],
        conn: &Connection,
        root_window: Window,
    ) -> Vec<BoundAction> {
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
                bound_actions.push(BoundAction {
                    key: *key,
                    modifiers,
                    action_index: i,
                });
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

    pub fn unbind_actions(
        &self,
        bound_actions: &[BoundAction],
        conn: &Connection,
        root_window: Window,
    ) {
        let cookies = bound_actions
            .iter()
            .map(|bound_action| {
                conn.send_request_checked(&UngrabKey {
                    grab_window: root_window,
                    key: bound_action.key.into(),
                    modifiers: bound_action.modifiers,
                })
            })
            .collect::<Vec<_>>();

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
        let state = self.state.borrow();
        let keysym = state.key_get_one_sym(keycode);
        let mods = XModMask::from_bits_truncate(event.state().bits());

        if press {
            Event::KeyPress(KeyboardEvent {
                key: keysym,
                characters: state.key_get_utf8(keycode).into_boxed_str(),
                mods,
                keycode,
            })
        } else {
            Event::KeyRelease(KeyboardEvent {
                key: keysym,
                characters: Box::<str>::default(),
                mods,
                keycode,
            })
        }
    }
}
