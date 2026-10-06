//! Plugin window drawn as a chrome ball. The bounce inside can be hidden.

use crate::dsp::{Direction, MAX_BALLS};
use crate::BouncingBallParams;
use egui::{
    emath::Align2,
    epaint::{Mesh, Vertex},
    pos2, vec2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    UiBuilder, Vec2,
};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{Frame, NiceEguiApp};
use std::f32::consts::TAU;
use std::sync::Arc;

pub struct BallEditor {
    params: Arc<BouncingBallParams>,
    gui: Option<GuiContext>,
    preview: Preview,
    /// When false the inner bounce is hidden and the window stays a still ball.
    animate: bool,
    /// Highlight position on the window ball, frozen while the animation is hidden.
    shine: f32,
}

impl BallEditor {
    pub(crate) fn new(params: Arc<BouncingBallParams>) -> Self {
        Self {
            params,
            gui: None,
            preview: Preview::default(),
            animate: true,
            shine: 0.35,
        }
    }

    fn simulate(&mut self, dt: f32) -> [Option<DrawnBall>; MAX_BALLS] {
        let drop = (self.params.drop.value() / 1000.0).clamp(0.02, 2.0);
        let rest = (self.params.rest.value() / 1000.0).clamp(0.001, drop * 0.85);
        let restitution = self.params.bounciness.value().clamp(0.5, 0.97);
        let direction = self.params.direction.value();
        let count = self.params.balls.value().clamp(1, MAX_BALLS as i32) as usize;
        self.preview
            .advance(dt, count, direction, drop, rest, restitution)
    }
}

impl NiceEguiApp for BallEditor {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = Color32::TRANSPARENT;
        visuals.extreme_bg_color = Color32::from_rgb(22, 24, 28);
        visuals.faint_bg_color = Color32::from_rgba_unmultiplied(18, 20, 24, 40);
        visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(16, 18, 22, 70);
        visuals.widgets.hovered.bg_fill = Color32::from_rgba_unmultiplied(24, 26, 30, 120);
        visuals.widgets.active.bg_fill = Color32::from_rgba_unmultiplied(12, 14, 18, 160);
        visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(214, 218, 224);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_black_alpha(90));
        visuals.selection.bg_fill = Color32::from_rgb(70, 74, 82);
        visuals.selection.stroke.color = Color32::from_rgb(210, 214, 220);
        visuals.override_text_color = Some(Color32::from_rgb(226, 228, 232));
        egui_ctx.set_visuals(visuals);
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "round_bound".to_owned(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/round-bound-font/RoundBoundDemoRegular-V4XqB.ttf"
            ))),
        );
        fonts
            .families
            .entry(egui::FontFamily::Name("RoundBound".into()))
            .or_default()
            .insert(0, "round_bound".to_owned());
        egui_ctx.set_fonts(fonts);
        self.gui = Some(nice_gui_ctx);
        Ok(())
    }

    fn editor_closed(&mut self) {
        self.gui = None;
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        // Corners outside the disc stay clear so the editor reads as a ball.
        frame.set_clear_color(egui::Rgba::TRANSPARENT);
        let full = ui.max_rect();
        let diameter = full.width().min(full.height());
        let center = pos2(full.left() + diameter * 0.5, full.top() + diameter * 0.5);
        let radius = diameter * 0.5;

        let drawn = if self.animate {
            ui.ctx().request_repaint();
            let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.05);
            let drawn = self.simulate(dt);
            if let Some(ball) = drawn.iter().flatten().next() {
                self.shine = ball.flight;
            }
            Some(drawn)
        } else {
            None
        };
        paint_chrome_ball(
            ui.painter(),
            center,
            vec2(radius * 0.985, radius * 0.985),
            self.shine,
        );
        // A shallow cut around the equator, so the controls read as part of the same sphere.
        ui.painter().add(Shape::ellipse_stroke(
            pos2(center.x, center.y + radius * 0.01),
            vec2(radius * 0.9, radius * 0.045),
            Stroke::new(1.0, Color32::from_black_alpha(55)),
        ));
        ui.painter().add(Shape::ellipse_stroke(
            pos2(center.x, center.y + radius * 0.028),
            vec2(radius * 0.86, radius * 0.03),
            Stroke::new(1.0, Color32::from_white_alpha(28)),
        ));
        let title = curved_title(ui, radius);
        let title_bottom = center.y - title.arc_radius + title.galley.rect.height() * 0.5;
        let button = Rect::from_center_size(pos2(center.x, title_bottom + 18.0), vec2(92.0, 22.0));
        if let Some(drawn) = drawn.as_ref() {
            let stage_top = ((button.bottom() + 8.0 - center.y) / radius).clamp(-0.58, -0.28);
            let stage_bottom = (stage_top + 0.16).min(-0.12);
            if stage_bottom > stage_top + 0.05 {
                let stage = chord_rect(center, radius * 0.86, stage_top, stage_bottom);
                paint_stage(ui.painter(), stage, drawn);
            }
        }

        ui.painter().add(Shape::ellipse_stroke(
            center,
            vec2(radius - 1.5, radius - 1.5),
            Stroke::new(2.0, Color32::from_rgb(36, 38, 42)),
        ));

        engrave_arc(ui.painter(), center, title.arc_radius, &title.galley);

        let hide = ui.interact(button, ui.id().with("hide-anim"), Sense::click());
        let label = if self.animate { "Hide" } else { "Show" };
        paint_inset(ui.painter(), button, hide.hovered());
        engrave(
            ui.painter(),
            button.center(),
            Align2::CENTER_CENTER,
            label,
            12.0,
        );
        if hide.clicked() {
            self.animate = !self.animate;
        }

        if let Some(gui) = self.gui.as_ref() {
            let controls = centered_panel(center, radius);
            let mut controls_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt("ball-controls")
                    .max_rect(controls)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
            );
            controls_ui.spacing_mut().item_spacing = vec2(10.0, 2.0);
            let params = Arc::clone(&self.params);
            paint_controls(&mut controls_ui, &params, &gui.param_setter());
        }
    }
}

