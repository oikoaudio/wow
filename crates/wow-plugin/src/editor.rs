use std::{
    f32::consts::PI,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
    time::Duration,
};

use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Id, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Vec2,
};
use nice_plug::{context::gui::GuiContext, params::Param};
use nice_plug_egui::{NiceEguiApp, baseview::HandlerError};

use crate::WowParams;

pub(crate) const DISPLAY_REFRESH_HZ: f64 = 480.0;
const DISPLAY_POINTS: usize = 2048;
pub(crate) const EDITOR_WIDTH: f32 = 500.0;
pub(crate) const EDITOR_HEIGHT: f32 = 390.0;
const UI_SCALE_STEPS: [f32; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
const PROJECT_URL: &str = "https://github.com/oikoaudio/wow";

pub(crate) struct ModulationDisplay {
    left: [AtomicU32; DISPLAY_POINTS],
    right: [AtomicU32; DISPLAY_POINTS],
    cursor: AtomicUsize,
}

impl Default for ModulationDisplay {
    fn default() -> Self {
        Self {
            left: [const { AtomicU32::new(0) }; DISPLAY_POINTS],
            right: [const { AtomicU32::new(0) }; DISPLAY_POINTS],
            cursor: AtomicUsize::new(0),
        }
    }
}

impl ModulationDisplay {
    #[inline]
    pub(crate) fn push(&self, left: f32, right: f32) {
        let sequence = self.cursor.load(Ordering::Relaxed);
        let slot = sequence % DISPLAY_POINTS;
        self.left[slot].store(left.to_bits(), Ordering::Relaxed);
        self.right[slot].store(right.to_bits(), Ordering::Relaxed);
        self.cursor
            .store(sequence.wrapping_add(1), Ordering::Release);
    }

    pub(crate) fn clear(&self) {
        self.cursor.store(0, Ordering::Release);
    }

    fn snapshot(&self) -> (Vec<f32>, Vec<f32>) {
        let cursor = self.cursor.load(Ordering::Acquire);
        let count = cursor.min(DISPLAY_POINTS);
        let start = cursor.saturating_sub(count);
        let mut left = Vec::with_capacity(count);
        let mut right = Vec::with_capacity(count);
        for sequence in start..cursor {
            let slot = sequence % DISPLAY_POINTS;
            left.push(f32::from_bits(self.left[slot].load(Ordering::Relaxed)));
            right.push(f32::from_bits(self.right[slot].load(Ordering::Relaxed)));
        }
        (left, right)
    }
}

pub struct WowEditor {
    params: Arc<WowParams>,
    display: Arc<ModulationDisplay>,
    gui_context: Option<GuiContext>,
    dark: Arc<AtomicBool>,
    about_open: bool,
}

impl WowEditor {
    pub(crate) fn new(params: Arc<WowParams>, display: Arc<ModulationDisplay>) -> Self {
        Self {
            params,
            display,
            gui_context: None,
            dark: Arc::new(AtomicBool::new(true)),
            about_open: false,
        }
    }
}

impl NiceEguiApp for WowEditor {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), HandlerError> {
        self.gui_context = Some(nice_gui_ctx);
        apply_theme(&egui_ctx, self.dark.load(Ordering::Relaxed));
        let settled_scale = closest_ui_scale(self.params.ui_scale.get());
        self.params.ui_scale.set(settled_scale);
        egui_ctx.set_zoom_factor(settled_scale);
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut nice_plug_egui::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        let dark = self.dark.load(Ordering::Relaxed);
        let palette = Palette::new(dark);
        let setter = self
            .gui_context
            .as_ref()
            .expect("the GUI context is set before drawing")
            .param_setter();

        ui.set_min_size(Vec2::new(EDITOR_WIDTH, EDITOR_HEIGHT));
        ui.painter().rect_filled(ui.max_rect(), 0.0, palette.panel);

        header(ui, palette, &self.dark, &mut self.about_open);
        motion_display(
            ui,
            palette,
            &self.display,
            crate::display_rate_scale(&self.params),
        );

        Frame::NONE
            .inner_margin(egui::Margin::symmetric(8, 15))
            .show(ui, |ui| {
                ui.columns(4, |columns| {
                    knob(
                        &mut columns[0],
                        "WOW RATE",
                        &self.params.rate,
                        &setter,
                        palette,
                    );
                    knob(
                        &mut columns[1],
                        "FLUTTER RATE",
                        &self.params.flutter_rate,
                        &setter,
                        palette,
                    );
                    knob(
                        &mut columns[2],
                        "WOW / FLUTTER",
                        &self.params.wow_flutter,
                        &setter,
                        palette,
                    );
                    knob(
                        &mut columns[3],
                        "AMOUNT",
                        &self.params.amount,
                        &setter,
                        palette,
                    );
                });

                ui.add_space(8.0);
                ui.columns(2, |columns| {
                    Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 0,
                            right: 14,
                            top: 0,
                            bottom: 0,
                        })
                        .show(&mut columns[0], |ui| {
                            parameter_slider(ui, "DRIFT", &self.params.drift, &setter, palette);
                        });
                    Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 14,
                            right: 0,
                            top: 0,
                            bottom: 0,
                        })
                        .show(&mut columns[1], |ui| {
                            parameter_slider(
                                ui,
                                "L/R PHASE OFFSET",
                                &self.params.stereo,
                                &setter,
                                palette,
                            );
                        });
                });
            });

        footer(ui, palette, &self.params, &setter);
        if self.about_open {
            about_popup(ui, palette, &mut self.about_open, &self.params.ui_scale);
        }
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }
}

