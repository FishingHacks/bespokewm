use std::sync::Arc;

use anyhow::{Context, Result};
use xcb::{
    x::{
        ChangeWindowAttributes, CreateGlyphCursor, Cw, Drawable, EventMask, GetGeometry,
        OpenFont, Window, Event as XEvent,
    },
    Event as XcbEvent,
    Connection,
};

use crate::{
    atoms::Atoms,
    config,
    events::{Event, MouseButton},
    keyboard::Keyboard,
    layout::{AbstractWindow, Layout, Tiler},
};

pub struct Wm {
    conn: Arc<Connection>,
    workspaces: [Tiler<Window>; 10],
    atoms: Atoms,
    keyboard: Keyboard,
}

impl Wm {
    pub fn new() -> Result<Self> {
        let (conn, _) = xcb::Connection::connect(None)
            .context("Failed to connect to the X Server. Is $DISPLAY correct?")?;
        let conn = Arc::new(conn);

        let root = Self::setup(&conn)?;
        let atoms = Atoms::get(&conn);

        let root_dimensions = request_sync!(conn => GetGeometry { drawable: Drawable::Window(root) }; "failed to get the initial window size");

        println!(
            "Root Window: {}x{}",
            root_dimensions.width(),
            root_dimensions.height()
        );
        println!(
            "Root Border Width: {} | Depth: {}",
            root_dimensions.border_width(),
            root_dimensions.depth()
        );
        assert_eq!(root_dimensions.x(), 0, "x of rootwindow != 0");
        assert_eq!(root_dimensions.y(), 0, "y of rootwindow != 0");

        let keyboard = Keyboard::new(&conn).context("Failed to initialise the keyboard")?;

        let mut workspaces = Self::make_workspaces(
            root_dimensions.width(),
            root_dimensions.height(),
            config::GAP_SIZE,
        );
        workspaces[0].show(&conn);

        Ok(Self {
            conn,
            workspaces,
            atoms,
            keyboard,
        })
    }

    fn setup(conn: &Connection) -> Result<Window> {
        let setup = conn.get_setup();
        let screen = setup.roots().next().context("Failed to get a screen")?;
        let window = screen.root();

        let font = conn.generate_id();
        conn.send_and_check_request(&OpenFont {
            fid: font,
            name: b"cursor",
        })
        .context("Failed to get the cursor font")?;

        let cursor = conn.generate_id();
        conn.send_and_check_request(&CreateGlyphCursor {
            cid: cursor,
            mask_font: font,
            source_font: font,
            source_char: 68,
            mask_char: 69,
            fore_red: 0,
            fore_green: 0,
            fore_blue: 0,
            back_red: 0xffff,
            back_green: 0xffff,
            back_blue: 0xffff,
        })
        .context("Failed to a new create cursor")?;

        conn.send_and_check_request(&ChangeWindowAttributes {
            window,
            value_list: &[
                Cw::EventMask(
                    EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::SUBSTRUCTURE_REDIRECT
                        | EventMask::ENTER_WINDOW
                        | EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::BUTTON_MOTION,
                ),
                Cw::Cursor(cursor),
            ],
        })
        .context("Failed to acquire root window")?;

        Ok(window)
    }

    fn make_workspaces(width: u16, height: u16, gaps: u16) -> [Tiler<Window>; 10] {
        [
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
            Tiler::new(width, height, gaps),
        ]
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let ev = self
                .conn
                .wait_for_event()
                .context("Ran into an error while trying to fetch the next event")?;
            let Some(ev) = self.translate_event(ev) else { continue; };
            println!("{ev:?}");
        }
        Ok(())
    }

    fn translate_event(&self, event: xcb::Event) -> Option<Event> {
        match event {
            XcbEvent::X(XEvent::KeyPress(event)) => Some(self.keyboard.translate_event(event, true)),
            XcbEvent::X(XEvent::KeyRelease(event)) => Some(self.keyboard.translate_event(event, false)),
            XcbEvent::X(XEvent::ButtonPress(btn)) if btn.detail() == 4 => Some(Event::MouseScroll(-1)),
            XcbEvent::X(XEvent::ButtonPress(btn)) if btn.detail() == 5 => Some(Event::MouseScroll(1)),
            XcbEvent::X(XEvent::ButtonRelease(btn)) if btn.detail() == 4 || btn.detail() == 5 => None,
            XcbEvent::X(XEvent::ButtonPress(btn)) => MouseButton::try_from(btn.detail()).ok().map(Event::ButtonPress),
            XcbEvent::X(XEvent::ButtonRelease(btn)) => MouseButton::try_from(btn.detail()).ok().map(Event::ButtonRelease),
            
            XcbEvent::X(XEvent::MotionNotify(ev)) => Some(Event::MouseMove {
                absolute_x: ev.root_x(),
                absolute_y: ev.root_y(),
                window_x: ev.event_x(),
                window_y: ev.event_y(),
            }),

            XcbEvent::X(XEvent::MapRequest(ev)) => Some(Event::MapRequest(ev.window())),
            XcbEvent::X(XEvent::EnterNotify(ev)) => Some(Event::EnterNotify(ev.event())),
            XcbEvent::X(XEvent::UnmapNotify(ev)) => Some(Event::EnterNotify(ev.window())),
            XcbEvent::X(XEvent::DestroyNotify(ev)) => Some(Event::DestroyNotify(ev.event())),

            XcbEvent::Xkb(xcb::xkb::Event::StateNotify(xkb_ev))
                if xkb_ev.device_id() as i32 == self.keyboard.device_id() =>
            {
                self.keyboard.update_state(xkb_ev);
                None
            }
            _ => {
                println!("Unhandled Event: {event:?}");
                None
            }
        }
    }
}

impl AbstractWindow for Window {
    fn update(&mut self, width: u16, height: u16, x: u16, y: u16, conn: &Connection) {
        todo!()
    }

    fn hide(&mut self, conn: &Connection) {
        todo!()
    }

    fn show(&mut self, conn: &Connection) {
        todo!()
    }
}