/// Light type for the grey steel, with a dark shadow so the name stays readable.
fn slider_label(painter: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, size: f32) {
    let font = FontId::proportional(size);
    let ink = Color32::from_rgb(255, 214, 120);
    for offset in [
        vec2(-1.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, -1.0),
        vec2(0.0, 1.0),
    ] {
        painter.text(
            pos + offset,
            anchor,
            text,
            font.clone(),
            Color32::from_black_alpha(230),
        );
    }
    painter.text(pos, anchor, text, font, ink);
}

const TITLE: &str = "Bouncing Boll";

fn round_bound_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("RoundBound".into()))
}

fn layout_title(ui: &egui::Ui, text: &str, size: f32) -> std::sync::Arc<egui::epaint::Galley> {
    ui.ctx().fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            text.to_owned(),
            round_bound_font(size),
            Color32::PLACEHOLDER,
        )
    })
}

/// Crown-to-end angle of the title, in radians.
/// The letters sit on a circle concentric with the ball, so the end letters
/// tilt by this much. Kept shallow so the line stays above the preview.
const TITLE_HALF_ANGLE: f32 = 0.62;

struct CurvedTitle {
    galley: std::sync::Arc<egui::epaint::Galley>,
    /// Distance from the ball center to the middle of the line.
    arc_radius: f32,
}

/// Font size and arc radius for a title of the given unscaled box.
/// `width_at_100` and `height_at_100` are the galley size at font size 100.
fn title_metrics(radius: f32, width_at_100: f32, height_at_100: f32) -> (f32, f32) {
    let pad = 18.0;
    let half = TITLE_HALF_ANGLE;
    let width_per = width_at_100 / 100.0;
    let height_per = height_at_100 / 100.0;
    let numer = half * (radius - pad);
    let denom = width_per * 0.5 + half * height_per * 0.5;
    let size = (numer / denom.max(1.0e-3)).clamp(18.0, radius * 0.72);
    let arc_radius = (radius - height_per * size * 0.5 - pad).max(radius * 0.35);
    (size, arc_radius)
}

fn curved_title(ui: &egui::Ui, radius: f32) -> CurvedTitle {
    let probe = layout_title(ui, TITLE, 100.0);
    let (size, arc_radius) = title_metrics(radius, probe.rect.width(), probe.rect.height());
    CurvedTitle {
        galley: layout_title(ui, TITLE, size),
        arc_radius,
    }
}

/// Point on the title arc. `local.x` runs right from the crown, `local.y` runs
/// down from the middle of the line, toward the ball center.
fn arc_point(center: Pos2, arc_radius: f32, local: Vec2) -> Pos2 {
    let theta = local.x / arc_radius;
    let (sin, cos) = theta.sin_cos();
    center + vec2(sin, -cos) * (arc_radius - local.y)
}

fn engrave_arc(
    painter: &egui::Painter,
    center: Pos2,
    arc_radius: f32,
    galley: &egui::epaint::Galley,
) {
    paint_arc(
        painter,
        center,
        arc_radius,
        galley,
        vec2(0.0, 2.0),
        Color32::from_white_alpha(70),
    );
    paint_arc(
        painter,
        center,
        arc_radius,
        galley,
        Vec2::ZERO,
        Color32::from_rgb(32, 36, 42),
    );
}

