use super::*;
use crate::protocol::endpoint::EndpointStatusDetails;
use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::style::Color;
use std::time::{Duration, Instant};

const OPENING_DURATION: Duration = Duration::from_millis(150);
const OPENING_FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, PartialEq, Eq)]
pub(super) struct StatusHover {
    endpoint_id: ClientEndpointId,
    generation: Option<u64>,
    boot_id: String,
    revision: u64,
    index: usize,
    segment: crate::protocol::ClientShellTabStatusSegment,
    point: (u16, u16),
    rect: Rect,
    opened_at: Instant,
    opening_elapsed: Duration,
    // Companions precede snapshots; keep validated text for the still-visible revision.
    tooltip: String,
}

impl ClientShellState {
    pub(crate) fn set_endpoint_status_details(
        &mut self,
        source: (&ClientEndpointId, u64),
        projection: EndpointStatusDetails,
    ) -> bool {
        let (endpoint_id, generation) = source;
        let was_hovering = endpoint_id == &self.active_endpoint_id && self.status_hover.is_some();
        if was_hovering {
            self.status_hover = self.refreshed_status_hover();
        }
        let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return false;
        };
        endpoint.status_details = Some((generation, projection));
        if endpoint_id != &self.active_endpoint_id || self.status_hover.is_none() {
            return was_hovering;
        }
        let tooltip = self
            .status_hover
            .as_ref()
            .and_then(|hover| self.status_hover_tooltip(hover))
            .map(str::to_owned);
        if let (Some(hover), Some(tooltip)) = (self.status_hover.as_mut(), tooltip) {
            hover.tooltip = tooltip;
        } else {
            self.clear_status_hover();
        }
        true
    }

    fn status_tooltips(&self) -> Option<&[Option<String>]> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == self.active_endpoint_id)?;
        let (generation, details) = endpoint.status_details.as_ref()?;
        let snapshot = self.snapshot.as_deref()?;
        (endpoint.status == ClientEndpointStatus::Online
            && self
                .active_snapshot_generation
                .is_none_or(|current| current == *generation)
            && details.boot_id == snapshot.boot_id
            && details.revision == snapshot.revision
            && details.tooltips.len() == snapshot.tab_bar_right.len())
        .then_some(details.tooltips.as_slice())
    }

    fn status_hover_allowed(&self) -> bool {
        self.config.mouse_capture
            && self.mode == ClientShellMode::Terminal
            && self.overlay.is_none()
            && self.popup_terminal_id.is_none()
            && self
                .pane_surface
                .as_ref()
                .is_none_or(|surface| surface.popup.is_none())
            && self.outer_focused != Some(false)
            && self.chrome_drag.is_none()
            && self.pane_mouse_gesture.is_none()
            && self.endpoint_error.is_none()
            && self.config_diagnostic.is_none()
            && self.visible_endpoint_notice.is_none()
            && self.visible_notification.is_none()
            && self.copy_feedback.is_none()
    }

    pub(super) fn clear_status_hover(&mut self) -> bool {
        self.status_hover.take().is_some()
    }

    fn status_hover_matches_snapshot(&self, hover: &StatusHover) -> bool {
        self.status_hover_allowed()
            && hover.endpoint_id == self.active_endpoint_id
            && hover.generation == self.active_snapshot_generation
            && self.endpoint_status(&self.active_endpoint_id) == Some(ClientEndpointStatus::Online)
            && self.snapshot.as_deref().is_some_and(|snapshot| {
                hover.boot_id == snapshot.boot_id
                    && snapshot.tab_bar_right.get(hover.index) == Some(&hover.segment)
            })
    }

    fn status_hover_tooltip<'a>(&'a self, hover: &'a StatusHover) -> Option<&'a str> {
        let text = if let Some(tooltips) = self.status_tooltips() {
            tooltips.get(hover.index)?.as_deref()?
        } else {
            let endpoint = self
                .endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_id == hover.endpoint_id)?;
            let (generation, details) = endpoint.status_details.as_ref()?;
            let snapshot = self.snapshot.as_deref()?;
            if hover.revision != snapshot.revision
                || hover
                    .generation
                    .is_some_and(|current| current != *generation)
                || details.boot_id != hover.boot_id
                || details.revision <= snapshot.revision
            {
                return None;
            }
            &hover.tooltip
        };
        (!text.trim().is_empty()).then_some(text)
    }

    pub(super) fn status_hover_text(&self) -> Option<(&StatusHover, &str)> {
        let hover = self.status_hover.as_ref()?;
        if !self.status_hover_matches_snapshot(hover)
            || hover.revision != self.snapshot.as_deref()?.revision
            || !self
                .hits
                .status_segments
                .contains(&(hover.rect, hover.index))
        {
            return None;
        }
        Some((hover, self.status_hover_tooltip(hover)?))
    }

    fn refreshed_status_hover(&self) -> Option<StatusHover> {
        let hover = self.status_hover.as_ref()?;
        if !self.status_hover_matches_snapshot(hover) {
            return None;
        }
        Some(StatusHover {
            revision: self.snapshot.as_deref()?.revision,
            tooltip: self.status_hover_tooltip(hover)?.to_owned(),
            ..hover.clone()
        })
    }

    pub(super) fn revalidated_status_hover(&self) -> Option<StatusHover> {
        let mut hover = self.refreshed_status_hover()?;
        let (rect, _) = self
            .hits
            .status_segments
            .iter()
            .find(|(rect, index)| *index == hover.index && contains(*rect, hover.point))?;
        hover.rect = *rect;
        Some(hover)
    }

    pub(super) fn status_hover_deadline(&self) -> Option<Instant> {
        let hover = self.status_hover.as_ref()?;
        (hover.opening_elapsed < OPENING_DURATION).then(|| {
            hover.opened_at + (hover.opening_elapsed + OPENING_FRAME_INTERVAL).min(OPENING_DURATION)
        })
    }

    pub(crate) fn tick_status_hover(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.status_hover_deadline() else {
            return false;
        };
        if now < deadline {
            return false;
        }
        let hover = self
            .status_hover
            .as_mut()
            .expect("hover has an opening deadline");
        hover.opening_elapsed = now
            .saturating_duration_since(hover.opened_at)
            .min(OPENING_DURATION);
        true
    }

    pub(super) fn update_status_hover(&mut self, mouse: MouseEvent, now: Instant) -> bool {
        let next = (mouse.kind == MouseEventKind::Moved && self.status_hover_allowed())
            .then(|| {
                let (rect, index) = self
                    .hits
                    .status_segments
                    .iter()
                    .find(|(rect, _)| contains(*rect, (mouse.column, mouse.row)))?;
                let text = self.status_tooltips()?.get(*index)?.as_deref()?;
                if text.trim().is_empty() {
                    return None;
                }
                let snapshot = self.snapshot.as_deref()?;
                let previous = self.status_hover.as_ref().filter(|hover| {
                    self.status_hover_matches_snapshot(hover) && hover.index == *index
                });
                Some(StatusHover {
                    endpoint_id: self.active_endpoint_id.clone(),
                    generation: self.active_snapshot_generation,
                    boot_id: snapshot.boot_id.clone(),
                    revision: snapshot.revision,
                    index: *index,
                    segment: snapshot.tab_bar_right.get(*index)?.clone(),
                    point: (mouse.column, mouse.row),
                    rect: *rect,
                    opened_at: previous.map_or(now, |hover| hover.opened_at),
                    opening_elapsed: previous.map_or(Duration::ZERO, |hover| hover.opening_elapsed),
                    tooltip: text.to_owned(),
                })
            })
            .flatten();
        if self.status_hover == next {
            return false;
        }
        self.status_hover = next;
        true
    }

    pub(super) fn render_status_hover(
        &self,
        frame: &mut FrameData,
        occlusion: &mut crate::kitty_graphics::surface::Occlusion,
    ) {
        let Some((hover, text)) = self.status_hover_text() else {
            return;
        };
        let Some(mut buffer) = frame.to_ratatui_buffer() else {
            return;
        };
        let rect = render_status_card(
            &mut buffer,
            StatusCard {
                anchor: hover.rect,
                text,
                palette: &self.config.palette,
                fade: (128 + hover.opening_elapsed.as_millis() * 127 / OPENING_DURATION.as_millis())
                    as u8,
            },
        );
        if !rect.is_empty() {
            occlusion.cover(rect);
            let cursor = frame
                .cursor
                .clone()
                .filter(|cursor| !contains(rect, (cursor.x, cursor.y)));
            frame.replace_from_ratatui_buffer_preserving_effects(&buffer, cursor);
        }
    }
}

