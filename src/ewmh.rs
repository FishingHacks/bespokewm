use std::iter;

use xcb::{
    x::{
        Atom, ClientMessageData, ClientMessageEvent, DestroyWindow, EventMask, GetProperty,
        PropMode, SendEvent, Window, ATOM_ATOM, ATOM_CARDINAL, ATOM_STRING, ATOM_WINDOW,
    },
    Connection, Xid,
};

use crate::{atoms::Atoms, layout::Workspace, screen::Client};
type EwmhResult = anyhow::Result<(), xcb::ProtocolError>;

macro_rules! change_property {
    ($conn: expr, $window: expr, $mode: expr, $type: expr, $property: expr, $data: expr$(,)?) => {
        $conn.send_and_check_request(&xcb::x::ChangeProperty {
            window: $window,
            mode: $mode,
            r#type: $type,
            property: $property,
            data: $data,
        })
    };
}

pub fn set_number_of_desktops(
    new_amount: u32,
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_CARDINAL,
        atoms.net_number_of_desktops,
        &[new_amount]
    )
}

pub fn set_current_desktop(
    new_desktop: u32,
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_CARDINAL,
        atoms.net_current_desktop,
        &[new_desktop]
    )
}

pub fn set_desktop_names(
    workspaces: &[Workspace<Client>],
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_STRING,
        atoms.net_desktop_names,
        &workspaces
            .iter()
            .flat_map(|workspace| {
                workspace
                    .name()
                    .as_bytes()
                    .iter()
                    .copied()
                    .chain(iter::once(0u8))
            })
            .collect::<Vec<_>>(),
    )
}

pub fn set_desktop_viewport(
    x: u32,
    y: u32,
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_CARDINAL,
        atoms.net_desktop_viewport,
        &[x, y]
    )
}

/// updates _NET_WM_DESKTOP for all clients on all workspaces for the
/// current screen
pub fn set_wm_desktop(
    workspaces: &[Workspace<Client>],
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    for workspace in workspaces.iter() {
        for client in workspace.windows() {
            change_property!(
                conn,
                client.window,
                PropMode::Replace,
                ATOM_CARDINAL,
                atoms.net_wm_desktop,
                &[workspace.id()]
            )?;
        }
    }
    Ok(())
}

/// list all the clients currently managed by the window manager
/// by order of insertion
pub fn set_client_list<'a>(
    clients: impl IntoIterator<Item = &'a Window>,
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_WINDOW,
        atoms.net_client_list,
        &clients.into_iter().copied().collect::<Vec<_>>()
    )
}

/// list all the clients currently managed by the window manager
/// by stacking order, since we dont stack windows, this is the same
/// as the other list
pub fn set_client_list_stacking<'a>(
    clients: impl IntoIterator<Item = &'a Window>,
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_WINDOW,
        atoms.net_client_list_stacking,
        &clients.into_iter().copied().collect::<Vec<_>>()
    )
}

/// set desktop is a mode where the window manager is solely displaying
/// the background while hiding every other window
/// this never applies to us
pub fn set_showing_desktop(
    is_showing: bool,
    root: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_CARDINAL,
        atoms.net_showing_desktop,
        &[if is_showing { 1u32 } else { 0u32 }],
    )
}

pub fn window_supports(
    requested_atom: Atom,
    window: Window,
    atoms: &Atoms,
    conn: &Connection,
) -> bool {
    let Ok(cookie) = conn.wait_for_reply(conn.send_request(&GetProperty {
        delete: false,
        long_offset: 0,
        long_length: 4096,
        property: atoms.wm_protocols,
        r#type: ATOM_ATOM,
        window,
    })) else {
        return false;
    };

    cookie
        .value::<Atom>()
        .iter()
        .any(|&atom| atom == requested_atom)
}

pub fn delete_window(window: Window, atoms: &Atoms, conn: &Connection) -> bool {
    if window_supports(atoms.wm_delete_window, window, atoms, conn) {
        let event = ClientMessageEvent::new(
            window,
            atoms.wm_protocols,
            ClientMessageData::Data32([
                atoms.wm_delete_window.resource_id(),
                xcb::x::CURRENT_TIME,
                0,
                0,
                0,
            ]),
        );
        if let Err(_) = conn.send_and_check_request(&SendEvent {
            destination: xcb::x::SendEventDest::Window(window),
            event: &event,
            propagate: false,
            event_mask: EventMask::NO_EVENT,
        }) {
            // destroy window if we cant inform that it has to destroy itself
            _ = conn.send_and_check_request(&DestroyWindow { window });
            true
        } else {
            false
        }
    } else {
        _ = conn.send_and_check_request(&DestroyWindow { window });
        true
    }
}
