use std::iter;

use xcb::{
    x::{PropMode, Window, ATOM_CARDINAL, ATOM_STRING, ATOM_WINDOW},
    Connection,
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
pub fn set_showing_desktop(is_showing: bool, root: Window, atoms: &Atoms, conn: &Connection) -> EwmhResult {
    change_property!(
        conn,
        root,
        PropMode::Replace,
        ATOM_CARDINAL,
        atoms.net_showing_desktop,
        &[if is_showing { 1u32 } else { 0u32 }],
    )
}