fn request_settled_scale(context: &egui::Context, scale: f32) {
    let scale = closest_ui_scale(scale);
    context.set_zoom_factor(scale);
    context.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
        EDITOR_WIDTH,
        EDITOR_HEIGHT,
    )));
}

pub(crate) fn closest_ui_scale(scale: f32) -> f32 {
    UI_SCALE_STEPS
        .into_iter()
        .min_by(|left, right| (scale - left).abs().total_cmp(&(scale - right).abs()))
        .unwrap_or(1.0)
}

#[derive(Clone, Copy)]
struct Palette {
    page: Color32,
    panel: Color32,
    ink: Color32,
    muted: Color32,
    rule: Color32,
    track: Color32,
    left: Color32,
    right: Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                page: Color32::from_rgb(23, 24, 23),
                panel: Color32::from_rgb(32, 34, 32),
                ink: Color32::from_rgb(238, 238, 232),
                muted: Color32::from_rgb(167, 170, 161),
                rule: Color32::from_rgb(58, 61, 56),
                track: Color32::from_rgb(48, 51, 47),
                left: Color32::from_rgb(157, 180, 255),
                right: Color32::from_rgb(255, 149, 125),
            }
        } else {
            Self {
                page: Color32::from_rgb(232, 232, 229),
                panel: Color32::from_rgb(246, 246, 242),
                ink: Color32::from_rgb(22, 23, 21),
                muted: Color32::from_rgb(74, 77, 70),
                rule: Color32::from_rgb(201, 203, 196),
                track: Color32::from_rgb(216, 218, 211),
                left: Color32::from_rgb(40, 94, 232),
                right: Color32::from_rgb(211, 78, 46),
            }
        }
    }
}

fn apply_theme(context: &egui::Context, dark: bool) {
    let palette = Palette::new(dark);
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    context.set_theme(theme);
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.override_text_color = Some(palette.ink);
    visuals.selection.bg_fill = palette.left;
    visuals.selection.stroke = Stroke::new(1.0, palette.ink);
    visuals.widgets.noninteractive.bg_fill = palette.panel;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.ink);
    visuals.widgets.inactive.bg_fill = palette.page;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.muted);
    visuals.widgets.hovered.bg_fill = palette.track;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.ink);
    visuals.window_corner_radius = CornerRadius::same(6);
    context.set_visuals_of(theme, visuals);

    let mut style = (*context.style_of(theme)).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(7.0, 4.0);
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(13.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(11.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(10.0, egui::FontFamily::Proportional),
    );
    context.set_style_of(theme, style);
}

