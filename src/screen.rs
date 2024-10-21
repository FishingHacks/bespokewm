use core::str;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

const WINDOW_BAR_HEIGHT: u16 = 20;

use anyhow::{Context as _, Result};
use tracing::{error, warn};
use xcb::{
    x::{
        ChangeWindowAttributes, ConfigWindow, ConfigureWindow, CreateWindow, Cw, DestroyWindow,
        EventMask, GetProperty, GetPropertyReply, MapWindow, ReparentWindow, SetInputFocus,
        UnmapWindow, Window as XWindow, ATOM_ANY, ATOM_CARDINAL, COPY_FROM_PARENT, CURRENT_TIME,
    },
    Connection, Xid,
};

use crate::{
    atoms::Atoms,
    config, ewmh,
    layout::{Position, Workspace},
    slab::Slab,
    tiling::Layout,
};

pub struct Context {
    pub(crate) window_lookup: HashMap<XWindow, usize>,
    pub(crate) windows: Slab<Client>,
    pub(crate) current_workspace: u8,
    pub(crate) atoms: Atoms,
    pub(crate) root_window: XWindow,
    pub(crate) connection: Arc<Connection>,
    pub(crate) focused_window: Option<usize>,
}

pub struct Screen {
    width: u16,
    height: u16,
    reserved_space_bottom: u16,
    reserved_space_top: u16,
    reserved_space_left: u16,
    reserved_space_right: u16,
    workspaces: [Workspace; 10],
    context: Context,

    global_windows: Slab<ReservedClient>,
}

impl Screen {
    pub fn new(
        width: u16,
        height: u16,
        gap: u16,
        atoms: Atoms,
        root_window: XWindow,
        connection: Arc<Connection>,
        depth: u8,
    ) -> anyhow::Result<Self, xcb::ProtocolError> {
        // let mut draw = DrawContext::new(root_window, Position::new(0, 0, width, 25), connection.clone(), depth)?;
        // draw.open_font("fixed")?;

        let mut me = Self {
            width,
            height,
            reserved_space_bottom: 0,
            reserved_space_left: 0,
            reserved_space_right: 0,
            reserved_space_top: 0,
            // draw,
            workspaces: [
                Workspace::new(Position::new(0, 25, width, height), gap, 1),
                Workspace::new(Position::new(0, 25, width, height), gap, 2),
                Workspace::new(Position::new(0, 25, width, height), gap, 3),
                Workspace::new(Position::new(0, 25, width, height), gap, 4),
                Workspace::new(Position::new(0, 25, width, height), gap, 5),
                Workspace::new(Position::new(0, 25, width, height), gap, 6),
                Workspace::new(Position::new(0, 25, width, height), gap, 7),
                Workspace::new(Position::new(0, 25, width, height), gap, 8),
                Workspace::new(Position::new(0, 25, width, height), gap, 9),
                Workspace::new(Position::new(0, 25, width, height), gap, 10),
            ],
            global_windows: Slab::new(),
            context: Context {
                connection,
                windows: Slab::new(),
                window_lookup: HashMap::new(),
                atoms,
                root_window,
                focused_window: None,
                current_workspace: 0,
            },
        };
        ewmh::set_number_of_desktops(10, root_window, &atoms, &me.context.connection)?;
        me.switch_workspace(1)?;

        me.size_updated();
        _ = me.update_atoms();
        Ok(me)
    }

