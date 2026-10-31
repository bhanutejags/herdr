use super::*;
use crate::protocol::endpoint::EndpointStatusDetails;

fn status_state() -> ClientShellState {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.mobile_width_threshold = 0;
    config.hide_tab_bar_when_single_tab = false;
    config.tab_bar_position = TabBarPositionConfig::Top;
    config.mouse_capture = true;
    let mut state = ClientShellState::new(config);
    let mut projected = snapshot();
    projected.tab_bar_right = ["界", "🌞 +44°F"]
        .into_iter()
        .map(|text| crate::protocol::ClientShellTabStatusSegment {
            text: text.into(),
            accent: false,
        })
        .collect();
    projected.tab_bar_right_separator = " · ".into();
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.set_endpoint_status_details((&ClientEndpointId::Local, 1), details());
    state.compose(106, 20).unwrap();
    state
}

fn details() -> EndpointStatusDetails {
    EndpointStatusDetails {
        boot_id: "boot-1".into(),
        revision: 1,
        tooltips: vec![
            None,
            Some("Seattle\nFeels like: 42°F\nHumidity: 70%".into()),
        ],
    }
}

fn move_to(state: &mut ClientShellState, point: (u16, u16)) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: point.0,
        row: point.1,
        modifiers: KeyModifiers::NONE,
    })])
}

fn hover(state: &mut ClientShellState) -> ClientShellInput {
    let rect = state.hits.status_segments[1].0;
    move_to(state, (rect.right() - 1, rect.y))
}

fn hover_mouse(rect: Rect) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Moved,
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn status_hover_opening_completes_and_stops_scheduling_repaints() {
    let mut state = status_state();
    let now = std::time::Instant::now();
    let ms = std::time::Duration::from_millis;
    let mouse = hover_mouse(state.hits.status_segments[1].0);
    assert!(state.update_status_hover(mouse, now));
    assert_eq!(state.timer_delay(now), ms(16));
    assert!(!state.tick_status_hover(now + ms(15)));
    assert!(state.tick_status_hover(now + ms(16)));
    assert_eq!(state.status_hover_deadline(), Some(now + ms(32)));
    assert!(state.tick_status_hover(now + ms(150)));
    assert!(state.status_hover_deadline().is_none());
    assert_eq!(state.timer_delay(now + ms(150)), ms(100));
    for elapsed in [150, 151, 1_000] {
        assert!(!state.tick_status_hover(now + ms(elapsed)));
    }
    assert!(!state.update_status_hover(mouse, now + ms(1_001)));
    assert!(state.status_hover_deadline().is_none());
}

#[test]
fn status_hover_opening_keeps_its_deadline_on_same_segment_moves() {
    let mut state = status_state();
    let now = std::time::Instant::now();
    let ms = std::time::Duration::from_millis;
    let mut mouse = hover_mouse(state.hits.status_segments[1].0);
    state.update_status_hover(mouse, now);
    assert!(state.tick_status_hover(now + ms(64)));
    mouse.column += 1;
    assert!(state.update_status_hover(mouse, now + ms(70)));
    assert_eq!(state.status_hover_deadline(), Some(now + ms(80)));
    state.compose(106, 20).unwrap();
    let mut projection = details();
    projection.tooltips[1] = Some("Plain text detail".into());
    state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection);
    assert_eq!(state.status_hover_deadline(), Some(now + ms(80)));
}

#[test]
fn status_hover_opening_restarts_after_dismissal_or_target_change() {
    let mut state = status_state();
    let now = std::time::Instant::now();
    let ms = std::time::Duration::from_millis;
    let mut projection = details();
    projection.tooltips[0] = Some("Other detail".into());
    state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection);
    let mouse = hover_mouse(state.hits.status_segments[1].0);
    state.update_status_hover(mouse, now);
    state.tick_status_hover(now + ms(150));
    assert!(state.clear_status_hover());
    assert!(state.status_hover_deadline().is_none());
    assert!(!state.tick_status_hover(now + ms(200)));
    assert!(state.update_status_hover(mouse, now + ms(200)));
    assert_eq!(state.status_hover_deadline(), Some(now + ms(216)));
    let other = hover_mouse(state.hits.status_segments[0].0);
    assert!(state.update_status_hover(other, now + ms(210)));
    assert_eq!(state.status_hover_deadline(), Some(now + ms(226)));
    assert!(!state.tick_status_hover(now + ms(225)));
    assert!(state.tick_status_hover(now + ms(360)));
    assert!(state.status_hover_deadline().is_none());
}

