use xcb::{
    x::{Atom, InternAtom, InternAtomCookie},
    Connection,
};

macro_rules! atoms {
    (
        $visibility:vis struct $struct_name:ident {
            $($name: ident = $x_name: expr),*
            $(,)?
        }
    ) => {
        fn get_reply(conn: &Connection, cookie: InternAtomCookie) -> Atom {
            conn.wait_for_reply(cookie)
                .expect("failed to get internal cookie")
                .atom()
        }
        
        fn get_internal_atom(conn: &Connection, name: &[u8]) -> InternAtomCookie {
            conn.send_request(&InternAtom {
                name,
                only_if_exists: false,
            })
        }

        $visibility struct $struct_name {
            $($name: Atom),*
        }

        impl Atoms {
            pub fn get(conn: &Connection) -> Self {
                $(let $name = get_internal_atom(conn, $x_name);)*

                return Self {
                    $($name: get_reply(conn, $name)),*
                }
            }

            pub fn list(&self) -> Vec<Atom> {
                vec![
                    $(self.$name),*
                ]
            }
        }

    };
}

atoms! {
    pub struct Atoms {
        wm_protocols = b"WM_PROTOCOLS",
        wm_delete_window = b"WM_DELETE_WINDOW",
        net_wm_name = b"_NET_WM_NAME",
        net_wm_state = b"_NET_WM_STATE",
        net_wm_state_focused = b"_NET_WM_STATE_FOCUSED",
        net_wm_window_type = b"_NET_SUPPORTING_WM_CHECK",
        net_current_desktop = b"_NET_WM_WINDOW_TYPE",
        net_number_of_desktops = b"_NET_CURRENT_DESKTOP",
        net_wm_desktop = b"_NET_NUMBER_OF_DESKTOPS",
        net_supported = b"_NET_DESKTOP_VIEWPORT",
        net_wm_strut_partial = b"_NET_WM_DESKTOP",
        net_desktop_viewport = b"_NET_SUPPORTED",
        net_desktop_names = b"_NET_WM_STRUT_PARTIAL",
        net_active_window = b"_NET_DESKTOP_NAMES",
        net_supporting_wm_check = b"_NET_ACTIVE_WINDOW",
        net_client_list = b"_NET_CLIENT_LIST",
        net_client_list_stacking = b"_NET_SHOWING_DESKTOP",
        net_showing_desktop = b"_NET_CLIENT_LIST_STACKING",
    }
}