/// Galley meshes are straight. Each vertex is moved onto the ball's circle
/// so the baseline and the letter tops stay concentric with the rim.
fn paint_arc(
    painter: &egui::Painter,
    center: Pos2,
    arc_radius: f32,
    galley: &egui::epaint::Galley,
    offset: Vec2,
    color: Color32,
) {
    if galley.is_empty() || arc_radius < 1.0 {
        return;
    }
    let [tex_w, tex_h] = painter.fonts(|fonts| fonts.font_image_size());
    if tex_w == 0 || tex_h == 0 {
        return;
    }
    let uv_scale = vec2(1.0 / tex_w as f32, 1.0 / tex_h as f32);
    let origin = galley.rect.center().to_vec2();
    let mut mesh = Mesh::default();
    for row in &galley.rows {
        if row.visuals.mesh.is_empty() {
            continue;
        }
        let index_base = mesh.vertices.len() as u32;
        mesh.indices.extend(
            row.visuals
                .mesh
                .indices
                .iter()
                .map(|index| index + index_base),
        );
        let row_pos = row.pos.to_vec2();
        mesh.vertices
            .extend(row.visuals.mesh.vertices.iter().map(|vertex| {
                let local = row_pos + vertex.pos.to_vec2() - origin;
                Vertex {
                    pos: arc_point(center, arc_radius, local) + offset,
                    uv: pos2(vertex.uv.x * uv_scale.x, vertex.uv.y * uv_scale.y),
                    color,
                }
            }));
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn engrave(painter: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, size: f32) {
    let font = FontId::proportional(size);
    painter.text(
        pos + vec2(0.0, 1.0),
        anchor,
        text,
        font.clone(),
        Color32::from_white_alpha(48),
    );
    painter.text(pos, anchor, text, font, Color32::from_rgb(42, 46, 52));
}

fn paint_inset(painter: &egui::Painter, rect: Rect, hovered: bool) {
    let fill = if hovered {
        Color32::from_rgba_unmultiplied(18, 20, 24, 130)
    } else {
        Color32::from_rgba_unmultiplied(16, 18, 22, 70)
    };
    painter.rect_filled(rect, 11.0, fill);
    painter.rect_stroke(
        rect,
        11.0,
        Stroke::new(1.0, Color32::from_black_alpha(110)),
        StrokeKind::Inside,
    );
}

/// Control block in the lower left of the ball, kept inside the rim.
/// The right edge is the vertical center line. The bottom is as low as that
/// width can sit before the left corners would leave the circle.
fn centered_panel(center: Pos2, radius: f32) -> Rect {
    let inset = 28.0;
    let height = radius * 0.76;
    let width = radius * 0.45;
    let limit = width + inset;
    let dy_bottom = (radius * radius - limit * limit).max(0.0).sqrt();
    let bottom_y = center.y + dy_bottom;
    let top_y = bottom_y - height;
    Rect::from_min_max(pos2(center.x - width, top_y), pos2(center.x, bottom_y))
}

/// Largest rectangle inside the circle between two vertical positions.
/// `top` and `bottom` are offsets from the center in radii, positive downward.
fn chord_rect(center: Pos2, radius: f32, top: f32, bottom: f32) -> Rect {
    let y0 = center.y + radius * top;
    let y1 = center.y + radius * bottom;
    let inset = 8.0;
    let half = |y: f32| {
        let dy = y - center.y;
        (radius * radius - dy * dy).max(0.0).sqrt() - inset
    };
    let half_w = half(y0).min(half(y1)).max(36.0);
    Rect::from_min_max(pos2(center.x - half_w, y0), pos2(center.x + half_w, y1))
}

fn paint_controls(ui: &mut egui::Ui, params: &BouncingBallParams, setter: &ParamSetter) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        let picker_w = ((ui.available_width() - 36.0) / 3.0).clamp(72.0, 104.0);
        mode_picker(ui, "Trigger", &params.trigger, setter, picker_w);
        mode_picker(ui, "Source", &params.source, setter, picker_w);
        mode_picker(ui, "Direction", &params.direction, setter, picker_w);
    });
    ui.add_space(10.0);
    ui.columns(2, |columns| {
        metal_slider(&mut columns[0], &params.drop, setter);
        metal_slider(&mut columns[1], &params.bounciness, setter);
        metal_slider(&mut columns[0], &params.rest, setter);
        metal_slider(&mut columns[1], &params.hit, setter);
        metal_slider(&mut columns[0], &params.decay, setter);
        metal_slider(&mut columns[1], &params.rattle, setter);
        metal_slider(&mut columns[0], &params.mix, setter);
        metal_slider(&mut columns[1], &params.output, setter);
    });
    let mut looping = params.loop_sequence.value();
    if ui.checkbox(&mut looping, "Loop").changed() {
        setter.begin_set_parameter(&params.loop_sequence);
        setter.set_parameter(&params.loop_sequence, looping);
        setter.end_set_parameter(&params.loop_sequence);
    }
    ball_count_buttons(ui, params, setter);
}