    pub fn update_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.size_updated();
    }

    fn size_updated(&mut self) {
        if self.reserved_space_bottom + self.reserved_space_top >= self.height {
            warn!("The window is smaller than the reserved space (top: {}, bottom: {}, total: {}, window height: {})\nUnreserving Space",
                self.reserved_space_top, self.reserved_space_bottom, self.reserved_space_top + self.reserved_space_bottom, self.height);
            self.reserved_space_top = 0;
            self.reserved_space_bottom = 0;
        }
        if self.reserved_space_left + self.reserved_space_right >= self.width {
            warn!("The window is smaller than the reserved space (left: {}, right: {}, total: {}, window width: {})\nUnreserving Space",
                self.reserved_space_left, self.reserved_space_right, self.reserved_space_left+self.reserved_space_right, self.width);

            self.reserved_space_left = 0;
            self.reserved_space_right = 0;
        }

        for workspace in self.workspaces.iter_mut() {
            workspace.set_screen_position(
                Position::new(
                    self.reserved_space_left,
                    self.reserved_space_top,
                    self.width - self.reserved_space_left - self.reserved_space_right,
                    self.height - self.reserved_space_top - self.reserved_space_bottom,
                ),
                &mut self.context,
            );
        }
        _ = self.update_atoms();
    }

    pub fn add_reserved_client(&mut self, client: ReservedClient) -> anyhow::Result<()> {
        if self.global_windows.len() > u8::MAX as usize {
            error!("Tried to register >255 global clients!");
            anyhow::bail!("Not supporting >255 global clients!");
        }
        let map_cookie = self.context.connection.send_request_checked(&MapWindow {
            window: client.window,
        });
        let change_attributes_cookie =
            self.context
                .connection
                .send_request_checked(&ChangeWindowAttributes {
                    window: client.window,
                    value_list: &[Cw::EventMask(EventMask::ENTER_WINDOW)],
                });

        self.context.connection.check_request(map_cookie)?;
        self.context
            .connection
            .check_request(change_attributes_cookie)?;
        self.global_windows.push(client);
        self.update_atoms()?;
        Ok(())
    }

    pub fn switch_workspace(&mut self, new_workspace: u8) -> Result<(), xcb::ProtocolError> {
        let old_workspace = self.context.current_workspace;
        self.context.current_workspace = new_workspace;
        self.update_atoms()?;
        self.workspaces[old_workspace as usize].hide(&mut self.context);
        self.workspaces[new_workspace as usize].show(&mut self.context);
        Ok(())
    }

    pub fn update_atoms(&self) -> Result<(), xcb::ProtocolError> {
        let atoms = &self.context.atoms;
        let conn = &self.context.connection;

        ewmh::set_desktop_viewport(
            self.reserved_space_left as u32,
            self.reserved_space_top as u32,
            self.context.root_window,
            atoms,
            conn,
        )?;
        ewmh::set_number_of_desktops(
            self.workspaces.len() as u32,
            self.context.root_window,
            atoms,
            conn,
        )?;
        ewmh::set_current_desktop(
            self.context.current_workspace as u32,
            self.context.root_window,
            atoms,
            conn,
        )?;
        ewmh::set_desktop_names(&self.workspaces, self.context.root_window, atoms, conn)?;
        ewmh::set_wm_desktop(&self.workspaces, &self.context)?;

        let current_workspace = &self.workspaces[self.context.current_workspace as usize];
        ewmh::set_client_list(
            &self
                .context
                .window_lookup
                .values()
                .map(|v| self.context.windows[*v].window)
                .collect::<Vec<_>>(),
            self.context.root_window,
            atoms,
            conn,
        )?;
        let mut windows =
            Vec::with_capacity(self.global_windows.max_len() + current_workspace.window_amount());
        windows.extend(self.global_windows.iter().map(|v| v.window));
        windows.extend(
            current_workspace
                .windows()
                .map(|v| self.context.windows[v].window),
        );
        ewmh::set_client_list_stacking(&windows, self.context.root_window, atoms, conn)?;
        ewmh::set_showing_desktop(false, self.context.root_window, atoms, conn)?;

        Ok(())
    }

    pub fn enter_client(&mut self, client: XWindow) {
        for workspace in self.workspaces.iter_mut() {
            workspace.unfocus_all(&mut self.context);
        }
        self.context.focused_window = None;

        if client == self.context.root_window {
            trace_result!(self.context.connection.send_and_check_request(&SetInputFocus {
                time: CURRENT_TIME,
                focus: self.context.root_window,
                revert_to: xcb::x::InputFocus::Parent
            }); "failed to give root focus");

            return;
        }

        if let Some(idx) = self.context.window_lookup.get(&client).copied() {
            if self.workspaces[self.context.current_workspace as usize]
                .focus_client(idx, &mut self.context)
            {
                self.context.focused_window = Some(idx);
                return;
            }
        }
        for reserved_client in self.global_windows.iter() {
            if reserved_client.window == client {
                _ = self
                    .context
                    .connection
                    .send_and_check_request(&SetInputFocus {
                        time: CURRENT_TIME,
                        focus: reserved_client.window,
                        revert_to: xcb::x::InputFocus::Parent,
                    });
                break;
            }
        }
    }

    fn free_reserved_space(&mut self, amount: u16, direction: ScreenSide) {
        match direction {
            ScreenSide::Bottom => self.free_space_bottom(amount),
            ScreenSide::Left => self.free_space_left(amount),
            ScreenSide::Right => self.free_space_right(amount),
            ScreenSide::Top => self.free_space_top(amount),
        }
    }

    pub fn remove_window(&mut self, window: XWindow) {
        if let Some(window_idx) = self.context.window_lookup.get(&window).copied() {
            for ws in self.workspaces.iter_mut() {
                ws.remove_window(window_idx, &mut self.context);
            }
            self.context.windows[window_idx].destroy(&self.context.connection);

            self.context.windows.remove(window_idx);
            let mut to_remove = vec![];
            for (k, v) in self.context.window_lookup.iter() {
                if *v == window_idx {
                    to_remove.push(*k);
                }
            }
            for k in to_remove {
                self.context.window_lookup.remove(&k);
            }
        };

        for i in 0..self.global_windows.max_len() {
            let Some(global_window) = self.global_windows.get(i) else {
                continue;
            };
            if global_window.window == window {
                let child = self
                    .global_windows
                    .remove(i)
                    .expect("we should have a child");
                self.free_reserved_space(child.reserved, child.direction);
                _ = self
                    .context
                    .connection
                    .send_and_check_request(&UnmapWindow {
                        window: child.window,
                    });
                _ = self
                    .context
                    .connection
                    .send_and_check_request(&DestroyWindow {
                        window: child.window,
                    });
            }
        }

        trace_result!(self.context.connection.flush(); "failed to flush the connection after window remove");
    }

    fn handle_reserved_client(&mut self, window: XWindow, values: [u32; 12]) -> anyhow::Result<()> {
        // _NET_WM_STRUT: https://specifications.freedesktop.org/wm-spec/latest/ar01s05.html#id-1.6.10
        // _NET_WM_STRUT_PARTIAL: https://specifications.freedesktop.org/wm-spec/latest/ar01s05.html#id-1.6.11
        let left = values[0];
        let right = values[1];
        let top = values[2];
        let bottom = values[3];
        let left_start_y = values[4];
        let left_end_y = values[5];
        let right_start_y = values[6];
        let right_end_y = values[7];
        let top_start_x = values[8];
        let top_end_x = values[9];
        let bottom_start_x = values[10];
        let bottom_end_x = values[11];

        let (position, direction, reserved) = if left > 0 {
            self.reserve_space_left(left as u16);
            (
                Position {
                    x: 0,
                    y: left_start_y as u16,
                    width: left as u16,
                    height: (left_end_y - left_start_y) as u16,
                },
                ScreenSide::Left,
                left as u16,
            )
        } else if bottom > 0 {
            self.reserve_space_bottom(bottom as u16);
            (
                Position {
                    x: bottom_start_x as u16,
                    y: self.height - bottom as u16,
                    width: (bottom_end_x - bottom_start_x) as u16,
                    height: bottom as u16,
                },
                ScreenSide::Bottom,
                bottom as u16,
            )
        } else if top > 0 {
            self.reserve_space_top(top as u16);
            (
                Position {
                    x: top_start_x as u16,
                    y: 0,
                    width: (top_end_x - top_start_x) as u16,
                    height: top as u16,
                },
                ScreenSide::Top,
                top as u16,
            )
        } else if right > 0 {
            self.reserve_space_right(right as u16);
            (
                Position {
                    x: self.width - right as u16,
                    y: right_start_y as u16,
                    width: right as u16,
                    height: (right_end_y - right_start_y) as u16,
                },
                ScreenSide::Right,
                right as u16,
            )
        } else {
            anyhow::bail!(
                "Invalid _NET_WM_STRUT/_NET_WM_STRUT_PARTIAL values: [left,right,top,bottom]=0"
            );
        };

        if let Err(e) = self.add_reserved_client(ReservedClient {
            window,
            direction,
            position,
            reserved,
        }) {
            self.free_reserved_space(reserved, direction);

            Err(e)
        } else {
            Ok(())
        }
    }

    pub fn add_window(&mut self, window: XWindow) -> anyhow::Result<()> {
        // checking for strut and partial strut
        {
            let strut_partial_cookie = self.context.connection.send_request(&xcb::x::GetProperty {
                delete: false,
                window,
                property: self.context.atoms.net_wm_strut_partial,
                r#type: ATOM_CARDINAL,
                long_offset: 0,
                long_length: 12,
            });
            let strut_cookie = self.context.connection.send_request(&xcb::x::GetProperty {
                delete: false,
                window,
                property: self.context.atoms.net_wm_strut,
                r#type: ATOM_CARDINAL,
                long_offset: 0,
                long_length: 4,
            });

            if let Some(values) = self
                .context
                .connection
                .wait_for_reply(strut_partial_cookie)?
                .value::<u32>()
                .get(0..12)
            {
                self.handle_reserved_client(
                    window,
                    values
                        .try_into()
                        .context("strut_partial_cookie returned in invalid value")?,
                )?;
                let _ = self.update_atoms();
                return Ok(());
            }
            if let Some(values) = self
                .context
                .connection
                .wait_for_reply(strut_cookie)?
                .value::<u32>()
                .get(0..4)
            {
                self.handle_reserved_client(
                    window,
                    [
                        values[0], values[1], values[2], values[3], 0, 0, 0, 0, 0, 0, 0, 0,
                    ],
                )?;
                let _ = self.update_atoms();
                return Ok(());
            }
        }

        // if we have neither of those elements
        let client = Client::new(
            window,
            self.context.root_window,
            &self.context.connection,
            &self.context.atoms,
            self.context.current_workspace,
        )?;

        let frame = client.frame;
        let window = client.window;
        let idx = self.context.windows.push(client);
        self.context.window_lookup.insert(frame, idx);
        self.context.window_lookup.insert(window, idx);
        self.workspaces[self.context.current_workspace as usize]
            .spawn_window(idx, &mut self.context);
        Ok(())
    }

    pub fn close_focused_window(&mut self) {
        let Some(idx) = self.context.focused_window.take() else {
            return;
        };

        if self.context.windows[idx].close(&self.context.atoms, &self.context.connection) {
            self.workspaces
                .iter_mut()
                .for_each(|v| v.remove_window(idx, &mut self.context));

            self.context.windows.remove(idx);
            let mut to_remove = vec![];
            for (k, v) in self.context.window_lookup.iter() {
                if *v == idx {
                    to_remove.push(*k);
                }
            }
            for k in to_remove {
                self.context.window_lookup.remove(&k);
            }
        }
    }

    pub fn cycle_layout(&mut self) {
        self.workspaces[self.context.current_workspace as usize].cycle_layout(&mut self.context);
        _ = self.update_atoms();
    }

    pub fn set_layout(&mut self, new_layout: Layout) {
        self.workspaces[self.context.current_workspace as usize]
            .set_layout(new_layout, &mut self.context);
        _ = self.update_atoms();
    }

    pub fn kill_children(&mut self) {
        let mut cookies = vec![self
            .context
            .connection
            .send_request_checked(&SetInputFocus {
                focus: self.context.root_window,
                revert_to: xcb::x::InputFocus::Parent,
                time: CURRENT_TIME,
            })];

        for client in self.context.windows.iter() {
            cookies.push(
                self.context
                    .connection
                    .send_request_checked(&DestroyWindow {
                        window: client.window,
                    }),
            );
            cookies.push(
                self.context
                    .connection
                    .send_request_checked(&DestroyWindow {
                        window: client.frame,
                    }),
            );
        }

        for window in self.global_windows.iter() {
            cookies.push(
                self.context
                    .connection
                    .send_request_checked(&DestroyWindow {
                        window: window.window,
                    }),
            );
        }

        self.global_windows.clear();
        self.reserved_space_bottom = 0;
        self.reserved_space_left = 0;
        self.reserved_space_right = 0;
        self.reserved_space_top = 0;
        self.context.windows.clear();
        self.context.focused_window = None;
        self.context.window_lookup.clear();
        self.workspaces
            .iter_mut()
            .for_each(Workspace::clear_windows);

        for cookie in cookies.into_iter() {
            _ = self.context.connection.check_request(cookie);
        }
    }

    // pub fn draw_bar(&mut self) {
    //     _ = self.draw.draw_rect(Position::new(0, 0, self.width, 25), config::BORDER_COLOR_ACTIVE, config::BORDER_COLOR_ACTIVE);
    //     _ = self.draw.draw_string(10, 15, "Xephyr on :1.0", 0xffffffff, config::BORDER_COLOR_ACTIVE);
    //     _ = self.draw.finalise();
    // }
}