fn header(ui: &mut egui::Ui, palette: Palette, dark: &AtomicBool, about_open: &mut bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        "WOW",
        FontId::new(15.0, egui::FontFamily::Proportional),
        palette.ink,
    );

    let brand_rect = Rect::from_center_size(rect.center(), Vec2::new(92.0, 32.0));
    let brand = ui.interact(brand_rect, Id::new("oiko-audio-about"), Sense::click());
    if brand.clicked() {
        *about_open = !*about_open;
    }
    if brand.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().text(
        brand_rect.center(),
        Align2::CENTER_CENTER,
        "OIKO AUDIO",
        FontId::new(10.0, egui::FontFamily::Proportional),
        if brand.hovered() {
            palette.ink
        } else {
            palette.muted
        },
    );

    let current_dark = dark.load(Ordering::Relaxed);
    let theme_rect = Rect::from_min_size(
        Pos2::new(rect.right() - 48.0, rect.top() + 9.0),
        Vec2::new(32.0, 28.0),
    );
    let theme = ui.interact(theme_rect, Id::new("theme-toggle"), Sense::click());
    if theme.clicked() {
        dark.store(!current_dark, Ordering::Relaxed);
    }
    if theme.hovered() {
        ui.painter().rect_filled(theme_rect, 3.0, palette.track);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    draw_theme_icon(
        ui,
        theme_rect.center(),
        current_dark,
        theme.hovered(),
        palette,
    );
    let requested_dark = dark.load(Ordering::Relaxed);
    if requested_dark != current_dark {
        apply_theme(ui.ctx(), requested_dark);
    }
}

fn draw_theme_icon(ui: &mut egui::Ui, center: Pos2, dark: bool, hovered: bool, palette: Palette) {
    let color = if hovered { palette.ink } else { palette.muted };
    if dark {
        ui.painter().circle_filled(center, 6.0, color);
        let mask = if hovered {
            palette.track
        } else {
            palette.panel
        };
        ui.painter()
            .circle_filled(center + Vec2::new(3.0, -2.0), 5.5, mask);
    } else {
        ui.painter()
            .circle_stroke(center, 4.5, Stroke::new(1.4, color));
        for index in 0..8 {
            let angle = index as f32 * PI / 4.0;
            let direction = Vec2::angled(angle);
            ui.painter().line_segment(
                [center + direction * 7.0, center + direction * 9.0],
                Stroke::new(1.2, color),
            );
        }
    }
}

fn motion_display(
    ui: &mut egui::Ui,
    palette: Palette,
    display: &ModulationDisplay,
    display_scale: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 106.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette.page);
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, palette.rule),
    );
    let (left, right) = display.snapshot();
    if left.len() < 32 {
        return;
    }
    let peak = left
        .iter()
        .chain(&right)
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max)
        .max(1.0e-5);
    let plot = rect.shrink2(Vec2::new(0.0, 8.0));
    let stereo_is_visible = left
        .iter()
        .zip(&right)
        .any(|(left, right)| (left - right).abs() > peak * 1.0e-3);
    if stereo_is_visible {
        draw_history(ui, plot, &right, display_scale, palette.right, 1.35);
    }
    draw_history(ui, plot, &left, display_scale, palette.left, 1.6);
}

fn draw_history(ui: &egui::Ui, rect: Rect, values: &[f32], scale: f32, color: Color32, width: f32) {
    let last = (values.len() - 1) as f32;
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = rect.left() + index as f32 / last * rect.width();
            let y = rect.center().y - (*value / scale).clamp(-1.0, 1.0) * rect.height() * 0.43;
            Pos2::new(x, y)
        })
        .collect();
    ui.painter()
        .add(Shape::line(points, Stroke::new(width, color)));
}

fn knob<P: Param>(
    ui: &mut egui::Ui,
    label: &str,
    param: &P,
    setter: &nice_plug::context::gui::ParamSetter<'_>,
    palette: Palette,
) {
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(label).size(10.0).color(palette.muted));
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(58.0), Sense::click_and_drag());
        parameter_drag(ui, &response, param, setter, 0.0045);
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        let normalized = param.modulated_normalized_value().clamp(0.0, 1.0);
        let center = rect.center();
        ui.painter().circle_filled(center, 28.0, palette.page);
        ui.painter()
            .circle_stroke(center, 28.0, Stroke::new(1.0, palette.rule));
        let angle = PI * 0.75 + normalized * PI * 1.5;
        let inner = center + Vec2::angled(angle) * 7.0;
        let outer = center + Vec2::angled(angle) * 22.0;
        ui.painter()
            .line_segment([inner, outer], Stroke::new(2.0, palette.left));
        ui.label(
            egui::RichText::new(param.to_string())
                .size(11.5)
                .color(palette.ink),
        );
    });
}

