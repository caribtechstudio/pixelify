//! Shared visual tokens and accessible vector components for the desktop studio.
use eframe::egui::*;
use lucide_static_svg::Icon as LucideIcon;

pub const INK: Color32 = Color32::from_rgb(26, 33, 52);
pub const MUTED: Color32 = Color32::from_rgb(112, 121, 141);
pub const BLUE: Color32 = Color32::from_rgb(69, 88, 230);
pub const TINT: Color32 = Color32::from_rgb(238, 240, 255);
pub const BG: Color32 = Color32::from_rgb(247, 248, 251);
pub const LINE: Color32 = Color32::from_rgb(231, 234, 241);
pub const GREEN: Color32 = Color32::from_rgb(49, 148, 112);

pub fn configure(ctx: &Context) {
    egui_extras::install_image_loaders(ctx);
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter.ttf"
        ))),
    );
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    ctx.set_fonts(fonts);
    let mut style = Style::default();
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style.spacing.item_spacing = vec2(10.0, 10.0);
    style.spacing.button_padding = vec2(14.0, 10.0);
    style.spacing.interact_size = vec2(32.0, 36.0);
    style.spacing.slider_width = 190.0;
    style.visuals = Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.selection.bg_fill = TINT;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, BLUE);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(8);
        widget.fg_stroke = Stroke::new(1.3_f32, INK);
    }
    style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(233, 236, 243);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.hovered.weak_bg_fill = TINT;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(213, 220, 255);
    style.visuals.widgets.hovered.bg_stroke =
        Stroke::new(1.0_f32, Color32::from_rgb(197, 205, 246));
    style.visuals.widgets.active.bg_fill = BLUE;
    style.visuals.widgets.active.weak_bg_fill = TINT;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    ctx.set_style(style);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Grid,
    Wand,
    Layers,
    Export,
    Import,
    Folder,
    Save,
    Undo,
    Redo,
    Plus,
    Image,
    Check,
    Eye,
}

pub fn icon(ui: &Ui, rect: Rect, kind: Icon, color: Color32) {
    let icon = match kind {
        Icon::Grid => LucideIcon::Grid2x2,
        Icon::Wand => LucideIcon::WandSparkles,
        Icon::Layers => LucideIcon::Layers,
        Icon::Export => LucideIcon::Download,
        Icon::Import => LucideIcon::Upload,
        Icon::Folder => LucideIcon::FolderOpen,
        Icon::Save => LucideIcon::Save,
        Icon::Undo => LucideIcon::Undo2,
        Icon::Redo => LucideIcon::Redo2,
        Icon::Plus => LucideIcon::Plus,
        Icon::Image => LucideIcon::Image,
        Icon::Check => LucideIcon::Check,
        Icon::Eye => LucideIcon::Eye,
    };
    render_lucide(ui, rect, icon, color);
}

fn render_lucide(ui: &Ui, rect: Rect, icon: LucideIcon, color: Color32) {
    let color = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
    let svg = icon.svg_str().replace("currentColor", &color);
    Image::from_bytes(
        format!("bytes://pixelify-lucide/{icon:?}/{color}.svg"),
        svg.into_bytes(),
    )
    .maintain_aspect_ratio(true)
    .paint_at(ui, rect);
}

#[derive(Clone, Copy)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
}
pub fn button(
    ui: &mut Ui,
    label: &str,
    symbol: Option<Icon>,
    kind: ButtonKind,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    let enabled = ui.is_enabled();
    let fill = match kind {
        ButtonKind::Primary => {
            if enabled {
                if response.hovered() {
                    Color32::from_rgb(54, 70, 209)
                } else {
                    BLUE
                }
            } else {
                Color32::from_rgb(192, 199, 233)
            }
        }
        ButtonKind::Secondary => {
            if response.hovered() {
                TINT
            } else {
                Color32::WHITE
            }
        }
        ButtonKind::Quiet => {
            if response.hovered() {
                BG
            } else {
                Color32::TRANSPARENT
            }
        }
    };
    let fg = if !enabled {
        MUTED
    } else if matches!(kind, ButtonKind::Primary) {
        Color32::WHITE
    } else {
        INK
    };
    ui.painter().rect_filled(rect, 8.0, fill);
    if matches!(kind, ButtonKind::Secondary) {
        ui.painter()
            .rect_stroke(rect, 8.0, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
    }
    if let Some(symbol) = symbol {
        icon(
            ui,
            Rect::from_center_size(
                pos2(
                    if label.is_empty() {
                        rect.center().x
                    } else {
                        rect.left() + 20.0
                    },
                    rect.center().y,
                ),
                vec2(18.0, 18.0),
            ),
            symbol,
            fg,
        );
    }
    let x = if symbol.is_some() && !label.is_empty() {
        rect.center().x + 10.0
    } else {
        rect.center().x
    };
    ui.painter().text(
        pos2(x, rect.center().y),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        fg,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}
pub fn title(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).size(17.0).strong());
}
pub fn caption(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).size(12.0).color(MUTED));
}
pub fn eyebrow(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).size(10.0).color(MUTED).strong());
}
pub fn divider(ui: &mut Ui) {
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, LINE));
    ui.add_space(8.0);
}
pub fn logo(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 10.0, BLUE);
    render_lucide(ui, rect.shrink(7.0), LucideIcon::Sparkles, Color32::WHITE);
}