/// Bold label centered over its drop-down.
fn mode_picker<T>(
    ui: &mut egui::Ui,
    label: &str,
    param: &EnumParam<T>,
    setter: &ParamSetter,
    width: f32,
) where
    T: Enum + PartialEq + Copy + 'static,
{
    ui.allocate_ui_with_layout(
        vec2(width, 52.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
            bold_label(ui.painter(), rect.center(), label, 16.0);
            enum_box(ui, param, setter, (width - 4.0).max(64.0));
        },
    );
}

/// Heavier label for the mode pickers.
fn bold_label(painter: &egui::Painter, pos: Pos2, text: &str, size: f32) {
    let font = FontId::proportional(size);
    let ink = Color32::from_rgb(255, 196, 72);
    for offset in [
        vec2(-1.6, 0.0),
        vec2(1.6, 0.0),
        vec2(0.0, -1.6),
        vec2(0.0, 1.6),
        vec2(1.2, 1.2),
        vec2(-1.2, 1.2),
    ] {
        painter.text(
            pos + offset,
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            Color32::from_black_alpha(240),
        );
    }
    for nudge in [0.0, 0.7, 1.2] {
        painter.text(
            pos + vec2(nudge, 0.0),
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            ink,
        );
    }
}

/// Four exclusive buttons. The number sits above each one, and the chosen
/// button takes an amber tint.
fn ball_count_buttons(ui: &mut egui::Ui, params: &BouncingBallParams, setter: &ParamSetter) {
    let selected = params.balls.value().clamp(1, 4);
    let (button_w, gap, height) = (42.0, 8.0, 50.0);
    let row_w = button_w * 4.0 + gap * 3.0;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let left = row.center().x - row_w * 0.5;
    {
        for count in 1..=4 {
            let x = left + (count - 1) as f32 * (button_w + gap);
            let rect = Rect::from_min_size(pos2(x, row.top()), vec2(button_w, height));
            let response = ui.interact(rect, ui.id().with(("ball-count", count)), Sense::click());
            let on = selected == count;
            let painter = ui.painter();
            slider_label(
                &painter,
                pos2(rect.center().x, rect.top() + 1.0),
                Align2::CENTER_TOP,
                &count.to_string(),
                13.0,
            );
            let button =
                Rect::from_min_max(pos2(rect.left(), rect.top() + 20.0), rect.right_bottom());
            paint_count_button(&painter, button, on, response.hovered());
            if response.clicked() && !on {
                setter.begin_set_parameter(&params.balls);
                setter.set_parameter(&params.balls, count);
                setter.end_set_parameter(&params.balls);
            }
        }
    }
}

fn paint_count_button(painter: &egui::Painter, rect: Rect, on: bool, hovered: bool) {
    let fill = if on {
        Color32::from_rgb(204, 122, 28)
    } else if hovered {
        Color32::from_rgba_unmultiplied(20, 22, 26, 150)
    } else {
        Color32::from_rgba_unmultiplied(16, 18, 22, 55)
    };
    painter.rect_filled(rect, 8.0, fill);
    let stroke = if on {
        Stroke::new(1.5, Color32::from_rgb(255, 196, 64))
    } else {
        Stroke::new(1.0, Color32::from_black_alpha(110))
    };
    painter.rect_stroke(rect, 8.0, stroke, StrokeKind::Inside);
}