#[test]
fn status_hover_survives_stationary_clock_and_unrelated_snapshot_updates() {
    let mut state = status_state();
    hover(&mut state);
    let opening_deadline = state.status_hover_deadline();
    for (revision, clock) in [(2, "12:34"), (3, "12:34:56")] {
        let previous_text = state.status_hover_text().unwrap().1.to_owned();
        let mut projection = details();
        projection.revision = revision;
        let tooltip = format!("Seattle\nClock revision: {revision}");
        projection.tooltips[1] = Some(tooltip.clone());
        assert!(state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection));
        assert_eq!(state.status_hover_text().unwrap().1, previous_text);
        let rows = frame_rows(&state.compose(106, 20).unwrap()).join("\n");
        for line in previous_text.lines() {
            assert!(rows.contains(line));
        }
        assert_eq!(state.status_hover_text().unwrap().1, previous_text);
        let mut next = state.snapshot.as_deref().unwrap().clone();
        next.revision = revision;
        next.tab_bar_right[0].text = clock.into();
        next.workspaces[0].label = format!("workspace-{revision}");
        state.set_snapshot(Box::new(next));
        assert!(state.compose(106, 20).is_none());
        assert!(state.status_hover.is_some());
        let mut next = surface();
        next.projection_revision = revision;
        state.set_pane_surface(next);
        let rows = frame_rows(&state.compose(106, 20).unwrap()).join("\n");
        assert!(rows.contains(&format!("Clock revision: {revision}")));
        assert!(rows.contains(clock));
        assert_eq!(state.status_hover_text().unwrap().1, tooltip);
        assert_eq!(state.status_hover_deadline(), opening_deadline);
    }
}

#[test]
fn status_hover_survives_controls_advancing_ahead_of_pane_surfaces() {
    let mut state = status_state();
    hover(&mut state);
    for revision in 2..=3 {
        let mut projection = details();
        projection.revision = revision;
        projection.tooltips[1] = Some(format!("Seattle\nRevision: {revision}"));
        state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection);
        assert!(state.status_hover.is_some());
        let mut next = state.snapshot.as_deref().unwrap().clone();
        next.revision = revision;
        next.tab_bar_right[0].text = format!("12:34:0{revision}");
        state.set_snapshot(Box::new(next));
        assert!(state.compose(106, 20).is_none());
    }
    let mut next = surface();
    next.projection_revision = 3;
    state.set_pane_surface(next);
    let rows = frame_rows(&state.compose(106, 20).unwrap()).join("\n");
    assert!(rows.contains("Revision: 3"));
    assert_eq!(state.status_hover_text().unwrap().1, "Seattle\nRevision: 3");
}

#[test]
fn status_hover_bottom_bar_card_keeps_the_hovered_segment_visible() {
    let mut state = status_state();
    state.config.tab_bar_position = TabBarPositionConfig::Bottom;
    let original = frame_rows(&state.compose(106, 20).unwrap());
    let anchor = state.hits.status_segments[1].0;
    hover(&mut state);
    let rows = frame_rows(&state.compose(106, 20).unwrap());
    assert_eq!(rows[usize::from(anchor.y)], original[usize::from(anchor.y)]);
    assert!(rows[..usize::from(anchor.y)].join("\n").contains("Seattle"));
    assert!(state.status_hover.is_some());
}

