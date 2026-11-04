use super::*;

#[test]
fn waiting_for_changed_geometry_does_not_rewrite_captured_mouse_coordinates() {
    for fenced in [false, true] {
        let (client, mut server, observer) = peer();
        let mut surface = large_surface();
        surface.surface_revision = 2;
        surface.panes[0].rect.width = 64;
        surface.panes[0].inner_rect.width = 64;
        let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
        let mut click = wheel();
        if let ClientPaneInputEvent::Mouse { kind, position, .. } = &mut click {
            *kind = ClientMouseKind::Down(ClientMouseButton::Left);
            *position = ClientMousePosition::Cell {
                column: 100,
                row: 40,
            };
        }
        if fenced {
            client.handle.set_focus("boot-v1", false).unwrap();
        }
        for event in [click.clone(), wheel()] {
            client
                .handle
                .send_input("boot-v1", "w1:p1", [event])
                .unwrap();
        }
        if fenced {
            quiet(&mut server);
            server.write_all(&tail).unwrap();
            assert!(matches!(event(&client), ClientEvent::Surface(_)));
            assert_eq!(
                receive(&mut server),
                ClientMessage::ClientShellFocus { focused: false }
            );
        }
        for expected in [click, wheel()] {
            assert_eq!(receive(&mut server), pane_input(vec![expected]));
        }
        if !fenced {
            assert!(client.events.try_recv().is_err());
            server.write_all(&tail).unwrap();
            assert!(matches!(event(&client), ClientEvent::Surface(_)));
        }
        close(client, server, observer);
    }
}

#[test]
fn pending_api_and_malformed_tail_keep_mouse_order_and_disconnect_policy() {
    for malformed in [false, true] {
        let (client, mut server, observer) = peer();
        if !malformed {
            client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
            assert!(matches!(
                receive(&mut server),
                ClientMessage::ClientShellEndpointRequest { .. }
            ));
        }
        let mut surface = large_surface();
        surface.surface_revision = 2;
        if malformed {
            surface.frame.cells.pop();
        }
        let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
        client
            .handle
            .send_input("boot-v1", "w1:p1", [wheel()])
            .unwrap();
        if malformed {
            assert_eq!(receive(&mut server), pane_input(vec![wheel()]));
        } else {
            quiet(&mut server);
        }
        server.write_all(&tail).unwrap();
        if malformed {
            let _ = client.handle.send_input("boot-v1", "w1:p1", [key()]);
            assert!(matches!(event(&client), ClientEvent::Disconnected { .. }));
            drop(observer);
            assert_eq!(server.read(&mut [0]).unwrap(), 0);
        } else {
            assert!(matches!(event(&client), ClientEvent::Surface(_)));
            assert_eq!(receive(&mut server), pane_input(vec![wheel()]));
            close(client, server, observer);
        }
    }
}