/// A groove cut into the steel, with an obsidian bead as the handle.
fn metal_slider<P: Param>(ui: &mut egui::Ui, param: &P, setter: &ParamSetter) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click_and_drag());
    let response = response.on_hover_cursor(CursorIcon::Grab);

    let track = Rect::from_min_max(
        pos2(rect.left() + 7.0, rect.bottom() - 13.0),
        pos2(rect.right() - 7.0, rect.bottom() - 7.0),
    );
    let painter = ui.painter_at(rect);
    let name = param.name();
    let name_font = FontId::proportional(13.5);
    slider_label(
        &painter,
        rect.left_top() + vec2(2.0, 1.0),
        Align2::LEFT_TOP,
        name,
        13.5,
    );
    let name_width = ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(name.to_owned(), name_font, Color32::WHITE)
            .rect
            .width()
    });
    let name_rect = Rect::from_min_size(rect.left_top(), vec2(name_width + 8.0, 16.0));
    let name_hover = ui.interact(
        name_rect,
        ui.id().with(("slider-tip", name)),
        Sense::hover(),
    );
    if let Some(tip) = slider_tip(name) {
        name_hover.on_hover_text(tip);
    }
    slider_label(
        &painter,
        rect.right_top() + vec2(-2.0, 1.0),
        Align2::RIGHT_TOP,
        &param.to_string(),
        13.5,
    );

    painter.rect_filled(track, 3.0, Color32::from_rgba_unmultiplied(10, 12, 16, 150));
    painter.line_segment(
        [track.left_top(), track.right_top()],
        Stroke::new(1.0, Color32::from_black_alpha(120)),
    );
    painter.line_segment(
        [track.left_bottom(), track.right_bottom()],
        Stroke::new(1.0, Color32::from_white_alpha(45)),
    );

    let normalized = param.modulated_normalized_value().clamp(0.0, 1.0);
    let set_from_x = |x: f32| {
        let next = ((x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
        setter.set_parameter(param, param.preview_plain(next));
    };
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    } else if response.drag_started() {
        setter.begin_set_parameter(param);
        if let Some(pos) = response.interact_pointer_pos() {
            set_from_x(pos.x);
        }
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            set_from_x(pos.x);
        }
    } else if response.clicked() {
        setter.begin_set_parameter(param);
        if let Some(pos) = response.interact_pointer_pos() {
            set_from_x(pos.x);
        }
        setter.end_set_parameter(param);
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }

    let traveled = normalized;
    let knob_x = track.left() + traveled * track.width();
    let filled = Rect::from_min_max(track.min, pos2(knob_x, track.max.y));
    painter.rect_filled(
        filled,
        3.0,
        Color32::from_rgba_unmultiplied(90, 98, 110, 70),
    );
    paint_obsidian_ball(
        &painter,
        pos2(knob_x, track.center().y),
        vec2(7.5, 7.5),
        0.35,
        0.2,
    );
}

fn slider_tip(name: &str) -> Option<&'static str> {
    Some(match name {
        "Drop" => "Length of the first gap.",
        "Bounciness" => "How much of each gap is kept.",
        "Rest" => "Shortest gap, where the phrase settles.",
        "Hit" => "Length of each hit.",
        "Bounce Decay" => "How quickly later hits get quieter.",
        "Rattle" => "How long the ending chatter lasts.",
        "Mix" => "Dry signal against the bounced hits.",
        "Output" => "Level after the effect.",
        _ => return None,
    })
}

fn enum_box<T>(ui: &mut egui::Ui, param: &EnumParam<T>, setter: &ParamSetter, width: f32)
where
    T: Enum + PartialEq + Copy + 'static,
{
    let current = param.value();
    let names = T::variants();
    let selected = names.get(current.to_index()).copied().unwrap_or("");
    egui::ComboBox::from_id_salt(param.name())
        .selected_text(selected)
        .width(width)
        .show_ui(ui, |ui| {
            for (index, name) in names.iter().enumerate() {
                if ui
                    .selectable_label(index == current.to_index(), *name)
                    .clicked()
                {
                    setter.begin_set_parameter(param);
                    setter.set_parameter(param, T::from_index(index));
                    setter.end_set_parameter(param);
                }
            }
        });
}

struct DrawnBall {
    lane: f32,
    /// 0 on the floor, 1 at the top of the first drop.
    height: f32,
    impact: f32,
    /// 0..1 through the current flight, used to slide the highlight.
    flight: f32,
}

#[derive(Clone, Copy)]
struct Hop {
    elapsed: f32,
    duration: f32,
    apex: f32,
    falling: bool,
    impact: f32,
}

impl Hop {
    fn start(direction: Direction, drop: f32, rest: f32, index: usize, count: usize) -> Self {
        let mut hop = match direction {
            Direction::Rise => Self {
                elapsed: 0.0,
                duration: rest,
                apex: (rest / drop).powi(2).clamp(0.02, 1.0),
                falling: false,
                impact: 0.0,
            },
            Direction::Fall | Direction::RoundTrip => Self {
                elapsed: 0.0,
                duration: drop,
                apex: 1.0,
                falling: true,
                impact: 0.0,
            },
        };
        let stagger = if count <= 1 {
            0.0
        } else {
            drop * index as f32 / count as f32
        };
        hop.elapsed = stagger % hop.duration.max(0.001);
        hop
    }

    fn height(&self) -> f32 {
        let t = (self.elapsed / self.duration.max(0.001)).clamp(0.0, 1.0);
        4.0 * self.apex * t * (1.0 - t)
    }

    fn flight(&self) -> f32 {
        (self.elapsed / self.duration.max(0.001)).clamp(0.0, 1.0)
    }

