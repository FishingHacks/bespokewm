use std::fmt::Display;

use crate::{layout::Position, screen::Context};

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
    fn retile_grid(windows: &[usize], gap: u16, screen_position: Position, conn: &mut Context) {
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
            conn.windows[windows[i]].update(
                win_width - gap,
                win_height - gap,
                x,
                y,
                &conn.connection,
            );
        }
    }

    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_with_master(
        windows: &[usize],
        gap: u16,
        screen_position: Position,
        master_is_left: bool,
        conn: &mut Context,
    ) {
        let half_gap = gap / 2;
        let half_width = screen_position.width / 2;

        // we do -1 because that later excludes the last element and is the last element
        let len = windows.len() - 1;
        conn.windows[windows[len]].update(
            half_width - gap,
            screen_position.height - gap,
            if master_is_left {
                half_gap
            } else {
                half_width + half_gap
            } + screen_position.x,
            half_gap + screen_position.y,
            &conn.connection,
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
            conn.windows[windows[len - 1 - i]].update(
                width,
                height,
                x,
                i as u16 * height_gapless + half_gap + screen_position.y,
                &conn.connection,
            );
        }
    }

    /// ASSUMPTIONS: windows.len() >= 1
    fn retile_with_master_grid(
        windows: &[usize],
        gap: u16,
        screen_position: Position,
        master_is_left: bool,
        conn: &mut Context,
    ) {
        let half_gap = gap / 2;
        let half_width = screen_position.width / 2;

        // we do -1 because that later excludes the last element and is the last element
        let len = windows.len() - 1;
        conn.windows[windows[len]].update(
            half_width - gap,
            screen_position.height - gap,
            if master_is_left {
                half_gap
            } else {
                half_width + half_gap
            } + screen_position.x,
            half_gap + screen_position.y,
            &conn.connection,
        );

        if master_is_left {
            Self::retile_grid(
                &windows[0..len],
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
                &windows[0..len],
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

    fn retile_monocle(windows: &[usize], gap: u16, screen_position: Position, conn: &mut Context) {
        let len = windows.len() - 1;

        let x = screen_position.x + gap / 2;
        let y = screen_position.y + gap / 2;

        for window in windows[..len].iter().copied() {
            conn.windows[window].update(30, 30, x, y, &conn.connection);
        }

        conn.windows[windows[len]].update(
            screen_position.width - gap,
            screen_position.height - gap,
            x,
            y,
            &conn.connection,
        );
    }

    pub fn retile(self, windows: &[usize], gap: u16, pos: Position, ctx: &mut Context) {
        if windows.len() < 1 {
            return;
        } else if windows.len() == 1 {
            // the window is always gonna be the entire window
            ctx.windows[windows[0]].update(
                pos.width - gap,
                pos.height - gap,
                gap / 2 + pos.x,
                gap / 2 + pos.y,
                &ctx.connection,
            );

            return;
        }

        match self {
            Self::Grid => Self::retile_grid(&windows, gap, pos, ctx),
            Self::MasterLeft => Self::retile_with_master(&windows, gap, pos, true, ctx),
            Self::MasterRight => Self::retile_with_master(&windows, gap, pos, false, ctx),
            Self::MasterLeftGrid => Self::retile_with_master_grid(&windows, gap, pos, true, ctx),
            Self::MasterRightGrid => Self::retile_with_master_grid(&windows, gap, pos, false, ctx),
            Self::Monocle => Self::retile_monocle(&windows, gap, pos, ctx),
        }
    }
}
