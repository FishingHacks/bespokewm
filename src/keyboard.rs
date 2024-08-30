use std::cell::{Cell, RefCell};

use xcb::{
    x::KeyPressEvent,
    xkb::{EventType, MapPart, SelectEvents, StateNotifyEvent, UseExtension},
    Connection,
};
use xkbcommon::xkb::{
    x11::{get_core_keyboard_device_id, keymap_new_from_device, state_new_from_device}, Context, Keycode, Keymap, Keysym, LayoutIndex, ModMask, State, CONTEXT_NO_FLAGS, KEYMAP_COMPILE_NO_FLAGS
};

use crate::events::Event;

pub const MODS_CTRL: u8 = 0x01 << 0;
pub const MODS_SHIFT: u8 = 0x01 << 1;
pub const MODS_META: u8 = 0x01 << 2;
pub const MODS_ALT: u8 = 0x01 << 3;
pub const MODS_SUPER: u8 = 0x01 << 4;
pub const MODS_MASK: u8 = MODS_CTRL | MODS_SHIFT | MODS_META | MODS_ALT | MODS_SUPER;

#[derive(Debug, Clone)]
pub struct KeyboardEvent {
        key: Keysym,
        characters: Box<str>,
        mods: u8,
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
        is_meta = MODS_META;
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

impl Keyboard {
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
        let code = Keycode::from(event.detail());
        let keysym = self.state.borrow().key_get_one_sym(code);

        let modmask = match keysym {
            Keysym::Control_L | Keysym::Control_R => MODS_CTRL,
            Keysym::Alt_L | Keysym::Alt_R => MODS_ALT,
            Keysym::Meta_L | Keysym::Meta_R => MODS_META,
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
                characters: self.state.borrow().key_get_utf8(code).into_boxed_str(),
                mods: self.mods.get(),
            })
        } else {
            Event::KeyRelease(KeyboardEvent {
                key: keysym,
                characters: Box::<str>::default(),
                mods: self.mods.get(),
            })
        }
    }
}
