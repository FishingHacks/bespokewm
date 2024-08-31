use std::fmt::{Debug, Display};

use xcb::Connection;

pub trait AbstractWindow: Debug + Eq + Copy {
    fn update(&mut self, width: u16, height: u16, x: u16, y: u16, conn: &Connection);
    fn hide(&mut self, conn: &Connection);
    fn show(&mut self, conn: &Connection);
    fn focus(&mut self, conn: &Connection);
    fn unfocus(&mut self, conn: &Connection);
    fn destroy(&mut self, conn: &Connection);
    /// Requests a window delete
    /// If this function returns true, it means we have
    /// removed the window and not just scheduled a delete
    /// Thus we need to remove the window from all relevant places
    fn close(&mut self, atoms: &crate::atoms::Atoms, conn: &Connection) -> bool;
}
#[derive(Debug)]
pub struct ClientWindow<T: AbstractWindow> {
    data: T,
    width: u16,
    height: u16,
    x: u16,
    y: u16,
}
impl<T: AbstractWindow> ClientWindow<T> {
    fn update(&mut self, width: u16, height: u16, x: u16, y: u16, conn: &Connection) {
        self.x = x;
        self.y = y;
        self.width = width;
        self.height = height;
        self.data.update(width, height, x, y, conn);
    }

    fn new(data: T) -> Self {
        Self {
            data,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Grid,
    MasterLeft,
    MasterRight,
    MasterLeftGrid,
    MasterRightGrid,
    Monocle,
}

impl Display for Layout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grid => "HHH",
            Self::MasterLeft => "[]=",
            Self::MasterRight => "=[]",
            Self::MasterLeftGrid => "[]H",
            Self::MasterRightGrid => "H[]",
            Self::Monocle => "[M]",
        })
    }
}

impl Layout {
    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_grid<T: AbstractWindow>(
        windows: &mut [ClientWindow<T>],
        gap: u16,
        screen_position: Position,
        conn: &Connection,
    ) {
        let half_gap = gap / 2;

        let num_wins_horz = (windows.len() as f64).sqrt().ceil() as u16;
        let num_wins_vert = windows.len().div_ceil(num_wins_horz as usize) as u16;

        let win_width = screen_position.width / num_wins_horz;
        let win_height = screen_position.height / num_wins_vert;

        let offset_x = half_gap + screen_position.x;
        let offset_y = half_gap + screen_position.y;

        let len = windows.len();
        for i in 0..windows.len() {
            let x = (i as u16 % num_wins_horz) * win_width + offset_x;
            let y = (i as u16 / num_wins_horz) * win_height + offset_y;

            let i = len - 1 - i;
            windows[i].update(win_width - gap, win_height - gap, x, y, conn);
        }
    }

    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_with_master<T: AbstractWindow>(
        windows: &mut [ClientWindow<T>],
        gap: u16,
        screen_position: Position,
        master_is_left: bool,
        conn: &Connection,
    ) {
        let half_gap = gap / 2;
        let half_width = screen_position.width / 2;

        // we do -1 because that later excludes the last element and is the last element
        let len = windows.len() - 1;
        windows[len].update(
            half_width - gap,
            screen_position.height - gap,
            if master_is_left {
                half_gap
            } else {
                half_width + half_gap
            } + screen_position.x,
            half_gap + screen_position.y,
            conn,
        );

        let width = half_width - gap;
        let height_gapless = screen_position.height / len as u16;
        let height = height_gapless - gap;
        let x = if master_is_left {
            half_width + half_gap
        } else {
            half_gap
        } + screen_position.x;

        for i in 0..len {
            windows[len - 1 - i].update(
                width,
                height,
                x,
                i as u16 * height_gapless + half_gap + screen_position.y,
                conn,
            );
        }
    }

    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_with_master_grid<T: AbstractWindow>(
        windows: &mut [ClientWindow<T>],
        gap: u16,
        screen_position: Position,
        master_is_left: bool,
        conn: &Connection,
    ) {
        let half_gap = gap / 2;
        let half_width = screen_position.width / 2;

        // we do -1 because that later excludes the last element and is the last element
        let len = windows.len() - 1;
        windows[len].update(
            half_width - gap,
            screen_position.height - gap,
            if master_is_left {
                half_gap
            } else {
                half_width + half_gap
            } + screen_position.x,
            half_gap + screen_position.y,
            conn,
        );

        if master_is_left {
            Self::retile_grid(
                &mut windows[0..len],
                gap,
                Position::new(
                    half_width + screen_position.x,
                    screen_position.y,
                    half_width,
                    screen_position.height,
                ),
                conn,
            );
        } else {
            Self::retile_grid(
                &mut windows[0..len],
                gap,
                Position::new(
                    screen_position.x,
                    screen_position.y,
                    half_width,
                    screen_position.height,
                ),
                conn,
            );
        }
    }