#[test]
fn status_hover_refreshes_details_without_pointer_motion() {
    let mut state = status_state();
    hover(&mut state);
    let mut projection = details();
    projection.tooltips[1] = Some("Cloudy".into());
    assert!(state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection));
    let rows = frame_rows(&state.compose(106, 20).unwrap()).join("\n");
    assert!(rows.contains("Cloudy"));
    assert!(!rows.contains("Seattle"));
    assert_eq!(state.status_hover_text().unwrap().1, "Cloudy");
    let mut invalid_mapping = details();
    invalid_mapping.tooltips.remove(0);
    assert!(state.set_endpoint_status_details((&ClientEndpointId::Local, 1), invalid_mapping));
    assert!(state.status_hover.is_none());
    assert!(!frame_rows(&state.compose(106, 20).unwrap())
        .join("\n")
        .contains("Cloudy"));
}

#[test]
fn status_hover_does_not_retarget_replaced_removed_or_reordered_segments() {
    for change in 0..7 {
        let mut state = status_state();
        hover(&mut state);
        let mut next = state.snapshot.as_deref().unwrap().clone();
        next.revision = 2;
        let mut projection = details();
        projection.revision = 2;
        match change {
            0 => next.tab_bar_right[1].text = "replacement".into(),
            1 => {
                next.tab_bar_right.swap(0, 1);
                projection.tooltips.swap(0, 1);
            }
            2 => {
                next.tab_bar_right.remove(0);
                projection.tooltips.remove(0);
            }
            3 => {
                next.tab_bar_right.remove(1);
                projection.tooltips.remove(1);
            }
            4 => {
                next.tab_bar_right
                    .push(crate::protocol::ClientShellTabStatusSegment {
                        text: "another long item".into(),
                        accent: false,
                    });
                projection.tooltips.push(Some("Not the weather".into()));
            }
            5 => projection.tooltips.clear(),
            _ => projection.tooltips[1] = None,
        }
        state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection);
        state.set_snapshot(Box::new(next));
        let mut next = surface();
        next.projection_revision = 2;
        state.set_pane_surface(next);
        let frame = state.compose(106, 20).unwrap();
        assert!(state.status_hover.is_none(), "change={change}");
        assert!(
            !frame_rows(&frame).join("\n").contains("Seattle"),
            "change={change}"
        );
    }
}

#[test]
fn status_hover_clears_when_boot_or_connection_generation_changes() {
    for reboot in [false, true] {
        let mut state = status_state();
        state.active_snapshot_generation = Some(1);
        state.pane_surface_generation = Some(1);
        hover(&mut state);
        let mut next = state.snapshot.as_deref().unwrap().clone();
        let generation = if reboot {
            next.boot_id = "boot-2".into();
            1
        } else {
            2
        };
        let mut projection = details();
        projection.boot_id.clone_from(&next.boot_id);
        state.set_endpoint_status_details((&ClientEndpointId::Local, generation), projection);
        state.set_endpoint_snapshot_for_generation(
            &ClientEndpointId::Local,
            generation,
            Box::new(next),
        );
        assert!(state.status_hover.is_none());
    }
}

#[test]
fn status_hover_is_dismissed_by_paste_and_committed_text_without_losing_input() {
    for input in 0..4 {
        let mut state = status_state();
        hover(&mut state);
        state.compose(106, 20).unwrap();
        let outcome = match input {
            0 => state.handle_raw_events(vec![RawInputEvent::Paste("paste".into())]),
            1 => state.handle_input_bytes(b"\x1b[200~paste\x1b[201~"),
            2 => state.handle_raw_events(vec![RawInputEvent::Text(crate::input::TextCommit::new(
                "你",
            ))]),
            _ => {
                state.set_host_reports_key_releases(true);
                state.set_host_reports_all_keys(true);
                state.handle_input_bytes("你".as_bytes())
            }
        };
        assert!(outcome.repaint, "input={input}");
        assert!(state.status_hover.is_none(), "input={input}");
        assert!(outcome.actions.is_empty());
        assert!(
            matches!(&outcome.requests[..], [ClientMessage::ClientShellPaneInput { events, .. }]
            if match (&events[..], input) {
                ([crate::protocol::ClientPaneInputEvent::Paste(text)], 0 | 1) => text == "paste",
                ([crate::protocol::ClientPaneInputEvent::TextCommit(text)], 2 | 3) => text == "你",
                _ => false,
            }),
            "input={input}"
        );
        assert!(!frame_rows(&state.compose(106, 20).unwrap())
            .join("\n")
            .contains("Seattle"));
    }
}