    fn tick(&mut self, dt: f32, drop: f32, rest: f32, restitution: f32, direction: Direction) {
        let mut remain = dt;
        for _ in 0..8 {
            if remain <= 0.0 {
                break;
            }
            let left = (self.duration - self.elapsed).max(0.0);
            if remain < left {
                self.elapsed += remain;
                break;
            }
            remain -= left;
            self.elapsed = 0.0;
            self.impact = 1.0;
            self.land(drop, rest, restitution, direction);
        }
        self.impact = (self.impact - dt.min(0.05) * 3.2).max(0.0);
    }

    fn land(&mut self, drop: f32, rest: f32, restitution: f32, direction: Direction) {
        let restitution = restitution.clamp(0.5, 0.97);
        let rest = rest.max(0.001);
        let drop = drop.max(rest + 0.001);
        match direction {
            Direction::Fall => self.step_down(drop, rest, restitution, true),
            Direction::Rise => self.step_up(drop, rest, restitution, true),
            Direction::RoundTrip => {
                if self.falling {
                    self.step_down(drop, rest, restitution, false);
                } else {
                    self.step_up(drop, rest, restitution, false);
                }
            }
        }
    }

    fn step_down(&mut self, drop: f32, rest: f32, restitution: f32, restart: bool) {
        self.duration = (self.duration * restitution).max(rest);
        self.apex = (self.apex * restitution * restitution).max(0.015);
        if self.duration <= rest * 1.02 {
            if restart {
                self.duration = drop;
                self.apex = 1.0;
                self.falling = true;
            } else {
                self.falling = false;
                self.duration = rest;
            }
        }
    }

    fn step_up(&mut self, drop: f32, rest: f32, restitution: f32, restart: bool) {
        self.duration = (self.duration / restitution).min(drop);
        self.apex = (self.apex / (restitution * restitution)).min(1.0);
        if self.duration >= drop * 0.98 {
            if restart {
                self.duration = rest;
                self.apex = (rest / drop).powi(2).clamp(0.02, 1.0);
                self.falling = false;
            } else {
                self.falling = true;
                self.duration = drop;
                self.apex = 1.0;
            }
        }
    }
}

struct Preview {
    count: usize,
    direction: Direction,
    hops: [Hop; MAX_BALLS],
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            count: 0,
            direction: Direction::Fall,
            hops: [Hop::start(Direction::Fall, 0.42, 0.008, 0, 1); MAX_BALLS],
        }
    }
}

impl Preview {
    fn advance(
        &mut self,
        dt: f32,
        count: usize,
        direction: Direction,
        drop: f32,
        rest: f32,
        restitution: f32,
    ) -> [Option<DrawnBall>; MAX_BALLS] {
        if self.count != count || self.direction != direction {
            self.count = count;
            self.direction = direction;
            for index in 0..MAX_BALLS {
                self.hops[index] = Hop::start(direction, drop, rest, index, count);
            }
        }
        let mut drawn = [None, None, None, None];
        for index in 0..count {
            let hop = &mut self.hops[index];
            hop.tick(dt, drop, rest, restitution, direction);
            let lane = if count == 1 {
                0.0
            } else {
                index as f32 / (count as f32 - 1.0) - 0.5
            };
            drawn[index] = Some(DrawnBall {
                lane,
                height: hop.height(),
                impact: hop.impact,
                flight: hop.flight(),
            });
        }
        drawn
    }
}

fn paint_stage(painter: &egui::Painter, rect: Rect, balls: &[Option<DrawnBall>; MAX_BALLS]) {
    let floor_y = rect.top() + rect.height() * 0.82;
    let apex_y = rect.top() + rect.height() * 0.16;
    let travel = (floor_y - apex_y).max(40.0);
    let ball_r = (rect.height().min(rect.width()) * 0.22).clamp(14.0, 64.0);

    painter.add(Shape::ellipse_stroke(
        pos2(rect.center().x, floor_y),
        vec2(rect.width() * 0.34, 2.5),
        Stroke::new(1.5, Color32::from_black_alpha(70)),
    ));

    for ball in balls.iter().flatten() {
        let x = rect.center().x + ball.lane * rect.width() * 0.36;
        let lift = ball.height.clamp(0.0, 1.0);
        let center = pos2(x, floor_y - lift * travel);
        // Mass holds its shape until the hit, then settles instead of stretching.
        let squash = ball.impact * (1.0 - (lift / 0.08).min(1.0));
        let rx = ball_r * (1.0 + 0.05 * squash);
        let ry = ball_r * (1.0 - 0.08 * squash);

        let shadow_alpha = (120.0 * (1.0 - lift * 0.9)).clamp(18.0, 120.0) as u8;
        painter.add(Shape::ellipse_filled(
            pos2(x, floor_y + 2.0),
            vec2(rx * (0.72 + lift * 0.08), ball_r * 0.09),
            Color32::from_black_alpha(shadow_alpha),
        ));

        if ball.impact > 0.04 {
            let ring = ball_r * (1.02 + (1.0 - ball.impact) * 0.45);
            painter.add(Shape::ellipse_stroke(
                pos2(x, floor_y),
                vec2(ring, ring * 0.16),
                Stroke::new(1.2, Color32::from_white_alpha((ball.impact * 90.0) as u8)),
            ));
        }

        paint_obsidian_ball(painter, center, vec2(rx, ry), ball.flight, ball.impact);
    }
}

