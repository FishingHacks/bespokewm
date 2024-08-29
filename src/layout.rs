use std::fmt::Debug;

use xcb::Connection;

pub trait AbstractWindow: Debug + Eq {
    fn update(&mut self, width: u16, height: u16, x: u16, y: u16, conn: &Connection);
    fn hide(&mut self, conn: &Connection);
    fn show(&mut self, conn: &Connection);
}
#[derive(Debug)]
pub struct Window<T: AbstractWindow> {
    data: T,
    width: u16,
    height: u16,
    x: u16,
    y: u16,
}
impl<T: AbstractWindow> Window<T> {
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

#[derive(Debug, Clone, Copy)]
pub enum Layout {
    Grid,
    MasterLeft,
    MasterRight,
    MasterLeftGrid,
    MasterRightGrid,
}

impl Layout {
    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_grid<T: AbstractWindow>(
        windows: &mut [Window<T>],
        gap: u16,
        screen_width: u16,
        screen_height: u16,
        offset_x: u16,
        offset_y: u16,
        conn: &Connection,
    ) {
        let half_gap = gap / 2;

        let num_wins_horz = (windows.len() as f64).sqrt().ceil() as u16;
        let num_wins_vert = windows.len().div_ceil(num_wins_horz as usize) as u16;

        let win_width = screen_width / num_wins_horz;
        let win_height = screen_height / num_wins_vert;

        let offset_x = half_gap + offset_x;
        let offset_y = half_gap + offset_y;

        for i in 0..windows.len() {
            let x = (i as u16 % num_wins_horz) * win_width + offset_x;
            let y = (i as u16 / num_wins_horz) * win_height + offset_y;

            let i = windows.len() - 1 - i;
            windows[i].update(win_width - gap, win_height - gap, x, y, conn);
        }
    }

    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_with_master<T: AbstractWindow>(
        windows: &mut [Window<T>],
        gap: u16,
        screen_width: u16,
        screen_height: u16,
        master_is_left: bool,
        conn: &Connection,
    ) {
        let half_gap = gap / 2;
        let half_width = screen_width / 2;

        // we do -1 because that later excludes the last element and is the last element
        let len = windows.len() - 1;
        windows[len].update(
            half_width - gap,
            screen_height - gap,
            if master_is_left {
                half_gap
            } else {
                half_width + half_gap
            },
            half_gap,
            conn
        );

        let width = half_width - gap;
        let height_gapless = screen_height / len as u16;
        let height = height_gapless - gap;
        let x = if master_is_left {
            half_width + half_gap
        } else {
            half_gap
        };

        for i in 0..len {
            windows[len - 1 - i].update(width, height, x, i as u16 * height_gapless + half_gap, conn);
        }
    }

    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_with_master_grid<T: AbstractWindow>(
        windows: &mut [Window<T>],
        gap: u16,
        screen_width: u16,
        screen_height: u16,
        master_is_left: bool,
        conn: &Connection,
    ) {
        let half_gap = gap / 2;
        let half_width = screen_width / 2;

        // we do -1 because that later excludes the last element and is the last element
        let len = windows.len() - 1;
        windows[len].update(
            half_width - gap,
            screen_height - gap,
            if master_is_left {
                half_gap
            } else {
                half_width + half_gap
            },
            half_gap,
            conn
        );

        if master_is_left {
            Self::retile_grid(
                &mut windows[0..len],
                gap,
                half_width,
                screen_height,
                half_width,
                0,
                conn,
            );
        } else {
            Self::retile_grid(
                &mut windows[0..len],
                gap,
                half_width,
                screen_height,
                0,
                0,
                conn,
            );
        }
    }

    fn retile<T: AbstractWindow>(self, tiler: &mut Tiler<T>, conn: &Connection) {
        if tiler.windows.len() < 1 {
            return;
        } else if tiler.windows.len() == 1 {
            // the window is always gonna be the entire window
            tiler.windows[0].update(
                tiler.screen_width - tiler.gap,
                tiler.screen_height - tiler.gap,
                tiler.gap / 2,
                tiler.gap / 2,
                conn
            );

            return;
        }

        match self {
            Self::Grid => Self::retile_grid(
                &mut tiler.windows,
                tiler.gap,
                tiler.screen_width,
                tiler.screen_height,
                0,
                0,
                conn
            ),
            Self::MasterLeft => Self::retile_with_master(
                &mut tiler.windows,
                tiler.gap,
                tiler.screen_width,
                tiler.screen_height,
                true,
                conn
            ),
            Self::MasterRight => Self::retile_with_master(
                &mut tiler.windows,
                tiler.gap,
                tiler.screen_width,
                tiler.screen_height,
                false,
                conn
            ),
            Self::MasterLeftGrid => Self::retile_with_master_grid(
                &mut tiler.windows,
                tiler.gap,
                tiler.screen_width,
                tiler.screen_height,
                true,
                conn
            ),
            Self::MasterRightGrid => Self::retile_with_master_grid(
                &mut tiler.windows,
                tiler.gap,
                tiler.screen_width,
                tiler.screen_height,
                false,
                conn
            ),
        }
    }
}

#[derive(Debug)]
pub struct Tiler<T: AbstractWindow> {
    pub windows: Vec<Window<T>>,
    pub floating_windows: Vec<Window<T>>,
    pub screen_width: u16,
    pub screen_height: u16,
    pub gap: u16,
    pub layout: Layout,
    pub is_showing: bool,
}

impl<T: AbstractWindow> Tiler<T> {
    pub fn new(width: u16, height: u16, gap: u16) -> Self {
        Self {
            windows: vec![],
            floating_windows: vec![],
            screen_width: width,
            screen_height: height,
            gap,
            layout: Layout::Grid,
            is_showing: false,
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
        for win in self.windows.iter_mut() {
            win.data.hide(conn);
        }
        for win in self.floating_windows.iter_mut() {
            win.data.hide(conn);
        }
    }

    pub fn toggle_layout(&mut self, conn: &Connection) {
        self.layout = match self.layout {
            Layout::Grid => Layout::MasterLeft,
            Layout::MasterLeft => Layout::MasterRight,
            Layout::MasterRight => Layout::MasterLeftGrid,
            Layout::MasterLeftGrid => Layout::MasterRightGrid,
            Layout::MasterRightGrid => Layout::Grid,
        };

        self.retile(conn);
    }

    pub fn spawn_window(&mut self, data: T, conn: &Connection) {
        self.windows.push(Window::new(data));
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

    pub fn remove_window(&mut self, window: &T, conn: &Connection) {
        for i in 0..self.windows.len() {
            let i = self.windows.len() - 1 - i;
            if self.windows[i].data == *window {
                self.windows.remove(i);
            }
        }

        for i in 0..self.floating_windows.len() {
            let i = self.floating_windows.len() - 1 - i;
            if self.floating_windows[i].data == *window {
                self.floating_windows.remove(i);
            }
        }

        self.retile(conn);
    }

    pub fn set_screen_size(&mut self, width: u16, height: u16, conn: &Connection) {
        self.screen_width = width;
        self.screen_height = height;
        
        self.retile(conn);
    }
}
