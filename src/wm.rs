use std::{process::Command, sync::Arc};

use anyhow::{Context, Result};
use tracing::error;
use xcb::{
    x::{
        ChangeWindowAttributes, CreateGlyphCursor, Cw, Drawable, Event as XEvent, EventMask,
        GetGeometry, OpenFont, Window,
    },
    Connection, Event as XcbEvent,
};

use crate::{
    actions::{Action, ActionType},
    atoms::Atoms,
    events::{Event, MouseButton},
    keyboard::Keyboard,
    screen::Screen,
};

pub struct Wm {
    conn: Arc<Connection>,
    screen: Screen,
    atoms: Atoms,
    keyboard: Keyboard,
    root: Window,
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

        let screen = Screen::new(
            root_dimensions.width(),
            root_dimensions.height(),
            0,
            atoms,
            root,
            conn.clone(),
        )
        .context("Failed to initialise the screen")?;

        Ok(Self {
            conn,
            screen,
            atoms,
            keyboard,
            root,
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

    pub fn run(&mut self, actions: &[Action]) -> anyhow::Result<()> {
        let bound_actions = self.keyboard.bind_actions(actions, &self.conn, self.root);
        println!("{bound_actions:?}");
        let mut procs = vec![];

        'mainloop: loop {
            let ev = self
                .conn
                .wait_for_event()
                .context("Ran into an error while trying to fetch the next event")?;
            let Some(ev) = self.translate_event(ev) else {
                continue;
            };

            match ev {
                Event::KeyPress(ev) => {
                    println!("{ev:?}");
                    for action in bound_actions.iter() {
                        if action.key == ev.keycode && action.modifiers == ev.mods {
                            match actions[action.action_index].action {
                                ActionType::Quit => break 'mainloop,
                                ActionType::CycleLayout => self.screen.cycle_layout(),
                                ActionType::CloseFocusedWindow => self.screen.close_focused_window(),
                                ActionType::SwitchToLayout(new_layout) => {
                                    self.screen.set_layout(new_layout)
                                }
                                ActionType::Launch(cmd) => {
                                    let mut command = Command::new(cmd);
                                    if let Some(display) = std::env::var_os("DISPLAY")
                                        .and_then(|str| str.into_string().ok())
                                    {
                                        command.env("DISPLAY", display);
                                    }
                                    match command.spawn() {
                                        Err(e) => {
                                            error!("Failed to run Action: Failed to run Command: {e:?}")
                                        }
                                        Ok(child) => procs.push(child),
                                    }
                                }
                            }
                        }
                    }
                }
                Event::MapRequest(window) => trace_result!(self.screen.add_window(window)),
                Event::DestroyNotify(window) => self.screen.remove_window(window),
                Event::EnterNotify(window) => self.screen.enter_client(window),
                _ => {}
            }

            // clean up child processes
            let len = procs.len();
            for i in 0..len {
                let i = len - 1 - i;
                // the process exited
                if !matches!(procs[i].try_wait(), Ok(None)) {
                    procs.remove(i);
                }
            }
        }

        self.keyboard
            .unbind_actions(&bound_actions, &self.conn, self.root);
        self.screen.kill_children();
        for proc in procs.iter_mut() {
            _ = proc.kill();
        }
        procs.clear();
        Ok(())
    }

    fn translate_event(&self, event: xcb::Event) -> Option<Event> {
        match event {
            XcbEvent::X(XEvent::KeyPress(event)) => {
                Some(self.keyboard.translate_event(event, true))
            }
            XcbEvent::X(XEvent::KeyRelease(event)) => {
                Some(self.keyboard.translate_event(event, false))
            }
            XcbEvent::X(XEvent::ButtonPress(btn)) if btn.detail() == 4 => {
                Some(Event::MouseScroll(-1))
            }
            XcbEvent::X(XEvent::ButtonPress(btn)) if btn.detail() == 5 => {
                Some(Event::MouseScroll(1))
            }
            XcbEvent::X(XEvent::ButtonRelease(btn)) if btn.detail() == 4 || btn.detail() == 5 => {
                None
            }
            XcbEvent::X(XEvent::ButtonPress(btn)) => MouseButton::try_from(btn.detail())
                .ok()
                .map(Event::ButtonPress),
            XcbEvent::X(XEvent::ButtonRelease(btn)) => MouseButton::try_from(btn.detail())
                .ok()
                .map(Event::ButtonRelease),

            XcbEvent::X(XEvent::MotionNotify(ev)) => Some(Event::MouseMove {
                absolute_x: ev.root_x(),
                absolute_y: ev.root_y(),
                window_x: ev.event_x(),
                window_y: ev.event_y(),
            }),

            XcbEvent::X(XEvent::EnterNotify(ev)) => Some(Event::EnterNotify(ev.event())),
            XcbEvent::X(XEvent::MapRequest(ev)) => Some(Event::MapRequest(ev.window())),
            XcbEvent::X(XEvent::DestroyNotify(ev)) => Some(Event::DestroyNotify(ev.window())),
            XcbEvent::X(XEvent::ReparentNotify(_)) => None,

            XcbEvent::Xkb(xcb::xkb::Event::StateNotify(xkb_ev))
                if xkb_ev.device_id() as i32 == self.keyboard.device_id() =>
            {
                self.keyboard.update_state(xkb_ev);
                None
            }
            _ => None,
        }
    }
}