    fn retile_monocle<T: AbstractWindow>(
        windows: &mut [ClientWindow<T>],
        gap: u16,
        screen_position: Position,
        conn: &Connection,
    ) {
        let len = windows.len() - 1;

        let x = screen_position.x + gap / 2;
        let y = screen_position.y + gap / 2;

        for window in windows[0..len].iter_mut() {
            window.update(30, 30, x, y, conn);
        }

        windows[len].update(
            screen_position.width - gap,
            screen_position.height - gap,
            x,
            y,
            conn,
        );
    }

    fn retile<T: AbstractWindow>(self, tiler: &mut Workspace<T>, conn: &Connection) {
        if tiler.windows.len() < 1 {
            return;
        } else if tiler.windows.len() == 1 {
            // the window is always gonna be the entire window
            tiler.windows[0].update(
                tiler.pos.width - tiler.gap,
                tiler.pos.height - tiler.gap,
                tiler.gap / 2 + tiler.pos.x,
                tiler.gap / 2 + tiler.pos.y,
                conn,
            );

            return;
        }

        match self {
            Self::Grid => Self::retile_grid(&mut tiler.windows, tiler.gap, tiler.pos, conn),
            Self::MasterLeft => {
                Self::retile_with_master(&mut tiler.windows, tiler.gap, tiler.pos, true, conn)
            }
            Self::MasterRight => {
                Self::retile_with_master(&mut tiler.windows, tiler.gap, tiler.pos, false, conn)
            }
            Self::MasterLeftGrid => {
                Self::retile_with_master_grid(&mut tiler.windows, tiler.gap, tiler.pos, true, conn)
            }
            Self::MasterRightGrid => {
                Self::retile_with_master_grid(&mut tiler.windows, tiler.gap, tiler.pos, false, conn)
            }
            Self::Monocle => Self::retile_monocle(&mut tiler.windows, tiler.gap, tiler.pos, conn),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Position {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug)]
pub struct Workspace<T: AbstractWindow> {
    windows: Vec<ClientWindow<T>>,
    floating_windows: Vec<ClientWindow<T>>,
    pos: Position,
    gap: u16,
    layout: Layout,
    is_showing: bool,
    name: String,
    id: u32,
    focused: Option<T>,
}

impl<T: AbstractWindow> Workspace<T> {
    pub fn new(width: u16, height: u16, gap: u16, id: u32) -> Self {
        Self {
            windows: vec![],
            floating_windows: vec![],
            focused: None,
            pos: Position::new(0, 0, width, height),
            gap,
            layout: Layout::Grid,
            is_showing: false,
            name: format!("Desktop {id}"),
            id,
        }
    }

    fn retile(&mut self, conn: &Connection) {
        if self.windows.len() > 0 && self.is_showing {
            self.layout.retile(self, conn);
        }
    }

    pub fn show(&mut self, conn: &Connection) {
        self.is_showing = true;

        for win in self.windows.iter_mut() {
            win.data.show(conn);
        }
        self.retile(conn);

        for win in self.windows.iter_mut() {
            win.data.show(conn);
            win.data.update(win.width, win.height, win.x, win.y, conn);
        }
    }

    pub fn hide(&mut self, conn: &Connection) {
        self.is_showing = false;
        self.unfocus_all(conn);
        for win in self.windows.iter_mut() {
            win.data.hide(conn);
        }
        for win in self.floating_windows.iter_mut() {
            win.data.hide(conn);
        }
    }

    pub fn cycle_layout(&mut self, conn: &Connection) {
        self.layout = match self.layout {
            Layout::Grid => Layout::MasterLeft,
            Layout::MasterLeft => Layout::MasterRight,
            Layout::MasterRight => Layout::MasterLeftGrid,
            Layout::MasterLeftGrid => Layout::MasterRightGrid,
            Layout::MasterRightGrid => Layout::Monocle,
            Layout::Monocle => Layout::Grid,
        };

        self.retile(conn);
    }

    pub fn set_layout(&mut self, new_layout: Layout, conn: &Connection) {
        if self.layout == new_layout {
            return;
        }
        self.layout = new_layout;

        self.retile(conn);
    }

    pub fn spawn_window(&mut self, mut data: T, conn: &Connection) {
        data.show(conn);

        self.windows.push(ClientWindow::new(data));
        self.retile(conn);
    }

    /// finds the window to toggle floating on. Usize is the window index and the boolean is if it is currently not floating
    fn find_floating_window(&mut self, window: &T) -> Option<(usize, bool)> {
        for i in 0..self.windows.len() {
            if self.windows[i].data.eq(window) {
                return Some((i, true));
            }
        }
        for i in 0..self.floating_windows.len() {
            if self.floating_windows[i].data.eq(window) {
                return Some((i, false));
            }
        }
        None
    }

    pub fn toggle_floating(&mut self, window: &T, conn: &Connection) {
        let Some((idx, enable)) = self.find_floating_window(window) else {
            return;
        };

        if enable {
            let val = self.windows.remove(idx);
            self.floating_windows.push(val);
        } else {
            let val = self.floating_windows.remove(idx);
            self.windows.push(val);
        }

        self.retile(conn);
    }

    pub fn remove_window(&mut self, predicate: impl Fn(&T) -> bool, conn: &Connection) -> Vec<T> {
        let mut removed: Vec<T> = vec![];
        self.unfocus(&predicate, conn);

        let len = self.windows.len();
        for i in 0..self.windows.len() {
            let i = len - 1 - i;
            if predicate(&self.windows[i].data) {
                self.windows[i].data.destroy(conn);
                removed.push(self.windows.remove(i).data);
            }
        }

        let len = self.floating_windows.len();
        for i in 0..self.floating_windows.len() {
            let i = len - 1 - i;
            if predicate(&self.floating_windows[i].data) {
                self.floating_windows[i].data.destroy(conn);
                removed.push(self.floating_windows.remove(i).data);
            }
        }

        if removed.len() > 0 {
            self.retile(conn);
        }

        removed
    }

    /// Requests a window delete
    /// Returns all removed clients
    pub fn close_window(&mut self, predicate: impl Fn(&T) -> bool, atoms: &crate::atoms::Atoms, conn: &Connection) -> Vec<T> {
        let mut clients = vec![];
        self.unfocus(&predicate, conn);

        let len = self.windows.len();
        for i in 0..len {
            let i = len - 1 - i;
            if predicate(&self.windows[i].data) {
                if self.windows[i].data.close(atoms, conn) {
                    clients.push(self.windows.remove(i).data);
                }
            }
        }

        let len = self.floating_windows.len();
        for i in 0..len {
            let i = len - 1 - i;
            if predicate(&self.floating_windows[i].data) {
                if self.floating_windows[i].data.close(atoms, conn) {
                    clients.push(self.floating_windows.remove(i).data);
                }
            }
        }

        clients
    }

    pub fn set_screen_size(&mut self, width: u16, height: u16, conn: &Connection) {
        self.pos.width = width;
        self.pos.height = height;

        self.retile(conn);
    }

    pub fn set_screen_offset(&mut self, offset_x: u16, offset_y: u16, conn: &Connection) {
        self.pos.x = offset_x;
        self.pos.y = offset_y;

        self.retile(conn);
    }

    pub fn set_screen_position(&mut self, pos: Position, conn: &Connection) {
        self.pos = pos;

        self.retile(conn);
    }

    pub fn get_screen_position(&self) -> Position {
        self.pos
    }

    pub fn windows(&self) -> impl Iterator<Item = &T> {
        self.windows
            .iter()
            .chain(self.floating_windows.iter())
            .map(|el| &el.data)
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn find_client(&self, predicate: impl Fn(&T) -> bool) -> Option<&T> {
        for window in self.windows.iter() {
            if predicate(&window.data) {
                return Some(&window.data);
            }
        }
        for window in self.floating_windows.iter() {
            if predicate(&window.data) {
                return Some(&window.data);
            }
        }
        None
    }

    pub fn focus_client(&mut self, mut client: T, conn: &Connection) {
        if let Some(mut focused) = self.focused.replace(client) {
            focused.unfocus(conn);
        }
        client.focus(conn);
    }

    pub fn unfocus(&mut self, predicate: &dyn Fn(&T) -> bool, conn: &Connection) {
        if let Some(focused) = self.focused.as_ref() {
            if predicate(focused) {
                if let Some(mut focused) = self.focused.take() {
                    focused.unfocus(conn);
                }
            }
        }
    }
    
    pub fn unfocus_all(&mut self, conn: &Connection) {
        if let Some(mut focused) = self.focused.take() {
            focused.unfocus(conn);
        }
    }

    pub fn clear_windows(&mut self) {
        self.windows.clear();
        self.floating_windows.clear();
        self.focused = None;
    }
}
