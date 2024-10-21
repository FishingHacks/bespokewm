use std::fmt::Debug;

use xcb::x::Rectangle;

use crate::{screen::Context, tiling::Layout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
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
impl Into<Rectangle> for Position {
    fn into(self) -> Rectangle {
        Rectangle {
            x: self.x as i16,
            y: self.y as i16,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Debug)]
pub struct Workspace {
    pub windows: Vec<usize>,
    floating_windows: Vec<usize>,
    pos: Position,
    gap: u16,
    layout: Layout,
    is_showing: bool,
    name: String,
    id: u32,
    focused: Option<(usize, bool)>,
}

impl Workspace {
    pub fn new(pos: Position, gap: u16, id: u32) -> Self {
        Self {
            windows: vec![],
            floating_windows: vec![],
            focused: None,
            pos,
            gap,
            layout: Layout::Grid,
            is_showing: false,
            name: format!("Desktop {id}"),
            id,
        }
    }

    fn retile(&mut self, context: &mut Context) {
        if self.windows.len() > 0 && self.is_showing {
            self.layout
                .retile(&self.windows, self.gap, self.pos, context);
        }
    }

    pub fn show(&mut self, ctx: &mut Context) {
        self.is_showing = true;

        for win in self.windows.iter().copied() {
            ctx.windows[win].show(&ctx.connection);
        }
        self.retile(ctx);

        for win in self.windows.iter().copied() {
            let win = &mut ctx.windows[win];
            win.show(&ctx.connection);
            win.update(win.width, win.height, win.x, win.y, &ctx.connection);
        }
    }

    pub fn hide(&mut self, ctx: &mut Context) {
        self.is_showing = false;
        self.unfocus_all(ctx);
        for win in self.windows.iter().copied() {
            ctx.windows[win].hide(&ctx.connection);
        }
        for win in self.floating_windows.iter().copied() {
            ctx.windows[win].hide(&ctx.connection);
        }
    }

    pub fn cycle_layout(&mut self, ctx: &mut Context) {
        self.layout = match self.layout {
            Layout::Grid => Layout::MasterLeft,
            Layout::MasterLeft => Layout::MasterRight,
            Layout::MasterRight => Layout::MasterLeftGrid,
            Layout::MasterLeftGrid => Layout::MasterRightGrid,
            Layout::MasterRightGrid => Layout::Monocle,
            Layout::Monocle => Layout::Grid,
        };

        self.retile(ctx);
    }

    pub fn set_layout(&mut self, new_layout: Layout, ctx: &mut Context) {
        if self.layout == new_layout {
            return;
        }
        self.layout = new_layout;

        self.retile(ctx);
    }

    pub fn spawn_window(&mut self, index: usize, ctx: &mut Context) {
        ctx.windows[index].show(&ctx.connection);
        self.windows.push(index);
        self.retile(ctx);
    }

    /// finds the window to toggle floating on. Usize is the window index and the boolean is if it is currently not floating
    fn find_floating_window(&mut self, window_idx: usize) -> Option<(usize, bool)> {
        for i in 0..self.windows.len() {
            if self.windows[i] == window_idx {
                return Some((i, true));
            }
        }
        for i in 0..self.floating_windows.len() {
            if self.floating_windows[i] == window_idx {
                return Some((i, false));
            }
        }
        None
    }

    pub fn toggle_floating(&mut self, window_idx: usize, ctx: &mut Context) {
        let Some((idx, enable)) = self.find_floating_window(window_idx) else {
            return;
        };
        if let Some((idx, _)) = self.focused {
            if idx == window_idx {
                self.focused = Some((idx, !enable));
            }
        }

        if enable {
            let val = self.windows.remove(idx);
            self.floating_windows.push(val);
        } else {
            let val = self.floating_windows.remove(idx);
            self.windows.push(val);
        }

        self.retile(ctx);
    }

    pub fn remove_window(&mut self, window_idx: usize, ctx: &mut Context) {
        self.unfocus(window_idx, ctx);

        let len = self.windows.len();
        for i in 0..self.windows.len() {
            let i = len - 1 - i;
            if self.windows[i] == window_idx {
                if let Some((idx, false)) = self.focused {
                    if idx > i {
                        self.focused = Some((idx - 1, false));
                    }
                }
                self.windows.remove(i);
            }
        }

        let len = self.floating_windows.len();
        for i in 0..self.floating_windows.len() {
            let i = len - 1 - i;
            if self.floating_windows[i] == window_idx {
                if let Some((idx, true)) = self.focused {
                    if idx > i {
                        self.focused = Some((idx - 1, true));
                    }
                }
                self.floating_windows.remove(i);
            }
        }

        self.retile(ctx);
    }

    pub fn set_screen_size(&mut self, width: u16, height: u16, ctx: &mut Context) {
        self.pos.width = width;
        self.pos.height = height;

        self.retile(ctx);
    }

    pub fn set_screen_offset(&mut self, offset_x: u16, offset_y: u16, ctx: &mut Context) {
        self.pos.x = offset_x;
        self.pos.y = offset_y;

        self.retile(ctx);
    }

    pub fn set_screen_position(&mut self, pos: Position, ctx: &mut Context) {
        self.pos = pos;

        self.retile(ctx);
    }

    pub fn get_screen_position(&self) -> Position {
        self.pos
    }

    pub fn windows<'a>(&'a self) -> impl Iterator<Item = usize> + 'a {
        self.windows
            .iter()
            .chain(self.floating_windows.iter())
            .copied()
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn get_window(&self, window_idx: usize) -> Option<(usize, bool)> {
        for idx in 0..self.windows.len() {
            if self.windows[idx] == window_idx {
                return Some((idx, false));
            }
        }
        for idx in 0..self.floating_windows.len() {
            if self.floating_windows[idx] == window_idx {
                return Some((idx, true));
            }
        }

        None
    }

    pub fn focus_client(&mut self, window_idx: usize, ctx: &mut Context) -> bool {
        if let Some((idx, is_floating)) = self.focused.take() {
            let window_idx = if is_floating {
                self.floating_windows[idx]
            } else {
                self.windows[idx]
            };
            ctx.windows[window_idx].unfocus(&ctx.connection);
        }
        self.focused = self.get_window(window_idx);

        if let Some((idx, is_floating)) = self.focused {
            let window_idx = if is_floating {
                self.floating_windows[idx]
            } else {
                self.windows[idx]
            };
            ctx.windows[window_idx].focus(&ctx.connection);
        }
        self.focused.is_some()
    }

    pub fn unfocus(&mut self, window_idx: usize, ctx: &mut Context) {
        if let Some((idx, is_floating)) = self.focused {
            let idx = if is_floating {
                self.floating_windows[idx]
            } else {
                self.windows[idx]
            };

            if idx != window_idx {
                return;
            }
            ctx.windows[window_idx].unfocus(&ctx.connection);
            self.focused = None;
        }
    }

    pub fn unfocus_all(&mut self, ctx: &mut Context) {
        if let Some((idx, is_floating)) = self.focused.take() {
            let window_idx = if is_floating {
                self.floating_windows[idx]
            } else {
                self.windows[idx]
            };
            ctx.windows[window_idx].unfocus(&ctx.connection);
        }
    }

    pub fn clear_windows(&mut self) {
        self.windows.clear();
        self.floating_windows.clear();
        self.focused = None;
    }

    pub(crate) fn window_amount(&self) -> usize {
        self.windows.len() + self.floating_windows.len()
    }
}
