//! Copy and paste for the zoom and 3D tracks.
//!
//! Both tracks are single-lane: the editor never lays a new segment over an
//! existing one, so a paste lands as a group at the playhead or the first free
//! stretch after it, keeping the copies' relative timing. A 3D shot's pose
//! tracks are timed from its own start, so moving the edges is all a clone
//! needs.

use cap_project::{Camera3DSegment, TimelineConfiguration, ZoomSegment};
use gpui::{Context, Window};

use crate::{
    editor_edits::Selection,
    editor_timeline::TrackKind,
    editor_window::EditorWindow,
};

#[derive(Debug, Clone, Default)]
pub(crate) enum EffectClipboard {
    #[default]
    Empty,
    Zoom(Vec<ZoomSegment>),
    ThreeD(Vec<Camera3DSegment>),
}

impl EffectClipboard {
    pub(crate) fn copy(
        timeline: &TimelineConfiguration,
        kind: TrackKind,
        indices: &[usize],
    ) -> Self {
        match kind {
            TrackKind::Zoom => {
                let mut segments: Vec<ZoomSegment> = indices
                    .iter()
                    .filter_map(|index| timeline.zoom_segments.get(*index).cloned())
                    .collect();
                segments.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.end.total_cmp(&b.end)));
                if segments.is_empty() {
                    Self::Empty
                } else {
                    Self::Zoom(segments)
                }
            }
            TrackKind::ThreeD => {
                let mut segments: Vec<Camera3DSegment> = indices
                    .iter()
                    .filter_map(|index| timeline.camera3d_segments.get(*index).cloned())
                    .collect();
                segments.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.end.total_cmp(&b.end)));
                if segments.is_empty() {
                    Self::Empty
                } else {
                    Self::ThreeD(segments)
                }
            }
            _ => Self::Empty,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Zoom(segments) => segments.is_empty(),
            Self::ThreeD(segments) => segments.is_empty(),
        }
    }

    pub(crate) fn track(&self) -> Option<TrackKind> {
        match self {
            Self::Empty => None,
            Self::Zoom(_) => Some(TrackKind::Zoom),
            Self::ThreeD(_) => Some(TrackKind::ThreeD),
        }
    }

    pub(crate) fn paste(
        &self,
        timeline: &mut TimelineConfiguration,
        at: f64,
        total: f64,
    ) -> Option<Selection> {
        match self {
            Self::Empty => None,
            Self::Zoom(segments) => {
                let spans: Vec<(f64, f64)> = segments
                    .iter()
                    .map(|segment| (segment.start, segment.end))
                    .collect();
                let existing: Vec<(f64, f64)> = timeline
                    .zoom_segments
                    .iter()
                    .map(|segment| (segment.start, segment.end))
                    .collect();
                let offset = paste_offset(&existing, &spans, at, total)?;
                let mut indices = Vec::with_capacity(segments.len());
                for segment in segments {
                    let mut copy = segment.clone();
                    copy.start += offset;
                    copy.end += offset;
                    let index = insert_zoom(timeline, copy);
                    bump_pasted_indices(&mut indices, index);
                    indices.push(index);
                }
                indices.sort_unstable();
                Some(Selection {
                    track: TrackKind::Zoom,
                    indices,
                })
            }
            Self::ThreeD(segments) => {
                let spans: Vec<(f64, f64)> = segments
                    .iter()
                    .map(|segment| (segment.start, segment.end))
                    .collect();
                let existing: Vec<(f64, f64)> = timeline
                    .camera3d_segments
                    .iter()
                    .map(|segment| (segment.start, segment.end))
                    .collect();
                let offset = paste_offset(&existing, &spans, at, total)?;
                let mut indices = Vec::with_capacity(segments.len());
                for segment in segments {
                    let mut copy = segment.clone();
                    copy.start += offset;
                    copy.end += offset;
                    let index = insert_three_d(timeline, copy);
                    bump_pasted_indices(&mut indices, index);
                    indices.push(index);
                }
                indices.sort_unstable();
                Some(Selection {
                    track: TrackKind::ThreeD,
                    indices,
                })
            }
        }
    }
}