fn paint_chrome_ball(painter: &egui::Painter, center: Pos2, radius: Vec2, flight: f32) {
    paint_sphere(painter, center, radius, flight, steel_color);
}

fn paint_obsidian_ball(
    painter: &egui::Painter,
    center: Pos2,
    radius: Vec2,
    flight: f32,
    impact: f32,
) {
    let glow = (28.0 + impact * 55.0).clamp(0.0, 90.0);
    painter.add(Shape::ellipse_filled(
        center,
        radius * vec2(1.55, 1.55),
        Color32::from_rgba_unmultiplied(150, 158, 172, (glow * 0.28) as u8),
    ));
    painter.add(Shape::ellipse_filled(
        center,
        radius * vec2(1.2, 1.2),
        Color32::from_rgba_unmultiplied(196, 204, 214, (glow * 0.45) as u8),
    ));
    paint_sphere(painter, center, radius, flight, obsidian_color);
}

fn paint_sphere(
    painter: &egui::Painter,
    center: Pos2,
    radius: Vec2,
    flight: f32,
    color: fn(f32, f32, f32) -> Color32,
) {
    let rings = 16;
    let segments = 42;
    let mut mesh = Mesh::default();
    let glide = (0.5 - flight) * 0.18;
    mesh.colored_vertex(center, color(0.0, 0.0, glide));
    for ring in 1..=rings {
        let radius_n = ring as f32 / rings as f32;
        for segment in 0..segments {
            let angle = segment as f32 / segments as f32 * TAU;
            let nx = angle.cos() * radius_n;
            let ny = angle.sin() * radius_n;
            let point = center + vec2(nx * radius.x, ny * radius.y);
            mesh.colored_vertex(point, color(nx, ny, glide));
        }
    }
    for ring in 0..rings {
        for segment in 0..segments {
            let next = (segment + 1) % segments;
            let outer_a = vertex_index(ring + 1, segment, segments);
            let outer_b = vertex_index(ring + 1, next, segments);
            if ring == 0 {
                mesh.add_triangle(0, outer_a, outer_b);
            } else {
                let inner_a = vertex_index(ring, segment, segments);
                let inner_b = vertex_index(ring, next, segments);
                mesh.add_triangle(inner_a, outer_a, outer_b);
                mesh.add_triangle(inner_a, outer_b, inner_b);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn vertex_index(ring: usize, segment: usize, segments: usize) -> u32 {
    (1 + (ring - 1) * segments + segment) as u32
}

/// Gray steel for the window itself. The lower half sits a little darker
/// so the engraved controls have a quieter field.
fn steel_color(nx: f32, ny: f32, glide: f32) -> Color32 {
    let z = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
    let (lx, ly, lz) = normalize(-0.38 + glide, -0.55, 0.74);
    let diffuse = (nx * lx + ny * ly + z * lz).max(0.0);
    let (hx, hy, hz) = normalize(lx, ly, lz + 1.0);
    let spec = (nx * hx + ny * hy + z * hz).max(0.0);
    let sharp = spec.powf(48.0);
    let broad = spec.powf(8.0);
    let fresnel = (1.0 - z).powf(1.8);
    let belly = (ny * 0.5 + 0.5).clamp(0.0, 1.0);
    let shade = 1.0 - 0.22 * belly;

    let r = (34.0 + 96.0 * diffuse + 120.0 * sharp + 36.0 * broad + 28.0 * fresnel) * shade;
    let g = (36.0 + 98.0 * diffuse + 124.0 * sharp + 38.0 * broad + 30.0 * fresnel) * shade;
    let b = (40.0 + 104.0 * diffuse + 128.0 * sharp + 44.0 * broad + 34.0 * fresnel) * shade;
    Color32::from_rgb(byte(r), byte(g), byte(b))
}

/// Polished obsidian: almost black, with a cold rim and a small highlight.
fn obsidian_color(nx: f32, ny: f32, glide: f32) -> Color32 {
    let z = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
    let (lx, ly, lz) = normalize(-0.35 + glide, -0.62, 0.7);
    let diffuse = (nx * lx + ny * ly + z * lz).max(0.0);
    let (hx, hy, hz) = normalize(lx, ly, lz + 1.0);
    let spec = (nx * hx + ny * hy + z * hz).max(0.0);
    let sharp = spec.powf(70.0);
    let broad = spec.powf(14.0);
    let fresnel = (1.0 - z).powf(2.2);

    let r = 3.0 + 14.0 * diffuse + 210.0 * sharp + 18.0 * broad + 22.0 * fresnel;
    let g = 4.0 + 16.0 * diffuse + 214.0 * sharp + 22.0 * broad + 28.0 * fresnel;
    let b = 6.0 + 20.0 * diffuse + 220.0 * sharp + 30.0 * broad + 42.0 * fresnel;
    Color32::from_rgb(byte(r), byte(g), byte(b))
}

fn normalize(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let length = (x * x + y * y + z * z).sqrt().max(1.0e-6);
    (x / length, y / length, z / length)
}

fn byte(value: f32) -> u8 {
    value.clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_hops_hasten() {
        let mut hop = Hop::start(Direction::Fall, 0.40, 0.05, 0, 1);
        let mut gaps = Vec::new();
        for _ in 0..5 {
            gaps.push(hop.duration);
            hop.tick(hop.duration, 0.40, 0.05, 0.8, Direction::Fall);
        }
        for pair in gaps.windows(2) {
            let ratio = pair[1] / pair[0];
            assert!(
                (ratio - 0.8).abs() < 1.0e-3,
                "visual gap ratio {ratio} from {} -> {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn control_panel_sits_on_the_left_of_the_ball() {
        let center = pos2(320.0, 320.0);
        let radius = 300.0;
        let panel = centered_panel(center, radius);
        assert!(panel.right() <= center.x + 0.5);
        assert!(panel.left() < center.x - radius * 0.4);
        assert!(panel.center().y > center.y);
        assert!(panel.bottom() <= center.y + radius - 8.0);
        for corner in [
            panel.left_top(),
            panel.right_top(),
            panel.left_bottom(),
            panel.right_bottom(),
        ] {
            let dx = corner.x - center.x;
            let dy = corner.y - center.y;
            assert!(
                dx * dx + dy * dy <= radius * radius + 0.5,
                "corner {corner:?} fell outside the ball"
            );
        }
    }

    #[test]
    fn controls_stay_inside_the_ball() {
        let center = pos2(320.0, 320.0);
        let radius = 300.0;
        let rect = chord_rect(center, radius, 0.02, 0.78);
        for corner in [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ] {
            let dx = corner.x - center.x;
            let dy = corner.y - center.y;
            assert!(
                dx * dx + dy * dy <= radius * radius + 0.5,
                "corner {corner:?} fell outside the ball"
            );
        }
    }

    #[test]
    fn title_arc_follows_the_ball() {
        let center = pos2(320.0, 320.0);
        let arc_r = 260.0;
        let crown = arc_point(center, arc_r, Vec2::ZERO);
        assert!((crown.x - center.x).abs() < 1.0e-3);
        assert!((crown.y - (center.y - arc_r)).abs() < 1.0e-3);

        let side = arc_point(center, arc_r, vec2(arc_r * 0.5, 0.0));
        let offset = side - center;
        assert!((offset.length() - arc_r).abs() < 1.0e-2);
        assert!(side.y > crown.y);
        assert!(side.x > center.x);

        let top = arc_point(center, arc_r, vec2(0.0, -20.0));
        assert!(((top - center).length() - (arc_r + 20.0)).abs() < 1.0e-2);
    }

    #[test]
    fn curved_title_stays_above_the_preview() {
        let radius = 320.0;
        // Round Bound: "Bouncing Boll" is about 5.33 em wide and 1 em tall.
        let (size, arc_r) = title_metrics(radius, 533.0, 100.0);
        let half_h = size * 0.5;
        let half_w = 5.33 * size * 0.5;
        let theta = half_w / arc_r;
        assert!(
            (theta - TITLE_HALF_ANGLE).abs() < 0.05,
            "title span {theta} rad drifted from the rim angle"
        );
        assert!(
            arc_r + half_h < radius - 8.0,
            "title crosses the rim at font size {size}"
        );
        let end_y = -(arc_r - half_h) * theta.cos();
        assert!(
            end_y < -radius * 0.58,
            "title ends drop into the preview, end_y={end_y}"
        );
    }

    #[test]
    fn ball_leaves_the_floor_and_returns() {
        let hop = Hop::start(Direction::Fall, 0.40, 0.05, 0, 1);
        assert!(hop.height() < 0.001);
        let mut mid = hop;
        mid.elapsed = hop.duration * 0.5;
        assert!((mid.height() - 1.0).abs() < 1.0e-3);
        let mut end = hop;
        end.elapsed = hop.duration;
        assert!(end.height() < 0.001);
    }
}