fn parameter_slider<P: Param>(
    ui: &mut egui::Ui,
    label: &str,
    param: &P,
    setter: &nice_plug::context::gui::ParamSetter<'_>,
    palette: Palette,
) {
    let (label_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    ui.painter().text(
        Pos2::new(label_rect.left() + 8.0, label_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(10.0, egui::FontFamily::Proportional),
        palette.muted,
    );
    ui.painter().text(
        Pos2::new(label_rect.right() - 8.0, label_rect.center().y),
        Align2::RIGHT_CENTER,
        param.to_string(),
        FontId::new(10.5, egui::FontFamily::Proportional),
        palette.ink,
    );
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 24.0),
        Sense::click_and_drag(),
    );
    let value_range = rect.shrink2(Vec2::new(8.0, 0.0));
    parameter_absolute(ui, &response, value_range, param, setter);
    let track = Rect::from_center_size(value_range.center(), Vec2::new(value_range.width(), 4.0));
    ui.painter().rect_filled(track, 2.0, palette.track);
    let normalized = param.modulated_normalized_value().clamp(0.0, 1.0);
    let x = value_range.left() + normalized * value_range.width();
    let filled = Rect::from_min_max(track.left_top(), Pos2::new(x, track.bottom()));
    ui.painter().rect_filled(filled, 2.0, palette.left);
    let thumb = Pos2::new(x, rect.center().y);
    ui.painter().circle_filled(thumb, 7.5, palette.track);
    ui.painter().circle_stroke(
        thumb,
        7.5,
        Stroke::new(
            1.0,
            if response.hovered() || response.dragged() {
                palette.muted
            } else {
                palette.rule
            },
        ),
    );
    ui.painter().circle_filled(thumb, 2.75, palette.left);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn parameter_drag<P: Param>(
    ui: &egui::Ui,
    response: &Response,
    param: &P,
    setter: &nice_plug::context::gui::ParamSetter<'_>,
    sensitivity: f32,
) {
    let memory_id = response.id.with("drag-start");
    if response.drag_started() {
        setter.begin_set_parameter(param);
        ui.data_mut(|data| data.insert_temp(memory_id, param.unmodulated_normalized_value()));
    }
    if response.dragged() {
        let start = ui
            .data(|data| data.get_temp::<f32>(memory_id))
            .unwrap_or_else(|| param.unmodulated_normalized_value());
        let delta = response.total_drag_delta().unwrap_or_default();
        let movement = -delta.y + delta.x * 0.35;
        let fine = if ui.input(|input| input.modifiers.shift) {
            0.2
        } else {
            1.0
        };
        let normalized = (start + movement * sensitivity * fine).clamp(0.0, 1.0);
        setter.set_parameter(param, param.preview_plain(normalized));
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    }
}

fn parameter_absolute<P: Param>(
    _ui: &egui::Ui,
    response: &Response,
    value_range: Rect,
    param: &P,
    setter: &nice_plug::context::gui::ParamSetter<'_>,
) {
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
        return;
    }
    if response.drag_started() {
        setter.begin_set_parameter(param);
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let normalized = ((pointer.x - value_range.left()) / value_range.width()).clamp(0.0, 1.0);
        setter.set_parameter(param, param.preview_plain(normalized));
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let normalized = ((pointer.x - value_range.left()) / value_range.width()).clamp(0.0, 1.0);
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.preview_plain(normalized));
        setter.end_set_parameter(param);
    }
}

fn footer(
    ui: &mut egui::Ui,
    palette: Palette,
    params: &WowParams,
    setter: &nice_plug::context::gui::ParamSetter<'_>,
) {
    let rect = Rect::from_min_max(
        Pos2::new(ui.max_rect().left(), ui.max_rect().bottom() - 30.0),
        ui.max_rect().right_bottom(),
    );
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, palette.rule),
    );
    let height = 20.0;
    let seed_rect = Rect::from_min_size(
        Pos2::new(rect.right() - 106.0, rect.center().y - height * 0.5),
        Vec2::new(90.0, height),
    );
    let range_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.center().y),
        Vec2::new(176.0, height),
    );
    let quality_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 16.0, rect.center().y - height * 0.5),
        Vec2::new(120.0, height),
    );
    footer_param(
        ui,
        quality_rect,
        "QUALITY",
        72.0,
        &params.quality,
        setter,
        palette,
    );
    footer_param(
        ui,
        range_rect,
        "PITCH RANGE",
        104.0,
        &params.depth_behavior,
        setter,
        palette,
    );
    footer_param(
        ui,
        seed_rect,
        "SEED",
        58.0,
        &params.random_seed,
        setter,
        palette,
    );
}

