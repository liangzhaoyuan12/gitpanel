use std::collections::HashMap;

use adw::prelude::*;
use gtk::cairo;

use gitpulsar_core::models::CommitInfo;

const LANE_WIDTH: f64 = 16.0;
const DOT_RADIUS: f64 = 4.0;
const LINE_WIDTH: f64 = 2.0;
const PADDING: f64 = 8.0;
const ROW_HEIGHT: f64 = 32.0;

const PALETTE: &[(f64, f64, f64)] = &[
    (0.204, 0.400, 0.643), // #3466A4 Blue
    (0.541, 0.282, 0.569), // #8A4891 Purple
    (0.180, 0.545, 0.341), // #2E8B57 Green
    (0.827, 0.525, 0.098), // #D38619 Orange
    (0.776, 0.263, 0.263), // #C64343 Red
    (0.200, 0.557, 0.557), // #338E8E Teal
    (0.659, 0.529, 0.267), // #A88744 Gold
    (0.467, 0.400, 0.647), // #7766A5 Violet
];

#[derive(Debug, Clone)]
pub struct GraphRow {
    pub commit_lane: usize,
    pub color_idx: usize,
    pub lanes: Vec<Option<bool>>, // true = active lane, false = inactive
    pub lane_colors: Vec<usize>,
    pub merge_sources: Vec<(usize, usize)>, // (lane, color_idx)
    pub num_active_lanes: usize,
    pub summary: String,
    pub short_id: String,
}

pub fn compute_graph(commits: &[CommitInfo]) -> Vec<GraphRow> {
    if commits.is_empty() {
        return Vec::new();
    }

    let commit_idx: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut lane_colors: Vec<usize> = Vec::new();
    let mut next_color: usize = 0;
    let mut rows: Vec<GraphRow> = Vec::with_capacity(commits.len());

    for commit in commits {
        let existing_lane = lanes
            .iter()
            .position(|l| l.as_deref() == Some(&commit.id));

        let commit_lane = if let Some(lane) = existing_lane {
            lane
        } else {
            let color = next_color % PALETTE.len();
            next_color += 1;
            if let Some(free) = lanes.iter().position(|l| l.is_none()) {
                lanes[free] = Some(commit.id.clone());
                lane_colors[free] = color;
                free
            } else {
                lanes.push(Some(commit.id.clone()));
                lane_colors.push(color);
                lanes.len() - 1
            }
        };

        let my_color = lane_colors[commit_lane];
        let mut merge_sources: Vec<(usize, usize)> = Vec::new();

        if let Some(first_parent) = commit.parent_ids.first() {
            if commit_idx.contains_key(first_parent.as_str()) {
                lanes[commit_lane] = Some(first_parent.clone());
            } else {
                lanes[commit_lane] = None;
            }
        } else {
            lanes[commit_lane] = None;
        }

        for parent_id in commit.parent_ids.iter().skip(1) {
            if !commit_idx.contains_key(parent_id.as_str()) {
                continue;
            }
            let parent_lane = lanes
                .iter()
                .position(|l| l.as_deref() == Some(parent_id.as_str()));

            if let Some(pl) = parent_lane {
                merge_sources.push((pl, lane_colors[pl]));
            } else {
                let color = next_color % PALETTE.len();
                next_color += 1;
                if let Some(free) = lanes.iter().position(|l| l.is_none()) {
                    lanes[free] = Some(parent_id.clone());
                    lane_colors[free] = color;
                    merge_sources.push((free, color));
                } else {
                    lanes.push(Some(parent_id.clone()));
                    lane_colors.push(color);
                    merge_sources.push((lanes.len() - 1, color));
                }
            }
        }

        let num_active = lanes.iter().filter(|l| l.is_some()).count();

        let row_lanes: Vec<Option<bool>> = lanes
            .iter()
            .map(|l| l.as_ref().map(|_| true))
            .collect();

        rows.push(GraphRow {
            commit_lane,
            color_idx: my_color,
            lanes: row_lanes,
            lane_colors: lane_colors.clone(),
            merge_sources,
            num_active_lanes: num_active,
            summary: commit.summary.clone(),
            short_id: commit.short_id.clone(),
        });
    }

    rows
}

