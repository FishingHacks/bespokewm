use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use tracing::{error, warn};
use xcb::{
    x::{
        ChangeWindowAttributes, ConfigWindow, ConfigureWindow, CreateWindow, Cw, DestroyWindow,
        EventMask, MapWindow, ReparentWindow, SetInputFocus, UnmapWindow, Window as XWindow,
        ATOM_CARDINAL, COPY_FROM_PARENT, CURRENT_TIME,
    },
    Connection, Xid,
};

use crate::{
    atoms::Atoms,
    config, ewmh,
    layout::{AbstractWindow, Layout, Position, Workspace},
};

pub struct Screen {
    width: u16,
    height: u16,
    reserved_space_bottom: u16,
    reserved_space_top: u16,
    reserved_space_left: u16,
    reserved_space_right: u16,
    window_lookup: HashMap<XWindow, u8>,

    workspaces: [Workspace<Client>; 10],
    global_windows: Vec<ReservedClient>,
    current_workspace: u8,
    atoms: Atoms,
    root_window: XWindow,
    connection: Arc<Connection>,

    // draw: DrawContext,
    /// Option<(window, frame)>
    /// The frame may be Window(0) to indicate the window is a reserved client
    focused_window: Option<(XWindow, XWindow)>,
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
            atoms,
            root_window,
            connection,
            width,
            height,
            reserved_space_bottom: 0,
            reserved_space_left: 0,
            reserved_space_right: 0,
            reserved_space_top: 0,
            window_lookup: Default::default(),
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
            global_windows: vec![],
            focused_window: None,
            current_workspace: 1,
        };
        ewmh::set_number_of_desktops(10, root_window, &atoms, &me.connection)?;
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
                &self.connection,
            );
        }
        _ = self.update_atoms();
    }

    pub fn add_reserved_client(&mut self, client: ReservedClient) -> anyhow::Result<()> {
        if self.global_windows.len() > u8::MAX as usize {
            error!("Tried to register >255 global clients!");
            anyhow::bail!("Not supporting >255 global clients!");
        }
        let map_cookie = self.connection.send_request_checked(&MapWindow { window: client.window });
        let change_attributes_cookie = self.connection
            .send_request_checked(&ChangeWindowAttributes {
                window: client.window,
                value_list: &[Cw::EventMask(EventMask::ENTER_WINDOW)],
            });
        
        self.connection.check_request(map_cookie)?;
        self.connection.check_request(change_attributes_cookie)?;
        self.global_windows.push(client);
        self.update_atoms()?;
        Ok(())
    }

    pub fn switch_workspace(&mut self, new_workspace: u8) -> Result<(), xcb::ProtocolError> {
        let old_workspace = self.current_workspace;
        self.current_workspace = new_workspace;
        self.update_atoms()?;
        self.workspaces[old_workspace as usize].hide(&self.connection);
        self.workspaces[new_workspace as usize].show(&self.connection);
        Ok(())
    }

    pub fn update_atoms(&self) -> Result<(), xcb::ProtocolError> {
        let atoms = &self.atoms;
        let conn = &self.connection;

        ewmh::set_desktop_viewport(
            self.reserved_space_left as u32,
            self.reserved_space_top as u32,
            self.root_window,
            atoms,
            conn,
        )?;
        ewmh::set_number_of_desktops(self.workspaces.len() as u32, self.root_window, atoms, conn)?;
        ewmh::set_current_desktop(self.current_workspace as u32, self.root_window, atoms, conn)?;
        ewmh::set_desktop_names(&self.workspaces, self.root_window, atoms, conn)?;
        ewmh::set_wm_desktop(&self.workspaces, atoms, conn)?;
        // TODO: when implementing reparenting, update this
        ewmh::set_client_list(self.window_lookup.keys(), self.root_window, atoms, conn)?;
        // this is the same as the above
        ewmh::set_client_list_stacking(self.window_lookup.keys(), self.root_window, atoms, conn)?;
        ewmh::set_showing_desktop(false, self.root_window, atoms, conn)?;

        Ok(())
    }

    pub fn enter_client(&mut self, client: XWindow) {
        for workspace in self.workspaces.iter_mut() {
            workspace.unfocus_all(&self.connection);
        }

        if client == self.root_window {
            trace_result!(self.connection.send_and_check_request(&SetInputFocus {
                time: CURRENT_TIME,
                focus: self.root_window,
                revert_to: xcb::x::InputFocus::Parent
            }); "failed to give root focus");
            self.focused_window = None;

            return;
        }

        if let Some(workspace) = self.window_lookup.get(&client) {
            let workspace = &mut self.workspaces[*workspace as usize];
            let client = workspace.find_client(|v| v.frame == client);
            if let Some(client) = client {
                self.focused_window = Some((client.window, client.frame));
                workspace.focus_client(*client, &self.connection);
            }
        } else {
            for reserved_client in self.global_windows.iter() {
                if reserved_client.window == client {
                    _ = self.connection.send_and_check_request(&SetInputFocus {
                        time: CURRENT_TIME,
                        focus: reserved_client.window,
                        revert_to: xcb::x::InputFocus::Parent,
                    });
                    self.focused_window = Some((reserved_client.window, XWindow::none()));
                    break;
                }
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
        for workspace in self.workspaces.iter_mut() {
            for client in workspace
                .remove_window(|client| client.window == window, &self.connection)
                .iter()
            {
                self.window_lookup.remove(&client.frame);
            }
        }

        let len = self.global_windows.len();
        for i in 0..self.global_windows.len() {
            let i = len - 1 - i;
            if self.global_windows[i].window == window {
                let child = self.global_windows.remove(i);
                self.free_reserved_space(child.reserved, child.direction);
                _ = self.connection.send_and_check_request(&UnmapWindow {
                    window: child.window,
                });
                _ = self.connection.send_and_check_request(&DestroyWindow {
                    window: child.window,
                });
            }
        }

        trace_result!(self.connection.flush(); "failed to flush the connection after window remove");
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
            let strut_partial_cookie = self.connection.send_request(&xcb::x::GetProperty {
                delete: false,
                window,
                property: self.atoms.net_wm_strut_partial,
                r#type: ATOM_CARDINAL,
                long_offset: 0,
                long_length: 12,
            });
            let strut_cookie = self.connection.send_request(&xcb::x::GetProperty {
                delete: false,
                window,
                property: self.atoms.net_wm_strut,
                r#type: ATOM_CARDINAL,
                long_offset: 0,
                long_length: 4,
            });

            if let Some(values) = self
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
                self.update_atoms();
                return Ok(());
            }
            if let Some(values) = self
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
                self.update_atoms();
                return Ok(());
            }
        }

        // if we have neither of those elements
        let client = Client::new(window, self.root_window, &self.connection)?;
        self.window_lookup
            .insert(client.frame, self.current_workspace);
        println!("client: {client:?}");
        self.workspaces[self.current_workspace as usize].spawn_window(client, &self.connection);
        Ok(())
    }

    pub fn close_focused_window(&mut self) {
        let Some((window, frame)) = self.focused_window else {
            return;
        };
        self.focused_window = None;

        if frame.is_none() {
            // global window

            let len = self.global_windows.len();
            for i in 0..len {
                let i = len - 1 - i;
                if self.global_windows[i].window == window {
                    if ewmh::delete_window(
                        self.global_windows[i].window,
                        &self.atoms,
                        &self.connection,
                    ) {
                        let client = self.global_windows.remove(i);
                        self.free_reserved_space(client.reserved, client.direction);
                    }
                }
            }
        } else {
            if let Some(&workspace_id) = self.window_lookup.get(&frame) {
                for client in self.workspaces[workspace_id as usize].close_window(
                    |c| c.window == window,
                    &self.atoms,
                    &self.connection,
                ) {
                    self.window_lookup.remove(&client.frame);
                }
            }
        }
    }

    pub fn cycle_layout(&mut self) {
        self.workspaces[self.current_workspace as usize].cycle_layout(&self.connection);
        _ = self.update_atoms();
    }

    pub fn set_layout(&mut self, new_layout: Layout) {
        self.workspaces[self.current_workspace as usize].set_layout(new_layout, &self.connection);
        _ = self.update_atoms();
    }

    pub fn kill_children(&mut self) {
        let mut cookies = vec![self.connection.send_request_checked(&SetInputFocus {
            focus: self.root_window,
            revert_to: xcb::x::InputFocus::Parent,
            time: CURRENT_TIME,
        })];

        for client in self.workspaces.iter().flat_map(Workspace::windows) {
            cookies.push(self.connection.send_request_checked(&DestroyWindow {
                window: client.window,
            }));
            cookies.push(self.connection.send_request_checked(&DestroyWindow {
                window: client.frame,
            }));
        }
        for window in self.global_windows.iter() {
            cookies.push(self.connection.send_request_checked(&DestroyWindow {
                window: window.window,
            }));
        }

        self.global_windows.clear();
        self.reserved_space_bottom = 0;
        self.reserved_space_left = 0;
        self.reserved_space_right = 0;
        self.reserved_space_top = 0;
        self.workspaces
            .iter_mut()
            .for_each(Workspace::clear_windows);

        for cookie in cookies.into_iter() {
            _ = self.connection.check_request(cookie);
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Client {
    pub window: XWindow,
    frame: XWindow,
    visible: bool,
}

impl Client {
    pub fn new(window: XWindow, root_window: XWindow, conn: &Connection) -> Result<Self> {
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
            value_list: &[Cw::BackPixel(0), Cw::BorderPixel(config::BORDER_COLOR)],
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
            value_list: &[Cw::EventMask(EventMask::SUBSTRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE | EventMask::ENTER_WINDOW | EventMask::KEY_PRESS | EventMask::KEY_RELEASE)]
        }); "failed to enable client events for the frame");

        Ok(Self {
            window,
            visible: false,
            frame,
        })
    }
}