fn footer_param<P: Param>(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    value_width: f32,
    param: &P,
    setter: &nice_plug::context::gui::ParamSetter<'_>,
    palette: Palette,
) {
    let value_rect = Rect::from_min_max(
        Pos2::new(rect.right() - value_width, rect.top() + 1.0),
        Pos2::new(rect.right(), rect.bottom() - 1.0),
    );
    let left_button = Rect::from_min_max(
        value_rect.left_top(),
        Pos2::new(value_rect.left() + 20.0, value_rect.bottom()),
    );
    let right_button = Rect::from_min_max(
        Pos2::new(value_rect.right() - 20.0, value_rect.top()),
        value_rect.right_bottom(),
    );
    let whole = ui.interact(value_rect, Id::new(("footer-param", label)), Sense::click());
    let previous = ui.interact(
        left_button,
        Id::new(("footer-param-prev", label)),
        Sense::click(),
    );
    let next = ui.interact(
        right_button,
        Id::new(("footer-param-next", label)),
        Sense::click(),
    );
    let normalized = param.unmodulated_normalized_value();
    let can_go_previous = normalized > 1.0e-6;
    let can_go_next = normalized < 1.0 - 1.0e-6;
    let hovered = whole.hovered() || previous.hovered() || next.hovered();
    ui.painter().rect_filled(
        value_rect,
        3.0,
        if hovered { palette.track } else { palette.page },
    );
    ui.painter().text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(9.0, egui::FontFamily::Proportional),
        palette.muted,
    );
    ui.painter().text(
        Pos2::new(value_rect.left() + 7.0, value_rect.center().y),
        Align2::LEFT_CENTER,
        "‹",
        FontId::new(12.0, egui::FontFamily::Proportional),
        if !can_go_previous {
            palette.rule
        } else if previous.hovered() {
            palette.ink
        } else {
            palette.muted
        },
    );
    ui.painter().text(
        Pos2::new(value_rect.right() - 7.0, value_rect.center().y),
        Align2::RIGHT_CENTER,
        "›",
        FontId::new(12.0, egui::FontFamily::Proportional),
        if !can_go_next {
            palette.rule
        } else if next.hovered() {
            palette.ink
        } else {
            palette.muted
        },
    );
    ui.painter().text(
        value_rect.center(),
        Align2::CENTER_CENTER,
        param.to_string(),
        FontId::new(9.5, egui::FontFamily::Proportional),
        palette.ink,
    );
    if (previous.hovered() && can_go_previous)
        || (next.hovered() && can_go_next)
        || (whole.hovered() && can_go_next)
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if can_go_next && (next.clicked() || (whole.clicked() && !previous.clicked())) {
        setter.begin_set_parameter(param);
        let value = param.next_step(param.unmodulated_plain_value(), false);
        setter.set_parameter(param, value);
        setter.end_set_parameter(param);
    } else if can_go_previous && (previous.clicked() || whole.secondary_clicked()) {
        setter.begin_set_parameter(param);
        let value = param.previous_step(param.unmodulated_plain_value(), false);
        setter.set_parameter(param, value);
        setter.end_set_parameter(param);
    }
}

