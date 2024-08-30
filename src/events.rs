use xcb::x::Window;

use crate::keyboard::KeyboardEvent;

#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
    Left = 1,
    Middle = 2,
    Right = 3,
}
impl TryFrom<u8> for MouseButton {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Left),
            2 => Ok(Self::Middle),
            3 => Ok(Self::Right),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    KeyPress(KeyboardEvent),
    KeyRelease(KeyboardEvent),
    MouseScroll(i32),
    ButtonPress(MouseButton),
    ButtonRelease(MouseButton),
    MouseMove {
        window_x: i16,
        window_y: i16,
        absolute_x: i16,
        absolute_y: i16,
    },

    MapRequest(Window),
    EnterNotify(Window),
    UnmapNotify(Window),
    DestroyNotify(Window),
}