struct StatusCard<'a> {
    anchor: Rect,
    text: &'a str,
    palette: &'a Palette,
    fade: u8,
}

impl StatusCard<'_> {
    fn color(&self, color: Color) -> Color {
        match (self.palette.panel_bg, color) {
            (Color::Rgb(br, bg, bb), Color::Rgb(r, g, b)) => {
                let mix = |from: u8, to: u8| {
                    ((u16::from(from) * (255 - u16::from(self.fade))
                        + u16::from(to) * u16::from(self.fade))
                        / 255) as u8
                };
                Color::Rgb(mix(br, r), mix(bg, g), mix(bb, b))
            }
            _ => color,
        }
    }
}

fn render_status_card(buffer: &mut Buffer, card: StatusCard<'_>) -> Rect {
    let StatusCard {
        anchor,
        text,
        palette,
        ..
    } = card;
    use ratatui::widgets::{Clear, Paragraph, Widget};
    let area = buffer.area;
    if area.width < 3 || area.height < 3 {
        return Rect::default();
    }
    let lines = text.lines().take(16).collect::<Vec<_>>();
    let width = lines
        .iter()
        .map(|line| render::display_width(line))
        .max()
        .unwrap_or(0)
        .saturating_add(2)
        .min(area.width);
    let desired_height = (lines.len() as u16).saturating_add(2);
    let below = area.bottom().saturating_sub(anchor.bottom().max(area.y));
    let above = anchor.y.min(area.bottom()).saturating_sub(area.y);
    let place_below = desired_height <= below || (desired_height > above && below >= above);
    let height = desired_height.min(if place_below { below } else { above });
    if height < 3 {
        return Rect::default();
    }
    let y = if place_below {
        anchor.bottom().max(area.y)
    } else {
        anchor.y.min(area.bottom()) - height
    };
    let rect = Rect::new(
        anchor.x.clamp(area.x, area.right().saturating_sub(width)),
        y,
        width,
        height,
    );
    Clear.render(rect, buffer);
    let block = notifications::notification_card_block(palette)
        .border_style(Style::default().fg(card.color(palette.overlay0)));
    let inner = block.inner(rect);
    block.render(rect, buffer);
    Paragraph::new(
        lines
            .into_iter()
            .map(ratatui::text::Line::from)
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .fg(card.color(palette.text))
            .bg(palette.panel_bg),
    )
    .render(inner, buffer);
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_card_fade_preserves_geometry_background_and_plain_text() {
        let palette = Palette::catppuccin();
        let mut previous: Option<Buffer> = None;
        for fade in [128, 192, 255] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 10));
            let rect = render_status_card(
                &mut buffer,
                StatusCard {
                    anchor: Rect::new(4, 0, 8, 1),
                    text: "Plain detail\nSecond line",
                    palette: &palette,
                    fade,
                },
            );
            assert_eq!(rect, Rect::new(4, 1, 14, 4));
            let text_color = buffer[(rect.x + 1, rect.y + 1)].fg;
            if fade == 255 {
                assert_eq!(text_color, palette.text);
            } else {
                assert_ne!(text_color, palette.text);
            }
            if let Some(previous) = previous.as_ref() {
                for (before, after) in previous.content.iter().zip(&buffer.content) {
                    assert_eq!(before.symbol(), after.symbol());
                    assert_eq!(before.bg, after.bg);
                }
            }
            previous = Some(buffer);
        }
    }

    #[test]
    fn status_card_stays_off_top_and_bottom_anchors_at_both_viewport_edges() {
        let palette = crate::app::client_palette_from_config(&Config::default());
        for area in [Rect::new(0, 0, 12, 6), Rect::new(4, 2, 12, 6)] {
            for x in [area.x, area.right() - 1] {
                for y in [area.y, area.bottom() - 1] {
                    let anchor = Rect::new(x, y, 1, 1);
                    let mut buffer = Buffer::empty(area);
                    buffer[(x, y)].set_symbol("A");
                    let rect = render_status_card(
                        &mut buffer,
                        StatusCard {
                            anchor,
                            text: "detail",
                            palette: &palette,
                            fade: 255,
                        },
                    );
                    assert_eq!(rect.width, 8);
                    assert_eq!(rect.height, 3);
                    assert_eq!(rect.intersection(area), rect);
                    assert!(rect.intersection(anchor).is_empty());
                    assert_eq!(buffer[(x, y)].symbol(), "A");
                    assert_eq!(
                        rect.y,
                        if y == area.y {
                            anchor.bottom()
                        } else {
                            anchor.y - 3
                        }
                    );
                    assert_eq!(
                        rect.x,
                        if x == area.x {
                            area.x
                        } else {
                            area.right() - 8
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn status_card_uses_only_available_rows_or_hides_on_tiny_screens() {
        let palette = crate::app::client_palette_from_config(&Config::default());
        for height in 1..=5 {
            let area = Rect::new(0, 0, 12, height);
            for y in [0, height / 2, height - 1] {
                let anchor = Rect::new(0, y, 2, 1);
                let mut buffer = Buffer::empty(area);
                let rect = render_status_card(
                    &mut buffer,
                    StatusCard {
                        anchor,
                        text: "one\ntwo\nthree",
                        palette: &palette,
                        fade: 255,
                    },
                );
                assert!(rect.intersection(anchor).is_empty());
                if y.max(height - y - 1) < 3 {
                    assert!(rect.is_empty());
                } else {
                    assert_eq!(rect.height, y.max(height - y - 1));
                    assert_eq!(rect.intersection(area), rect);
                }
            }
        }
    }

    #[test]
    fn status_card_clips_multiline_content_to_viewport_and_clamps_both_edges() {
        let palette = crate::app::client_palette_from_config(&Config::default());
        for area in [Rect::new(0, 0, 12, 6), Rect::new(4, 2, 12, 6)] {
            let mut buffer = Buffer::empty(area);
            let anchor = Rect::new(area.right() - 1, area.bottom() - 1, 1, 1);
            let rect = render_status_card(
                &mut buffer,
                StatusCard {
                    anchor,
                    text: "123456789012345\nline2\nline3",
                    palette: &palette,
                    fade: 255,
                },
            );
            assert_eq!(rect, Rect::new(area.x, anchor.y - 5, 12, 5));
            assert!(rect.intersection(anchor).is_empty());
            assert_eq!(rect.intersection(area), rect);
            let text = buffer
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(text.contains("line2"));
            assert!(!text.contains("123456789012345"));
        }
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 40));
        let text = (0..30).map(|_| "line").collect::<Vec<_>>().join("\n");
        let rect = render_status_card(
            &mut buffer,
            StatusCard {
                anchor: Rect::new(2, 0, 2, 1),
                text: &text,
                palette: &palette,
                fade: 255,
            },
        );
        assert_eq!(rect, Rect::new(2, 1, 6, 18));
        for area in [Rect::new(0, 0, 2, 8), Rect::new(0, 0, 8, 2)] {
            let mut buffer = Buffer::empty(area);
            assert!(render_status_card(
                &mut buffer,
                StatusCard {
                    anchor: area,
                    text: "detail",
                    palette: &palette,
                    fade: 255,
                }
            )
            .is_empty());
        }
    }
}