fn about_popup(
    ui: &mut egui::Ui,
    palette: Palette,
    open: &mut bool,
    ui_scale: &crate::UiScaleState,
) {
    let size = Vec2::new(286.0, 244.0);
    let rect = Rect::from_min_size(Pos2::new(ui.max_rect().right() - size.x - 12.0, 50.0), size);
    ui.painter().rect(
        rect,
        5.0,
        palette.panel,
        Stroke::new(1.0, palette.rule),
        StrokeKind::Outside,
    );
    let close_rect =
        Rect::from_min_size(rect.right_top() + Vec2::new(-30.0, 6.0), Vec2::splat(24.0));
    let close = ui.interact(close_rect, Id::new("close-about"), Sense::click());
    if close.clicked() {
        *open = false;
    }
    ui.painter().text(
        close_rect.center(),
        Align2::CENTER_CENTER,
        "×",
        FontId::new(16.0, egui::FontFamily::Proportional),
        palette.muted,
    );
    ui.painter().text(
        rect.left_top() + Vec2::new(15.0, 16.0),
        Align2::LEFT_TOP,
        "OIKO WOW",
        FontId::new(14.0, egui::FontFamily::Proportional),
        palette.ink,
    );
    ui.painter().text(
        rect.left_top() + Vec2::new(15.0, 45.0),
        Align2::LEFT_TOP,
        format!("Version {}", env!("CARGO_PKG_VERSION")),
        FontId::new(11.0, egui::FontFamily::Proportional),
        palette.muted,
    );
    ui.painter().text(
        rect.left_top() + Vec2::new(15.0, 67.0),
        Align2::LEFT_TOP,
        "Free and open source",
        FontId::new(11.0, egui::FontFamily::Proportional),
        palette.muted,
    );
    ui.painter().text(
        rect.left_top() + Vec2::new(15.0, 89.0),
        Align2::LEFT_TOP,
        "Created by David Fredman",
        FontId::new(11.0, egui::FontFamily::Proportional),
        palette.muted,
    );

    let link_rect = Rect::from_min_size(
        rect.left_top() + Vec2::new(11.0, 106.0),
        Vec2::new(190.0, 22.0),
    );
    let link = ui.interact(link_rect, Id::new("about-project-url"), Sense::click());
    if link.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(PROJECT_URL));
    }
    if link.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().text(
        link_rect.left_center() + Vec2::new(4.0, 0.0),
        Align2::LEFT_CENTER,
        "github.com/oikoaudio/wow",
        FontId::new(11.0, egui::FontFamily::Proportional),
        if link.hovered() {
            palette.ink
        } else {
            palette.left
        },
    );
    ui.painter().text(
        rect.left_top() + Vec2::new(15.0, 159.0),
        Align2::LEFT_TOP,
        "INTERFACE SCALE",
        FontId::new(9.0, egui::FontFamily::Proportional),
        palette.muted,
    );
    let scale_row = Rect::from_min_size(
        rect.left_top() + Vec2::new(15.0, 177.0),
        Vec2::new(rect.width() - 30.0, 24.0),
    );
    let gap = 3.0;
    let button_width =
        (scale_row.width() - gap * (UI_SCALE_STEPS.len() - 1) as f32) / UI_SCALE_STEPS.len() as f32;
    let current_scale = ui_scale.get();
    for (index, scale) in UI_SCALE_STEPS.into_iter().enumerate() {
        let button_rect = Rect::from_min_size(
            Pos2::new(
                scale_row.left() + index as f32 * (button_width + gap),
                scale_row.top(),
            ),
            Vec2::new(button_width, scale_row.height()),
        );
        let response = ui.interact(
            button_rect,
            Id::new(("interface-scale", index)),
            Sense::click(),
        );
        let selected = (scale - current_scale).abs() < 0.001;
        ui.painter().rect_filled(
            button_rect,
            2.0,
            if selected {
                mix_color(palette.track, palette.left, 0.18)
            } else if response.hovered() {
                palette.track
            } else {
                palette.page
            },
        );
        ui.painter().rect_stroke(
            button_rect,
            2.0,
            Stroke::new(1.0, if selected { palette.left } else { palette.rule }),
            StrokeKind::Inside,
        );
        ui.painter().text(
            button_rect.center(),
            Align2::CENTER_CENTER,
            format!("{}", (scale * 100.0) as u32),
            FontId::new(8.0, egui::FontFamily::Proportional),
            if selected || response.hovered() {
                palette.ink
            } else {
                palette.muted
            },
        );
        if response.clicked() {
            ui_scale.set(scale);
            request_settled_scale(ui.ctx(), scale);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    ui.painter().text(
        rect.left_top() + Vec2::new(15.0, 220.0),
        Align2::LEFT_TOP,
        "MIT License",
        FontId::new(11.0, egui::FontFamily::Proportional),
        palette.muted,
    );
}

fn mix_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |from: u8, to: u8| (from as f32 + (to as f32 - from as f32) * amount) as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_scale_snaps_to_the_nearest_quarter_step() {
        assert_eq!(closest_ui_scale(0.51), 0.5);
        assert_eq!(closest_ui_scale(1.13), 1.25);
        assert_eq!(closest_ui_scale(1.88), 2.0);
    }

    #[test]
    fn display_history_is_ordered_and_bounded() {
        let display = ModulationDisplay::default();
        for index in 0..DISPLAY_POINTS + 7 {
            display.push(index as f32, -(index as f32));
        }
        let (left, right) = display.snapshot();
        assert_eq!(left.len(), DISPLAY_POINTS);
        assert_eq!(left[0], 7.0);
        assert_eq!(left[DISPLAY_POINTS - 1], (DISPLAY_POINTS + 6) as f32);
        assert_eq!(right[0], -7.0);
    }

    #[test]
    fn clearing_display_hides_old_samples() {
        let display = ModulationDisplay::default();
        display.push(0.1, -0.1);
        display.clear();
        let (left, right) = display.snapshot();
        assert!(left.is_empty());
        assert!(right.is_empty());
    }
}
