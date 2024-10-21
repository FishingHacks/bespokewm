use std::{cell::Cell, sync::Arc};

use xcb::{
    x::{
        ChangeGc, CloseFont, CopyArea, CreateGc, CreatePixmap, Font, FreeGc, FreePixmap, Gc,
        Gcontext, ImageText8, OpenFont, Pixmap, PolyFillRectangle, Window,
    },
    Connection, ProtocolError,
};

use crate::layout::Position;

pub struct DrawContext {
    window: Window,
    pos: Position,
    pixmap: Pixmap,
    graphic_context: Gcontext,
    conn: Arc<Connection>,
    last_color: Cell<(u32, u32)>,
    depth: u8,
    font: Option<Font>,
}

impl DrawContext {
    pub fn new(
        window: Window,
        pos: Position,
        conn: Arc<Connection>,
        depth: u8,
    ) -> anyhow::Result<Self, ProtocolError> {
        let pixmap = conn.generate_id();
        let graphic_context = conn.generate_id();

        conn.send_and_check_request(&CreatePixmap {
            drawable: xcb::x::Drawable::Window(window),
            depth,
            width: pos.width,
            height: pos.height,
            pid: pixmap,
        })?;
        conn.send_and_check_request(&CreateGc {
            cid: graphic_context,
            drawable: xcb::x::Drawable::Pixmap(pixmap),
            value_list: &[
                Gc::Foreground(0),
                Gc::Background(0),
                Gc::LineStyle(xcb::x::LineStyle::Solid),
                Gc::CapStyle(xcb::x::CapStyle::Butt),
                Gc::JoinStyle(xcb::x::JoinStyle::Miter),
            ],
        })?;

        Ok(Self {
            conn,
            graphic_context,
            pixmap,
            pos,
            window,
            depth,
            last_color: Cell::new((0, 0)),
            font: None,
        })
    }

    pub fn open_font(&mut self, font_name: &str) -> Result<(), ProtocolError> {
        if let Some(font) = self.font {
            self.conn.send_and_check_request(&CloseFont { font })?;
            self.font = None;
        }

        let font = self.conn.generate_id();
        self.conn.send_and_check_request(&OpenFont {
            fid: font,
            name: font_name.as_bytes(),
        })?;
        if let Err(e) = self.conn.send_and_check_request(&ChangeGc {
            gc: self.graphic_context,
            value_list: &[Gc::Font(font)],
        }) {
            _ = self.conn.send_and_check_request(&CloseFont { font });

            return Err(e);
        }
        self.font = Some(font);

        Ok(())
    }

    pub fn draw_rect(&self, mut pos: Position, fg: u32, bg: u32) -> anyhow::Result<()> {
        if pos.x >= self.pos.width || pos.y >= self.pos.height {
            anyhow::bail!("Tried drawing outside of the rectt");
        }
        if pos.x + pos.width > self.pos.width {
            pos.width = self.pos.width - pos.x;
        }
        if pos.y + pos.height > self.pos.height {
            pos.height = self.pos.height - pos.y;
        }

        if self.last_color.get() != (fg, bg) {
            self.conn.send_and_check_request(&ChangeGc {
                gc: self.graphic_context,
                value_list: &[Gc::Foreground(fg), Gc::Background(bg)],
            })?;
            self.last_color.set((fg, bg));
        }

        self.conn.send_and_check_request(&PolyFillRectangle {
            drawable: xcb::x::Drawable::Pixmap(self.pixmap),
            gc: self.graphic_context,
            rectangles: &[pos.into()],
        })?;
        Ok(())
    }

    pub fn draw_string(
        &self,
        x: i16,
        y: i16,
        string: &str,
        fg: u32,
        bg: u32,
    ) -> Result<(), ProtocolError> {
        if self.last_color.get() != (fg, bg) {
            self.conn.send_and_check_request(&ChangeGc {
                gc: self.graphic_context,
                value_list: &[Gc::Foreground(fg), Gc::Background(bg)],
            })?;
            self.last_color.set((fg, bg));
        }

        self.conn.send_and_check_request(&ImageText8 {
            drawable: xcb::x::Drawable::Pixmap(self.pixmap),
            gc: self.graphic_context,
            string: string.as_bytes(),
            x,
            y,
        })
    }

    pub fn finalise(&mut self) -> anyhow::Result<(), ProtocolError> {
        self.conn.send_and_check_request(&CopyArea {
            gc: self.graphic_context,
            width: self.pos.width,
            height: self.pos.height,
            dst_drawable: xcb::x::Drawable::Window(self.window),
            dst_x: self.pos.x as i16,
            dst_y: self.pos.y as i16,
            src_drawable: xcb::x::Drawable::Pixmap(self.pixmap),
            src_x: 0,
            src_y: 0,
        })
    }

    pub fn resize(mut self, new_pos: Position) -> Result<Self, ProtocolError> {
        let new_pixmap = self.conn.generate_id();
        let new_graphic_context = self.conn.generate_id();

        let destroy_pixmap_cookie = self.conn.send_request_checked(&FreePixmap {
            pixmap: self.pixmap,
        });
        let destroy_gc_cookie = self.conn.send_request_checked(&FreeGc {
            gc: self.graphic_context,
        });

        let create_pixmap_cookie = self.conn.send_request_checked(&CreatePixmap {
            depth: self.depth,
            drawable: xcb::x::Drawable::Window(self.window),
            width: new_pos.width,
            height: new_pos.height,
            pid: new_pixmap,
        });
        self.conn.check_request(destroy_pixmap_cookie)?;
        self.conn.check_request(destroy_gc_cookie)?;
        self.conn.check_request(create_pixmap_cookie)?;
        self.conn.send_and_check_request(&CreateGc {
            drawable: xcb::x::Drawable::Pixmap(new_pixmap),
            cid: new_graphic_context,
            value_list: &[
                Gc::Foreground(self.last_color.get().0),
                Gc::Background(self.last_color.get().1),
                Gc::LineStyle(xcb::x::LineStyle::Solid),
                Gc::CapStyle(xcb::x::CapStyle::Butt),
                Gc::JoinStyle(xcb::x::JoinStyle::Miter),
            ],
        })?;

        self.pixmap = new_pixmap;
        self.graphic_context = new_graphic_context;
        self.pos = new_pos;

        Ok(self)
    }
}

impl Drop for DrawContext {
    fn drop(&mut self) {
        let mut cookies = Vec::with_capacity(3);

        cookies.push(self.conn.send_request_checked(&FreePixmap {
            pixmap: self.pixmap,
        }));
        cookies.push(self.conn.send_request_checked(&FreeGc {
            gc: self.graphic_context,
        }));

        if let Some(font) = self.font {
            cookies.push(self.conn.send_request_checked(&CloseFont { font }));
        }

        for cookie in cookies {
            _ = self.conn.check_request(cookie);
        }
    }
}