fn insert_zoom(timeline: &mut TimelineConfiguration, segment: ZoomSegment) -> usize {
    let start = segment.start;
    let mut index = 0;
    for (position, existing) in timeline.zoom_segments.iter().enumerate() {
        if existing.start < start {
            index = position + 1;
        }
    }
    timeline.zoom_segments.insert(index, segment);
    index
}

fn insert_three_d(timeline: &mut TimelineConfiguration, segment: Camera3DSegment) -> usize {
    let start = segment.start;
    let mut index = 0;
    for (position, existing) in timeline.camera3d_segments.iter().enumerate() {
        if existing.start < start {
            index = position + 1;
        }
    }
    timeline.camera3d_segments.insert(index, segment);
    index
}

fn bump_pasted_indices(indices: &mut [usize], inserted: usize) {
    for index in indices {
        if *index >= inserted {
            *index += 1;
        }
    }
}

/// The shift from the group's earliest start to where it should be pasted.
fn paste_offset(existing: &[(f64, f64)], group: &[(f64, f64)], at: f64, total: f64) -> Option<f64> {
    let start = paste_group_start(existing, group, at, total)?;
    let first = group
        .iter()
        .map(|span| span.0)
        .fold(f64::INFINITY, f64::min);
    Some(start - first)
}

/// Where a copied group lands: `at` unless the group would overlap something
/// already on the track, in which case it slides right past it, clamped so the
/// whole group stays inside `total`. `None` when no stretch is long enough.
pub(crate) fn paste_group_start(
    existing: &[(f64, f64)],
    group: &[(f64, f64)],
    at: f64,
    total: f64,
) -> Option<f64> {
    if group.is_empty() || !total.is_finite() || total <= 0. {
        return None;
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &(start, end) in group {
        if !start.is_finite() || !end.is_finite() || end <= start {
            return None;
        }
        min = min.min(start);
        max = max.max(end);
    }
    let span = max - min;
    if !span.is_finite() || span <= 0. || span > total {
        return None;
    }

    let mut occupied: Vec<(f64, f64)> = existing
        .iter()
        .copied()
        .filter(|(start, end)| start.is_finite() && end.is_finite() && end > start)
        .map(|(start, end)| (start.max(0.), end.min(total)))
        .filter(|(start, end)| end > start)
        .collect();
    occupied.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut start = if at.is_finite() {
        at.clamp(0., total - span)
    } else {
        0.
    };
    for _ in 0..=group.len() + occupied.len() {
        let mut next = start;
        for &(group_start, group_end) in group {
            let relative_start = group_start - min;
            let relative_end = group_end - min;
            for &(occupied_start, occupied_end) in &occupied {
                if start + relative_start < occupied_end - 1e-9
                    && occupied_start < start + relative_end - 1e-9
                {
                    next = next.max(occupied_end - relative_start);
                }
            }
        }
        if next <= start + 1e-9 {
            return (start + span <= total + 1e-9).then_some(start);
        }
        start = next;
        if start + span > total + 1e-9 {
            return None;
        }
    }
    None
}

impl EditorWindow {
    /// `Cmd/Ctrl+C` and `Cmd/Ctrl+V` on the zoom or 3D lane. Returns whether
    /// the key was consumed, so the shape clipboard's arms still run when this
    /// one has nothing to do.
    pub(crate) fn effect_clipboard_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "c" => self.copy_effects(),
            "v" => self.paste_effects(window, cx),
            _ => false,
        }
    }

    fn copy_effects(&mut self) -> bool {
        let Some(timeline) = self.project.timeline.as_ref() else {
            return false;
        };
        let Some(selection) = self.selection.as_ref() else {
            return false;
        };
        let clipboard = EffectClipboard::copy(timeline, selection.track, &selection.indices);
        if clipboard.is_empty() {
            return false;
        }
        self.effect_clipboard = clipboard;
        self.shape_clipboard.clear();
        true
    }

    fn paste_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let clipboard = self.effect_clipboard.clone();
        let Some(track) = clipboard.track() else {
            return false;
        };
        let at = self.playhead.max(0.);
        let total = self.total_duration();
        let mut pasted = None;
        let changed = self.edit(
            |timeline| {
                pasted = clipboard.paste(timeline, at, total);
                pasted.is_some()
            },
            window,
            cx,
        );
        if !changed {
            return false;
        }
        if let Some(selection) = pasted {
            self.set_selection(Some(selection), cx);
            let reason = match track {
                TrackKind::ThreeD => "paste-camera3d",
                _ => "paste-zoom",
            };
            self.note_edit(reason, Some(track));
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_edits::{default_camera3d_segment, insert_zoom_segment};

    fn timeline() -> TimelineConfiguration {
        serde_json::from_value(serde_json::json!({
            "segments": [{ "recordingSegment": 0, "timescale": 1.0, "start": 0.0, "end": 30.0 }],
            "zoomSegments": []
        }))
        .expect("fixture parses")
    }

    #[test]
    fn copying_zoom_segments_takes_the_selection_in_time_order() {
        let mut timeline = timeline();
        insert_zoom_segment(&mut timeline, 2., 5., 1.5);
        insert_zoom_segment(&mut timeline, 20., 24., 2.);
        timeline.zoom_segments[0].mode = cap_project::ZoomMode::Manual { x: 0.25, y: 0.75 };
        timeline.zoom_segments[0].glide_direction = cap_project::GlideDirection::Right;
        timeline.zoom_segments[0].glide_speed = 0.9;

        let clipboard = EffectClipboard::copy(&timeline, TrackKind::Zoom, &[1, 0]);
        let EffectClipboard::Zoom(copied) = &clipboard else {
            panic!("expected the zoom clipboard");
        };
        assert_eq!(copied.len(), 2);
        assert_eq!(copied[0].start, 2.);
        assert_eq!(
            copied[0].glide_direction,
            cap_project::GlideDirection::Right
        );
        assert_eq!(copied[0].glide_speed, 0.9);
        assert!(matches!(
            copied[0].mode,
            cap_project::ZoomMode::Manual { .. }
        ));
        assert_eq!(copied[1].start, 20.);
        assert_eq!(copied[1].amount, 2.);
    }

    #[test]
    fn pasting_zoom_segments_keeps_their_settings_and_relative_timing() {
        let mut timeline = timeline();
        insert_zoom_segment(&mut timeline, 2., 5., 1.5);
        insert_zoom_segment(&mut timeline, 8., 10., 2.);
        timeline.zoom_segments[1].mode = cap_project::ZoomMode::Manual { x: 0.1, y: 0.2 };

        let clipboard = EffectClipboard::copy(&timeline, TrackKind::Zoom, &[0, 1]);
        let selection = clipboard
            .paste(&mut timeline, 12., 30.)
            .expect("room after 12s");

        assert_eq!(selection.track, TrackKind::Zoom);
        assert_eq!(selection.indices, vec![2, 3]);
        assert_eq!(timeline.zoom_segments.len(), 4);
        assert_eq!(timeline.zoom_segments[2].start, 12.);
        assert_eq!(timeline.zoom_segments[2].end, 15.);
        assert_eq!(timeline.zoom_segments[3].start, 18.);
        assert_eq!(timeline.zoom_segments[3].end, 20.);
        assert_eq!(timeline.zoom_segments[2].amount, 1.5);
        assert!(matches!(
            timeline.zoom_segments[3].mode,
            cap_project::ZoomMode::Manual { .. }
        ));
    }

    #[test]
    fn pasting_zoom_segments_slides_past_what_is_already_there() {
        let mut timeline = timeline();
        insert_zoom_segment(&mut timeline, 2., 5., 1.5);
        let clipboard = EffectClipboard::copy(&timeline, TrackKind::Zoom, &[0]);
        let selection = clipboard.paste(&mut timeline, 3.5, 30.).expect("room");

        assert_eq!(timeline.zoom_segments.len(), 2);
        assert_eq!(timeline.zoom_segments[1].start, 5.);
        assert_eq!(timeline.zoom_segments[1].end, 8.);
        assert_eq!(selection.indices, vec![1]);
    }

    #[test]
    fn pasting_zoom_segments_keeps_the_group_inside_the_timeline() {
        let mut timeline = timeline();
        insert_zoom_segment(&mut timeline, 2., 5., 1.5);
        let clipboard = EffectClipboard::copy(&timeline, TrackKind::Zoom, &[0]);
        let selection = clipboard.paste(&mut timeline, 29., 30.).expect("room");

        assert_eq!(timeline.zoom_segments[1].start, 27.);
        assert_eq!(timeline.zoom_segments[1].end, 30.);
        assert_eq!(selection.indices, vec![1]);
    }

    #[test]
    fn pasting_zoom_segments_needs_a_free_stretch() {
        let mut timeline = timeline();
        insert_zoom_segment(&mut timeline, 0., 30., 1.5);
        let clipboard = EffectClipboard::copy(&timeline, TrackKind::Zoom, &[0]);

        assert_eq!(clipboard.paste(&mut timeline, 0., 30.), None);
        assert_eq!(timeline.zoom_segments.len(), 1);
    }

    #[test]
    fn pasting_3d_shots_moves_the_edges_and_keeps_the_pose_tracks() {
        let mut timeline = timeline();
        let mut shot = default_camera3d_segment(2., 6.);
        shot.properties.zoom = 1.75;
        shot.transition_in = 0.4;
        shot.tracks.tilt_x = vec![
            cap_project::Camera3DKeyframe {
                time: 0.,
                value: 12.,
                out_easing: None,
                in_easing: None,
            },
            cap_project::Camera3DKeyframe {
                time: 4.,
                value: 24.,
                out_easing: None,
                in_easing: None,
            },
        ];
        timeline.camera3d_segments.push(shot);

        let clipboard = EffectClipboard::copy(&timeline, TrackKind::ThreeD, &[0]);
        let selection = clipboard.paste(&mut timeline, 10., 30.).expect("room");

        assert_eq!(selection.track, TrackKind::ThreeD);
        assert_eq!(selection.indices, vec![1]);
        let pasted = &timeline.camera3d_segments[1];
        assert_eq!(pasted.start, 10.);
        assert_eq!(pasted.end, 14.);
        assert_eq!(pasted.properties.zoom, 1.75);
        assert_eq!(pasted.transition_in, 0.4);
        assert_eq!(pasted.tracks.tilt_x.len(), 2);
        assert_eq!(pasted.tracks.tilt_x[0].time, 0.);
        assert_eq!(pasted.tracks.tilt_x[1].time, 4.);
        assert_eq!(pasted.tracks.tilt_x[1].value, 24.);
    }

    #[test]
    fn an_empty_or_unsupported_selection_copies_nothing() {
        let timeline = timeline();
        assert!(matches!(
            EffectClipboard::copy(&timeline, TrackKind::Zoom, &[]),
            EffectClipboard::Empty
        ));
        assert!(matches!(
            EffectClipboard::copy(&timeline, TrackKind::Shape, &[0]),
            EffectClipboard::Empty
        ));
        assert!(matches!(
            EffectClipboard::copy(&timeline, TrackKind::ThreeD, &[0]),
            EffectClipboard::Empty
        ));
        assert!(EffectClipboard::Empty.is_empty());
        assert_eq!(EffectClipboard::Empty.track(), None);
    }

    #[test]
    fn paste_placement_rejects_invalid_spans() {
        assert_eq!(paste_group_start(&[], &[], 0., 30.), None);
        assert_eq!(paste_group_start(&[], &[(0., 0.)], 0., 30.), None);
        assert_eq!(paste_group_start(&[], &[(0., f64::NAN)], 0., 30.), None);
        assert_eq!(paste_group_start(&[], &[(0., 1.)], 0., 0.), None);
        assert_eq!(paste_group_start(&[], &[(0., 31.)], 0., 30.), None);
        assert_eq!(paste_group_start(&[], &[(0., 1.)], f64::NAN, 30.), Some(0.));
    }
}
