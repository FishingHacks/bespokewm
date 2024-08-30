use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use tracing::{error, info, warn};
use xcb::{
    x::{
        ChangeWindowAttributes, ConfigWindow, ConfigureWindow, CreateWindow, Cw, DestroyWindow,
        EventMask, MapWindow, ReparentWindow, SetInputFocus, UnmapWindow, Window as XWindow,
        COPY_FROM_PARENT, CURRENT_TIME,
    },
    Connection,
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
}

impl Screen {
    pub fn new(
        width: u16,
        height: u16,
        gap: u16,
        atoms: Atoms,
        root_window: XWindow,
        connection: Arc<Connection>,
    ) -> anyhow::Result<Self, xcb::ProtocolError> {
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
            workspaces: [
                Workspace::new(width, height, gap, 1),
                Workspace::new(width, height, gap, 2),
                Workspace::new(width, height, gap, 3),
                Workspace::new(width, height, gap, 4),
                Workspace::new(width, height, gap, 5),
                Workspace::new(width, height, gap, 6),
                Workspace::new(width, height, gap, 7),
                Workspace::new(width, height, gap, 8),
                Workspace::new(width, height, gap, 9),
                Workspace::new(width, height, gap, 10),
            ],
            global_windows: vec![],
            current_workspace: 1,
        };
        ewmh::set_number_of_desktops(10, root_window, &atoms, &me.connection)?;
        me.switch_workspace(1)?;

        me.update_atoms();
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
        self.update_atoms();
    }

    pub fn add_reserved_client(&mut self, client: ReservedClient) -> anyhow::Result<()> {
        if self.global_windows.len() > u8::MAX as usize {
            error!("Tried to register >255 global clients!");
            anyhow::bail!("Not supporting >255 global clients!");
        }
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

            return;
        }

        if let Some(workspace) = self.window_lookup.get(&client) {
            let workspace = &mut self.workspaces[*workspace as usize];
            let client = workspace.find_client(|v| v.frame == client);
            if let Some(client) = client {
                workspace.focus_client(*client, &self.connection);
            }
        }
    }

    pub fn remove_window(&mut self, window: XWindow) {
        println!("Destroying {window:?}");

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
                self.connection.send_request(&UnmapWindow {
                    window: child.window,
                });
                self.connection.send_request(&DestroyWindow {
                    window: child.window,
                });
            }
        }

        trace_result!(self.connection.flush(); "failed to flush the connection after window remove");
    }

    pub fn add_window(&mut self, window: XWindow) -> anyhow::Result<()> {
        println!("Adding window {window:?}");
        let client = Client::new(window, self.root_window, &self.connection)?;
        self.window_lookup
            .insert(client.frame, self.current_workspace);
        println!("client: {client:?}");
        self.workspaces[self.current_workspace as usize].spawn_window(client, &self.connection);
        Ok(())
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
            cookies.push(self.connection.send_request_checked(&DestroyWindow { window: client.window }));
            cookies.push(self.connection.send_request_checked(&DestroyWindow { window: client.frame }));
        }
        for window in self.global_windows.iter() {
            cookies.push(self.connection.send_request_checked(&DestroyWindow { window: window.window }));
        }

        self.global_windows.clear();
        self.reserved_space_bottom = 0;
        self.reserved_space_left = 0;
        self.reserved_space_right = 0;
        self.reserved_space_top = 0;
        self.workspaces.iter_mut().for_each(Workspace::clear_windows);

        for cookie in cookies.into_iter() {
            _ = self.connection.check_request(cookie);
        }
    }
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
        self.reserved_space_top += amount;
        self.size_updated();
    }
    pub fn free_space_bottom(&mut self, amount: u16) {
        self.reserved_space_bottom += amount;
        self.size_updated();
    }
    pub fn free_space_left(&mut self, amount: u16) {
        self.reserved_space_left += amount;
        self.size_updated();
    }
    pub fn free_space_right(&mut self, amount: u16) {
        self.reserved_space_right += amount;
        self.size_updated();
    }
}

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
        conn.send_request(&UnmapWindow {
            window: self.window,
        });
        conn.send_request(&UnmapWindow { window: self.frame });
    }

    fn show(&mut self, conn: &Connection) {
        self.visible = true;
        trace_result!(conn.send_and_check_request(&MapWindow { window: self.frame }));
        trace_result!(conn.send_and_check_request(&MapWindow {
            window: self.window
        }));
    }
}