#[test]
fn status_hover_hit_testing_uses_unicode_widths_and_excludes_separators() {
    let mut state = status_state();
    let first = state.hits.status_segments[0].0;
    let last = state.hits.status_segments[1].0;
    assert_eq!(first.width, 2);
    assert_eq!(last.width, render::display_width("🌞 +44°F"));
    assert_eq!(last.x - first.right(), 3);
    let outcome = hover(&mut state);
    assert!(outcome.repaint);
    assert!(outcome.actions.is_empty() && outcome.requests.is_empty());
    let frame = state.compose(106, 20).unwrap();
    let rows = frame_rows(&frame).join("\n");
    assert!(rows.contains("Seattle"));
    assert!(rows.contains("Feels like: 42°F"));
    assert!(rows.contains("Humidity: 70%"));
    assert!(!hover(&mut state).repaint);
    assert!(move_to(&mut state, (first.right(), first.y)).repaint);
    assert!(state.status_hover.is_none());
    let frame = state.compose(106, 20).unwrap();
    assert!(!frame_rows(&frame).join("\n").contains("Seattle"));
    assert!(!move_to(&mut state, (first.x, first.y)).repaint);
}

#[test]
fn status_hover_clears_on_focus_drag_overlay_popup_mode_and_snapshot_changes() {
    for change in 0..7 {
        let mut state = status_state();
        hover(&mut state);
        assert!(state.status_hover.is_some());
        match change {
            0 => {
                state.handle_raw_events(vec![RawInputEvent::OuterFocusLost]);
            }
            1 => state.chrome_drag = Some(ClientChromeDrag::SidebarWidth),
            2 => state.overlay = Some(ClientShellOverlay::Onboarding),
            3 => state.set_pane_surface(surface_with_popup()),
            4 => state.mode = ClientShellMode::Prefix,
            5 => {
                let mut next = state.snapshot.as_deref().unwrap().clone();
                next.revision += 1;
                state.set_snapshot(Box::new(next));
                let mut next = surface();
                next.projection_revision += 1;
                state.set_pane_surface(next);
            }
            _ => state
                .set_endpoint_status(&ClientEndpointId::Local, ClientEndpointStatus::Reconnecting),
        }
        let frame = state.compose(106, 20).unwrap();
        assert!(state.status_hover.is_none(), "change={change}");
        assert!(!frame_rows(&frame).join("\n").contains("Seattle"));
    }
}

#[test]
fn status_hover_requires_matching_optional_companion_identity_and_positions() {
    for mismatch in 0..6 {
        let mut state = status_state();
        let mut projection = details();
        let mut generation = 1;
        state.active_snapshot_generation = Some(1);
        match mismatch {
            0 => {
                state.endpoints[0].status_details = None;
            }
            1 => projection.boot_id = "old-boot".into(),
            2 => projection.revision += 1,
            3 => generation = 2,
            4 => {
                projection.tooltips.remove(0);
            }
            _ => projection.tooltips[1] = Some("\n  ".into()),
        }
        if mismatch != 0 {
            state.set_endpoint_status_details((&ClientEndpointId::Local, generation), projection);
        }
        assert!(!hover(&mut state).repaint, "mismatch={mismatch}");
        assert!(state.status_hover.is_none());
    }
    let mut state = status_state();
    let mut projection = details();
    projection.revision = 2;
    projection.tooltips[1] = Some("Cloudy".into());
    state.set_endpoint_status_details((&ClientEndpointId::Local, 1), projection);
    let mut next = state.snapshot.as_deref().unwrap().clone();
    next.revision = 2;
    state.set_snapshot(Box::new(next));
    let mut next = surface();
    next.projection_revision = 2;
    state.set_pane_surface(next);
    state.compose(106, 20).unwrap();
    assert!(hover(&mut state).repaint);
    assert!(frame_rows(&state.compose(106, 20).unwrap())
        .join("\n")
        .contains("Cloudy"));
}