/// Public wrapper for drawing a single graph row (used by graph tab).
pub fn draw_graph_row_public(cr: &cairo::Context, row: &GraphRow, height: f64, all_rows: &[GraphRow], row_idx: usize) {
    draw_graph_row(cr, row, height, all_rows, row_idx);
}

fn draw_graph_row(cr: &cairo::Context, row: &GraphRow, height: f64, all_rows: &[GraphRow], row_idx: usize) {
    let mid_y = height / 2.0;

    // Draw vertical pass-through lines
    for (i, segment) in row.lanes.iter().enumerate() {
        if segment.is_some() {
            let x = lane_x(i);
            let (r, g, b) = color_for_lane(i, &row.lane_colors);
            cr.set_source_rgb(r, g, b);
            cr.set_line_width(LINE_WIDTH);

            if i == row.commit_lane {
                // Line above dot
                cr.move_to(x, 0.0);
                cr.line_to(x, mid_y - DOT_RADIUS);
                let _ = cr.stroke();
                // Line below dot (only if next row also has this lane active)
                let has_continuation = row_idx + 1 < all_rows.len()
                    && all_rows[row_idx + 1]
                        .lanes
                        .get(i)
                        .copied()
                        .flatten()
                        .is_some();
                if has_continuation {
                    cr.move_to(x, mid_y + DOT_RADIUS);
                    cr.line_to(x, height);
                    let _ = cr.stroke();
                }
            } else {
                cr.move_to(x, 0.0);
                cr.line_to(x, height);
                let _ = cr.stroke();
            }
        }
    }

    // Draw merge curves
    let commit_x = lane_x(row.commit_lane);
    for &(src_lane, color_idx) in &row.merge_sources {
        let src_x = lane_x(src_lane);
        let (r, g, b) = PALETTE[color_idx % PALETTE.len()];
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(LINE_WIDTH);

        cr.move_to(src_x, 0.0);
        cr.curve_to(src_x, mid_y * 0.5, commit_x, mid_y * 0.5, commit_x, mid_y);
        let _ = cr.stroke();
    }

    // Draw commit dot
    let (r, g, b) = PALETTE[row.color_idx % PALETTE.len()];
    cr.set_source_rgb(r, g, b);
    cr.arc(commit_x, mid_y, DOT_RADIUS, 0.0, 2.0 * std::f64::consts::PI);
    let _ = cr.fill();
}

/// Create a small inline graph widget for a single commit row.
/// Uses Rc to share graph data across all rows without cloning.
pub fn create_inline_graph(graph_rows: &std::rc::Rc<Vec<GraphRow>>, row_idx: usize, max_lanes: usize) -> gtk::DrawingArea {
    let width = (max_lanes as f64 * LANE_WIDTH + PADDING * 2.0).ceil() as i32;
    let row_height = ROW_HEIGHT as i32;

    let da = gtk::DrawingArea::builder()
        .content_width(width.min(120))
        .content_height(row_height)
        .valign(gtk::Align::Center)
        .build();

    let rows = graph_rows.clone();
    let idx = row_idx;
    da.set_draw_func(move |_da, cr, _w, _h| {
        if let Some(row) = rows.get(idx) {
            draw_graph_row(cr, row, ROW_HEIGHT, &rows, idx);
        }
    });

    da
}

fn lane_x(lane: usize) -> f64 {
    PADDING + lane as f64 * LANE_WIDTH + LANE_WIDTH / 2.0
}

fn color_for_lane(lane: usize, lane_colors: &[usize]) -> (f64, f64, f64) {
    let idx = lane_colors.get(lane).copied().unwrap_or(0);
    PALETTE[idx % PALETTE.len()]
}