impl AbstractWindow for Client {
    fn destroy(&mut self, conn: &Connection) {
        trace_result!(conn.send_and_check_request(&UnmapWindow { window: self.frame }); "failed to unmap the frame");
        trace_result!(conn.send_and_check_request(&DestroyWindow { window: self.frame }); "failed to destroy the frame");
    }

    fn close(&mut self, atoms: &Atoms, conn: &Connection) -> bool {
        if ewmh::delete_window(self.window, atoms, conn) {
            self.destroy(conn);
            true
        } else {
            false
        }
    }

    fn focus(&mut self, conn: &Connection) {
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

    fn unfocus(&mut self, conn: &Connection) {
        trace_result!(conn.send_and_check_request(&ChangeWindowAttributes {
            window: self.frame,
            value_list: &[Cw::BorderPixel(config::BORDER_COLOR)],
        }); "failed to reset the border color");
    }

    fn update(&mut self, width: u16, height: u16, x: u16, y: u16, conn: &Connection) {
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
                ConfigWindow::Y(0),
                ConfigWindow::Width((width - border_double) as u32),
                ConfigWindow::Height((height - border_double) as u32),
            ],
        }));
    }

    fn hide(&mut self, conn: &Connection) {
        self.visible = false;
        _ = conn.send_and_check_request(&UnmapWindow {
            window: self.window,
        });
        _ = conn.send_and_check_request(&UnmapWindow { window: self.frame });
    }

    fn show(&mut self, conn: &Connection) {
        self.visible = true;
        trace_result!(conn.send_and_check_request(&MapWindow { window: self.frame }));
        trace_result!(conn.send_and_check_request(&MapWindow {
            window: self.window
        }));
    }
}