#[test]
fn status_hover_does_not_follow_a_machine_switch_with_identical_snapshot_ids() {
    let mut state = status_state();
    hover(&mut state);
    let profile = SavedSshEndpoint {
        id: crate::client::endpoint::ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Remote".into(),
        target: "host".into(),
        session: "default".into(),
        enabled: true,
    };
    let endpoint_id = ClientEndpointId::Ssh(profile.id.clone());
    let projected = state.snapshot.as_ref().unwrap().clone();
    state.set_endpoint_catalog(&[profile]);
    state.set_endpoint_status(&endpoint_id, ClientEndpointStatus::Online);
    state.set_endpoint_snapshot(&endpoint_id, projected);
    assert!(state.activate_endpoint_projection(&endpoint_id));
    assert!(state.status_hover.is_none());
    state.set_pane_surface(surface());
    state.compose(106, 20).unwrap();
    assert!(!hover(&mut state).repaint);
    state.set_endpoint_status_details((&endpoint_id, 1), details());
    assert!(hover(&mut state).repaint);
}

#[test]
fn status_hover_has_no_hits_in_narrow_hidden_or_keyboard_only_chrome() {
    for hidden in 0..3 {
        let mut state = status_state();
        hover(&mut state);
        match hidden {
            0 => {
                state.compose(24, 10).unwrap();
            }
            1 => {
                state.config.hide_tab_bar_when_single_tab = true;
                state.compose(106, 20).unwrap();
            }
            _ => {
                state.config.mouse_capture = false;
                state.compose(106, 20).unwrap();
            }
        }
        assert!(state.hits.status_segments.is_empty(), "hidden={hidden}");
        assert!(state.status_hover.is_none());
    }
}

#[test]
fn status_hover_blocks_surface_fast_path_and_dismissal_restores_the_pane() {
    let mut state = status_state();
    hover(&mut state);
    state.compose(106, 20).unwrap();
    let mut pane = surface().panes.remove(0);
    pane.content_revision = 2;
    let patch = crate::protocol::PaneSurfacePatch {
        boot_id: "boot-1".into(),
        projection_revision: 1,
        base_surface_revision: 1,
        surface_revision: 2,
        rows: vec![crate::protocol::PaneSurfacePatchRow {
            x: 0,
            y: 0,
            cells: vec![crate::protocol::CellData {
                symbol: "X".into(),
                ..surface().frame.cells[0].clone()
            }],
        }],
        panes: vec![pane],
        cursor: None,
    };
    assert!(matches!(
        state.apply_pane_surface_patch(patch.clone()),
        ClientPaneSurfacePatchOutcome::Applied(None)
    ));
    assert!(frame_rows(&state.compose(106, 20).unwrap())
        .join("\n")
        .contains("Seattle"));
    assert!(move_to(&mut state, (0, 10)).repaint);
    let restored = state.compose(106, 20).unwrap();
    assert!(!frame_rows(&restored).join("\n").contains("Seattle"));
    let pane = &state.hits.panes[0];
    assert_eq!(
        restored.cells[usize::from(pane.inner_rect.y) * usize::from(restored.width)
            + usize::from(pane.inner_rect.x)]
        .symbol,
        "X"
    );
    let mut next = patch;
    next.base_surface_revision = 2;
    next.surface_revision = 3;
    assert!(matches!(
        state.apply_pane_surface_patch(next),
        ClientPaneSurfacePatchOutcome::Applied(Some(_))
    ));
}