// reserve_space_DIR/free_space_DIR
impl Screen {
    // reserve
    pub fn reserve_space_top(&mut self, amount: u16) {
        self.reserved_space_top += amount;
        self.size_updated();
    }
    pub fn reserve_space_bottom(&mut self, amount: u16) {
        self.reserved_space_bottom += amount;
        self.size_updated();
    }
    pub fn reserve_space_left(&mut self, amount: u16) {
        self.reserved_space_left += amount;
        self.size_updated();
    }
    pub fn reserve_space_right(&mut self, amount: u16) {
        self.reserved_space_right += amount;
        self.size_updated();
    }

    // free
    pub fn free_space_top(&mut self, amount: u16) {
        self.reserved_space_top -= amount;
        self.size_updated();
    }
    pub fn free_space_bottom(&mut self, amount: u16) {
        self.reserved_space_bottom -= amount;
        self.size_updated();
    }
    pub fn free_space_left(&mut self, amount: u16) {
        self.reserved_space_left -= amount;
        self.size_updated();
    }
    pub fn free_space_right(&mut self, amount: u16) {
        self.reserved_space_right -= amount;
        self.size_updated();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ScreenSide {
    Top,
    Bottom,
    Left,
    Right,
}

pub struct ReservedClient {
    window: XWindow,
    position: Position,

    reserved: u16,
    direction: ScreenSide,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Client {
    pub window: XWindow,
    pub frame: XWindow,
    pub visible: bool,
    pub name: String,

    pub width: u16,
    pub height: u16,
    pub x: u16,
    pub y: u16,
    pub workspace: u8,
}

impl Client {
    pub fn new(
        window: XWindow,
        root_window: XWindow,
        conn: &Connection,
        atoms: &Atoms,
        workspace: u8,
    ) -> Result<Self> {
        let name = conn.wait_for_reply(conn.send_request(&GetProperty {
            window,
            long_length: 128,
            long_offset: 0,
            property: atoms.net_wm_name,
            delete: false,
            r#type: ATOM_ANY,
        }));
        let name = name
            .ok()
            .as_ref()
            .map(GetPropertyReply::value::<u8>)
            .and_then(|v| str::from_utf8(v).ok())
            .map(str::to_string)
            .unwrap_or_default();

        let frame = conn.generate_id();
        conn.send_and_check_request(&CreateWindow {
            depth: COPY_FROM_PARENT as u8,
            wid: frame,
            border_width: config::BORDER_SIZE,
            class: xcb::x::WindowClass::InputOutput,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            parent: root_window,
            visual: COPY_FROM_PARENT,
            value_list: &[
                Cw::BackPixel(0),
                Cw::BorderPixel(config::BORDER_COLOR),
                Cw::EventMask(
                    EventMask::PROPERTY_CHANGE
                        | EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::ENTER_WINDOW,
                ),
            ],
        })
        .context("failed to create a frame")?;

        conn.send_and_check_request(&ReparentWindow {
            parent: frame,
            window,
            x: 0,
            y: 0,
        })
        .context("failed to reparent the child to the frame")?;

        trace_result!(conn.send_and_check_request(&ChangeWindowAttributes {
            window: frame,
            value_list: &[Cw::EventMask(EventMask::SUBSTRUCTURE_NOTIFY | EventMask::ENTER_WINDOW | EventMask::KEY_PRESS | EventMask::KEY_RELEASE)]
        }); "failed to enable client events for the frame");

        Ok(Self {
            window,
            visible: false,
            frame,
            name,
            width: 1,
            height: 1,
            x: 0,
            y: 0,
            workspace,
        })
    }

    pub fn destroy(&mut self, conn: &Connection) {
        trace_result!(conn.send_and_check_request(&DestroyWindow { window: self.frame }); "failed to destroy the frame");
    }

    pub fn close(&mut self, atoms: &Atoms, conn: &Connection) -> bool {
        if ewmh::delete_window(self.window, atoms, conn) {
            self.destroy(conn);
            true
        } else {
            false
        }
    }

    pub fn focus(&mut self, conn: &Connection) {
        trace_result!(conn.send_and_check_request(&ChangeWindowAttributes {
            window: self.frame,
            value_list: &[Cw::BorderPixel(config::BORDER_COLOR_ACTIVE)],
        }); "failed to set the border color");
        trace_result!(conn.send_and_check_request(&SetInputFocus {
            focus: self.window,
            revert_to: xcb::x::InputFocus::Parent,
            time: CURRENT_TIME,
        }); "failed to focus the input");
    }

    pub fn unfocus(&mut self, conn: &Connection) {
        trace_result!(conn.send_and_check_request(&ChangeWindowAttributes {
            window: self.frame,
            value_list: &[Cw::BorderPixel(config::BORDER_COLOR)],
        }); "failed to reset the border color");
    }

    pub fn update(&mut self, width: u16, height: u16, x: u16, y: u16, conn: &Connection) {
        let border_double = config::BORDER_SIZE * 2;

        trace_result!(conn.send_and_check_request(&ConfigureWindow {
            window: self.frame,
            value_list: &[
                ConfigWindow::X(x as i32),
                ConfigWindow::Y(y as i32),
                ConfigWindow::Width((width - border_double) as u32),
                ConfigWindow::Height((height - border_double) as u32),
            ],
        }));
        trace_result!(conn.send_and_check_request(&ConfigureWindow {
            window: self.window,
            value_list: &[
                ConfigWindow::X(0),
                ConfigWindow::Y(WINDOW_BAR_HEIGHT as i32),
                ConfigWindow::Width((width - border_double) as u32),
                ConfigWindow::Height((height - border_double - WINDOW_BAR_HEIGHT) as u32),
            ],
        }));
    }

    pub fn hide(&mut self, conn: &Connection) {
        self.visible = false;
        let window_unmap = conn.send_request_checked(&UnmapWindow {
            window: self.window,
        });
        let frame_unmap = conn.send_request_checked(&UnmapWindow { window: self.frame });
        trace_result!(
            conn.check_request(window_unmap);
            "failed to unmap the window"
        );
        trace_result!(conn.check_request(frame_unmap); "failed to unmap the frame");
    }

    pub fn show(&mut self, conn: &Connection) {
        self.visible = true;
        let map_frame = conn.send_request_checked(&MapWindow { window: self.frame });
        let map_window = conn.send_request_checked(&MapWindow {
            window: self.window,
        });
        trace_result!(conn.check_request(map_frame); "failed to map the frame");
        trace_result!(conn.check_request(map_window); "failed to map the window");
    }
}
