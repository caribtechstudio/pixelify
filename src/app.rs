//! Desktop presentation and interaction state. Image algorithms live in pixelify_core.
use crate::design::{self as d, ButtonKind as B, Icon};
use eframe::egui::{self, *};
use image::{Rgba, RgbaImage};
use pixelify_core::lpc::{
    animation_preview, standard_animation, standard_animations, Catalog as LpcCatalog,
    Selection as LpcSelection,
};
use pixelify_core::{effects, export, project, Document};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Character,
    Pixelate,
    Background,
    Layers,
    Export,
}
impl Tool {
    fn label(self) -> &'static str {
        match self {
            Self::Character => "Personnage LPC",
            Self::Pixelate => "Pixelisation",
            Self::Background => "Arrière-plan",
            Self::Layers => "Calques",
            Self::Export => "Exportation",
        }
    }
    fn icon(self) -> Icon {
        match self {
            Self::Character => Icon::Grid,
            Self::Pixelate => Icon::Grid,
            Self::Background => Icon::Wand,
            Self::Layers => Icon::Layers,
            Self::Export => Icon::Export,
        }
    }
}

pub struct Studio {
    document: Option<Document>,
    history: Vec<Document>,
    redo: Vec<Document>,
    texture: Option<TextureHandle>,
    original: Option<TextureHandle>,
    thumbnails: Vec<TextureHandle>,
    tool: Tool,
    block_size: u32,
    colors: u16,
    preset: usize,
    tolerance: u8,
    brush: u32,
    restore: bool,
    zoom: f32,
    pan: Vec2,
    show_original: bool,
    grid: bool,
    status: String,
    name: String,
    frame_duration: u16,
    selected_frame: usize,
    help: bool,
    lpc_catalog: Option<LpcCatalog>,
    lpc_loader: Option<Receiver<Result<LpcCatalog, String>>>,
    lpc_loading: bool,
    lpc_body_type: String,
    lpc_category: String,
    lpc_type: String,
    lpc_query: String,
    lpc_selections: BTreeMap<String, LpcSelection>,
    lpc_preview_sheet: Option<RgbaImage>,
    lpc_preview: Option<TextureHandle>,
    lpc_preview_status: String,
    lpc_animation: String,
    lpc_frame_index: usize,
    lpc_playing: bool,
    lpc_last_frame: Instant,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_context(&cc.egui_ctx)
    }
    fn with_context(ctx: &Context) -> Self {
        d::configure(ctx);
        let mut studio = Self {
            document: None,
            history: Vec::new(),
            redo: Vec::new(),
            texture: None,
            original: None,
            thumbnails: Vec::new(),
            tool: Tool::Character,
            block_size: 8,
            colors: 16,
            preset: 0,
            tolerance: 45,
            brush: 12,
            restore: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            show_original: false,
            grid: false,
            status: "Prêt à créer".into(),
            name: "Nouvelle création".into(),
            frame_duration: 120,
            selected_frame: 0,
            help: false,
            lpc_catalog: None,
            lpc_loader: None,
            lpc_loading: false,
            lpc_body_type: "male".into(),
            lpc_category: String::new(),
            lpc_type: "body".into(),
            lpc_query: String::new(),
            lpc_selections: BTreeMap::new(),
            lpc_preview_sheet: None,
            lpc_preview: None,
            lpc_preview_status: "Sélectionnez une bibliothèque LPC".into(),
            lpc_animation: "idle".into(),
            lpc_frame_index: 0,
            lpc_playing: true,
            lpc_last_frame: Instant::now(),
        };
        studio.load_bundled_lpc();
        studio
    }
    fn checkpoint(&mut self) {
        if let Some(doc) = &self.document {
            self.history.push(doc.clone());
        }
        self.redo.clear();
        // Bound history by both edit count and pixel storage, including animation snapshots.
        while self.history.len() > 1
            && (self.history.len() > 30
                || self.history.iter().map(document_bytes).sum::<usize>() > 256 * 1024 * 1024)
        {
            self.history.remove(0);
        }
    }
    fn undo(&mut self, ctx: &Context) {
        if let Some(previous) = self.history.pop() {
            if let Some(current) = self.document.replace(previous) {
                self.redo.push(current);
            }
            self.selected_frame = 0;
            self.refresh(ctx);
            self.status = "Modification annulée".into();
        }
    }
    fn redo(&mut self, ctx: &Context) {
        if let Some(next) = self.redo.pop() {
            if let Some(current) = self.document.replace(next) {
                self.history.push(current);
            }
            self.selected_frame = 0;
            self.refresh(ctx);
            self.status = "Modification rétablie".into();
        }
    }
    fn refresh(&mut self, ctx: &Context) {
        let Some(doc) = &self.document else {
            self.texture = None;
            return;
        };
        let pixels = if self.selected_frame == 0 {
            doc.flattened()
        } else {
            doc.animation_frames
                .get(self.selected_frame - 1)
                .and_then(|frame| {
                    let snapshot = Document {
                        layers: frame.layers.clone(),
                        ..Document::default()
                    };
                    snapshot.flattened()
                })
        };
        if let Some(pixels) = pixels {
            set_texture(ctx, &mut self.texture, "canvas", &pixels);
            if self.selected_frame == 0 {
                let small =
                    image::imageops::resize(&pixels, 64, 48, image::imageops::FilterType::Nearest);
                if let Some(thumb) = self.thumbnails.first_mut() {
                    thumb.set(
                        ColorImage::from_rgba_unmultiplied([64, 48], small.as_raw()),
                        TextureOptions::NEAREST,
                    );
                }
            }
        }
        if self.thumbnails.len() != doc.animation_frames.len() + 1 {
            self.thumbnails.clear();
            for index in 0..=doc.animation_frames.len() {
                let pixels = if index == 0 {
                    doc.flattened()
                } else {
                    Document {
                        layers: doc.animation_frames[index - 1].layers.clone(),
                        ..Document::default()
                    }
                    .flattened()
                };
                if let Some(pixels) = pixels {
                    let small = image::imageops::resize(
                        &pixels,
                        64,
                        48,
                        image::imageops::FilterType::Nearest,
                    );
                    self.thumbnails.push(ctx.load_texture(
                        format!("frame-{index}"),
                        ColorImage::from_rgba_unmultiplied([64, 48], small.as_raw()),
                        TextureOptions::NEAREST,
                    ));
                }
            }
        }
    }
    fn load_path(&mut self, ctx: &Context, path: PathBuf) {
        if path.extension().is_some_and(|e| e == "pixelify") {
            match project::load(&path) {
                Ok(doc) => {
                    self.block_size = doc.pixel_scale.max(1);
                    self.colors = doc.palette_size;
                    self.name = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into();
                    self.original = None;
                    self.document = Some(doc);
                    self.reset_view();
                    self.refresh(ctx);
                    self.status = "Projet ouvert".into();
                }
                Err(error) => self.status = format!("Ouverture impossible : {error}"),
            }
            return;
        }
        match image::open(&path) {
            Ok(decoded) => {
                let pixels = decoded.to_rgba8();
                if pixels.width() > 8192 || pixels.height() > 8192 {
                    self.status = "Image trop grande : maximum 8192 pixels par côté".into();
                    return;
                }
                self.name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into();
                self.load_pixels(ctx, pixels);
                self.status = "Image importée".into();
            }
            Err(error) => self.status = format!("Import impossible : {error}"),
        }
    }
    fn load_pixels(&mut self, ctx: &Context, pixels: RgbaImage) {
        set_texture(ctx, &mut self.original, "original", &pixels);
        self.document = Some(Document::from_image(self.name.clone(), pixels));
        self.reset_view();
        self.refresh(ctx);
    }
    fn reset_view(&mut self) {
        self.thumbnails.clear();
        self.history.clear();
        self.redo.clear();
        self.selected_frame = 0;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.show_original = false;
    }
    fn import(&mut self, ctx: &Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "webp", "bmp", "tga", "tiff", "gif"],
            )
            .pick_file()
        {
            self.load_path(ctx, path);
        }
    }
    fn open_project(&mut self, ctx: &Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Projet Pixelify", &["pixelify"])
            .pick_file()
        {
            self.load_path(ctx, path);
        }
    }
    fn save(&mut self) {
        if let Some(doc) = &self.document {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Pixelify", &["pixelify"])
                .set_file_name(format!("{}.pixelify", self.name))
                .save_file()
            {
                self.status = match project::save(doc, &path) {
                    Ok(()) => "Projet enregistré".into(),
                    Err(error) => format!("Enregistrement impossible : {error}"),
                };
            }
        }
    }
    fn export(&mut self, ase: bool) {
        if let Some(doc) = &self.document {
            let exts = if ase {
                vec!["aseprite", "ase"]
            } else {
                vec!["png", "webp", "jpg", "bmp", "tga", "tiff"]
            };
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(if ase { "Aseprite" } else { "Images" }, &exts)
                .set_file_name(format!(
                    "{}.{}",
                    self.name,
                    if ase { "aseprite" } else { "png" }
                ))
                .save_file()
            {
                self.status = match if ase {
                    export::export_aseprite(doc, &path)
                } else {
                    export::export_image(doc, &path)
                } {
                    Ok(()) => "Export terminé".into(),
                    Err(error) => format!("Export impossible : {error}"),
                };
            }
        }
    }
    fn export_lpc_zip(&mut self) {
        if let Some(doc) = &self.document {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Archive personnage LPC", &["zip"])
                .set_file_name(format!("{}-lpc.zip", self.name))
                .save_file()
            {
                self.status = match export::export_sprite_sheet_zip(doc, &path) {
                    Ok(()) => "Archive personnage exportée avec ses crédits".into(),
                    Err(error) => format!("Export impossible : {error}"),
                };
            }
        }
    }
    fn load_bundled_lpc(&mut self) {
        let Some(path) = std::env::current_dir()
            .ok()
            .map(|directory| directory.join("assets/lpc"))
            .filter(|path| path.is_dir())
        else {
            return;
        };
        self.queue_lpc_library(path);
    }
    fn queue_lpc_library(&mut self, path: PathBuf) {
        let (sender, receiver) = mpsc::channel();
        self.lpc_catalog = None;
        self.lpc_loader = Some(receiver);
        self.lpc_loading = true;
        self.lpc_preview_sheet = None;
        self.lpc_preview = None;
        self.lpc_preview_status = "Chargement de la bibliothèque LPC…".into();
        self.status = "Chargement de la bibliothèque LPC en arrière-plan…".into();
        thread::spawn(move || {
            let _ = sender.send(LpcCatalog::open(&path).map_err(|error| error.to_string()));
        });
    }
    fn poll_lpc_library(&mut self, ctx: &Context) {
        let Some(receiver) = &self.lpc_loader else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("le chargement LPC a été interrompu".into()),
        };
        self.lpc_loader = None;
        self.lpc_loading = false;
        match result {
            Ok(catalog) => {
                self.install_lpc_catalog(catalog, ctx);
                self.status = "Bibliothèque LPC locale prête · composez votre personnage".into();
            }
            Err(error) => self.status = format!("Bibliothèque LPC impossible à ouvrir : {error}"),
        }
    }
    fn install_lpc_catalog(&mut self, catalog: LpcCatalog, ctx: &Context) {
        self.lpc_selections.clear();
        if let Some(selection) = catalog.default_body() {
            if let Some(item) = catalog.item(&selection.item_id) {
                self.lpc_selections
                    .insert(item.type_name.clone(), selection);
            }
        }
        self.lpc_category.clear();
        self.lpc_type = "body".into();
        self.lpc_catalog = Some(catalog);
        self.refresh_lpc_preview(ctx);
    }
    fn import_lpc_library(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.queue_lpc_library(path);
        }
    }
    fn refresh_lpc_preview(&mut self, ctx: &Context) {
        let Some(catalog) = &self.lpc_catalog else {
            self.lpc_preview_sheet = None;
            self.lpc_preview = None;
            return;
        };
        let selections = self.lpc_selections.values().cloned().collect::<Vec<_>>();
        match catalog.compose(&selections, &self.lpc_body_type) {
            Ok(document) => {
                let Some(sheet) = document.flattened() else {
                    self.lpc_preview_sheet = None;
                    self.lpc_preview = None;
                    self.lpc_preview_status = "Aperçu indisponible".into();
                    return;
                };
                self.lpc_preview_sheet = Some(sheet);
                self.lpc_frame_index = 0;
                self.lpc_last_frame = Instant::now();
                self.paint_lpc_preview(ctx);
            }
            Err(error) => {
                self.lpc_preview_sheet = None;
                self.lpc_preview = None;
                self.lpc_preview_status = format!("Aperçu impossible : {error}");
            }
        }
    }
    fn paint_lpc_preview(&mut self, ctx: &Context) {
        let Some(sheet) = &self.lpc_preview_sheet else {
            return;
        };
        let Some(animation) = standard_animation(&self.lpc_animation) else {
            return;
        };
        let cycle = animation.cycle();
        if cycle.is_empty() {
            return;
        }
        let frame_index = self.lpc_frame_index % cycle.len();
        let Some(preview) = animation_preview(sheet, animation.id(), frame_index) else {
            self.lpc_preview = None;
            self.lpc_preview_status = "Aperçu hors des limites de la feuille LPC".into();
            return;
        };
        set_texture(ctx, &mut self.lpc_preview, "lpc-live-preview", &preview);
        let directions = if animation.row_count() == 1 { 1 } else { 4 };
        self.lpc_preview_status = format!(
            "{} · image {}/{} · {} direction{} · {} élément(s)",
            animation.label(),
            frame_index + 1,
            cycle.len(),
            directions,
            if directions > 1 { "s" } else { "" },
            self.lpc_selections.len(),
        );
    }
    fn advance_lpc_preview(&mut self, ctx: &Context) {
        if !self.lpc_playing || self.lpc_preview_sheet.is_none() {
            return;
        }
        let Some(animation) = standard_animation(&self.lpc_animation) else {
            return;
        };
        let elapsed = self.lpc_last_frame.elapsed();
        let frame_time = Duration::from_millis(125);
        if elapsed < frame_time {
            return;
        }
        let steps = (elapsed.as_millis() / frame_time.as_millis()).max(1) as usize;
        self.lpc_frame_index = (self.lpc_frame_index + steps) % animation.cycle().len();
        self.lpc_last_frame = Instant::now();
        self.paint_lpc_preview(ctx);
    }
    fn create_lpc_character(&mut self, ctx: &Context) {
        let Some(catalog) = &self.lpc_catalog else {
            return;
        };
        let selections = self.lpc_selections.values().cloned().collect::<Vec<_>>();
        match catalog.compose(&selections, &self.lpc_body_type) {
            Ok(document) => {
                self.document = Some(document);
                self.name = "Personnage LPC".into();
                self.original = None;
                self.reset_view();
                self.refresh(ctx);
                self.tool = Tool::Layers;
                self.status =
                    "Personnage composé · la feuille contient ses animations LPC standards".into();
            }
            Err(error) => self.status = format!("Composition impossible : {error}"),
        }
    }
    fn demo(&mut self, ctx: &Context) {
        self.name = "Horizon".into();
        self.load_pixels(ctx, demo_image());
        self.status = "Exemple chargé · vous pouvez modifier tous les réglages".into();
    }
    fn apply_pixels(&mut self, ctx: &Context) {
        if self.document.is_none() {
            return;
        }
        self.checkpoint();
        if let Some(layer) = self.document.as_mut().and_then(Document::active_layer_mut) {
            let pixels = effects::pixelate(&layer.pixels, self.block_size);
            layer.pixels = if self.preset == 1 || self.preset == 3 {
                effects::map_tonal_palette(&pixels, preset_palette(self.preset))
            } else if self.preset > 0 {
                effects::map_palette(&pixels, preset_palette(self.preset))
            } else {
                effects::reduce_palette(&pixels, self.colors)
            };
        }
        self.selected_frame = 0;
        self.show_original = false;
        self.refresh(ctx);
        self.status = "Pixelisation appliquée · l’original reste accessible".into();
    }
    fn header(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(74.0)
            .frame(
                Frame::NONE
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::symmetric(22, 18)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                    d::logo(ui, r);
                    ui.add_space(2.0);
                    ui.label(RichText::new("Pixelify").size(21.0).strong());
                    ui.add_space(20.0);
                    ui.menu_button("Fichier", |ui| {
                        if ui.button("Importer une image…").clicked() {
                            self.import(ctx);
                            ui.close_menu();
                        }
                        if ui.button("Ouvrir un projet…").clicked() {
                            self.open_project(ctx);
                            ui.close_menu();
                        }
                        ui.add_enabled_ui(self.document.is_some(), |ui| {
                            if ui.button("Enregistrer le projet…").clicked() {
                                self.save();
                                ui.close_menu();
                            }
                        });
                    });
                    ui.add_enabled_ui(!self.history.is_empty(), |ui| {
                        if d::button(ui, "", Some(Icon::Undo), B::Quiet, 34.0)
                            .on_hover_text("Annuler · Cmd/Ctrl Z")
                            .clicked()
                        {
                            self.undo(ctx);
                        }
                    });
                    ui.add_enabled_ui(!self.redo.is_empty(), |ui| {
                        if d::button(ui, "", Some(Icon::Redo), B::Quiet, 34.0)
                            .on_hover_text("Rétablir · Cmd/Ctrl Shift Z")
                            .clicked()
                        {
                            self.redo(ctx);
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(self.document.is_some(), |ui| {
                            if d::button(ui, "Exporter", Some(Icon::Export), B::Primary, 122.0)
                                .clicked()
                            {
                                self.tool = Tool::Export;
                            }
                        });
                        if d::button(ui, "Importer", Some(Icon::Plus), B::Secondary, 112.0)
                            .clicked()
                        {
                            self.import(ctx);
                        }
                        ui.add_enabled_ui(self.document.is_some(), |ui| {
                            if d::button(ui, "", Some(Icon::Save), B::Quiet, 38.0)
                                .on_hover_text("Enregistrer le projet")
                                .clicked()
                            {
                                self.save();
                            }
                        });
                    });
                });
            });
    }
    fn sidebar(&mut self, ctx: &Context) {
        egui::SidePanel::left("navigation")
            .exact_width(204.0)
            .resizable(false)
            .frame(
                Frame::NONE
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::symmetric(16, 24)),
            )
            .show(ctx, |ui| {
                d::eyebrow(ui, "ESPACE DE CRÉATION");
                ui.add_space(12.0);
                for tool in [
                    Tool::Character,
                    Tool::Pixelate,
                    Tool::Background,
                    Tool::Layers,
                    Tool::Export,
                ] {
                    let selected = self.tool == tool;
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                    response.widget_info(|| {
                        WidgetInfo::labeled(WidgetType::Button, true, tool.label())
                    });
                    if selected || response.hovered() {
                        ui.painter()
                            .rect_filled(rect, 9.0, if selected { d::TINT } else { d::BG });
                    }
                    let fg = if selected { d::BLUE } else { d::MUTED };
                    d::icon(
                        ui,
                        Rect::from_center_size(
                            pos2(rect.left() + 22.0, rect.center().y),
                            vec2(19.0, 19.0),
                        ),
                        tool.icon(),
                        fg,
                    );
                    ui.painter().text(
                        pos2(rect.left() + 43.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        tool.label(),
                        FontId::proportional(13.0),
                        if selected { d::BLUE } else { d::INK },
                    );
                    if selected {
                        ui.painter().circle_filled(
                            pos2(rect.right() - 14.0, rect.center().y),
                            2.5,
                            d::BLUE,
                        );
                    }
                    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        self.tool = tool;
                    }
                }
                d::divider(ui);
                if d::button(ui, "Ouvrir un projet", Some(Icon::Folder), B::Quiet, 172.0).clicked()
                {
                    self.open_project(ctx);
                }
                if d::button(ui, "Image d’exemple", Some(Icon::Image), B::Quiet, 172.0).clicked()
                {
                    self.demo(ctx);
                }
                let y = (ui.max_rect().bottom() - 174.0).max(ui.cursor().top() + 24.0);
                ui.add_space((y - ui.cursor().top()).max(0.0));
                Frame::NONE
                    .fill(d::BG)
                    .corner_radius(10)
                    .inner_margin(14)
                    .show(ui, |ui| {
                        d::eyebrow(ui, "VOTRE ATELIER, EN LOCAL");
                        ui.add_space(4.0);
                        d::caption(ui, "Vos images restent sur votre ordinateur.");
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                            d::icon(ui, r, Icon::Check, d::GREEN);
                            ui.label(
                                RichText::new("Sans compte · hors ligne")
                                    .size(10.5)
                                    .color(d::GREEN),
                            );
                        });
                    });
                if d::button(ui, "Aide & raccourcis", None, B::Quiet, 172.0).clicked() {
                    self.help = true;
                }
            });
    }
    fn inspector(&mut self, ctx: &Context) {
        egui::SidePanel::right("inspector")
            .exact_width(290.0)
            .resizable(false)
            .frame(
                Frame::NONE
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::symmetric(20, 24)),
            )
            .show(ctx, |ui| {
                d::eyebrow(ui, "RÉGLAGES");
                ui.add_space(4.0);
                d::title(ui, self.tool.label());
                ui.add_space(4.0);
                d::caption(
                    ui,
                    match self.tool {
                        Tool::Character => "Composez localement un personnage LPC animé.",
                        Tool::Pixelate => "Un style rétro, à votre mesure.",
                        Tool::Background => "Isolez le sujet. Affinez les contours.",
                        Tool::Layers => "Organisez les éléments de votre image.",
                        Tool::Export => "Votre création, au bon format.",
                    },
                );
                d::divider(ui);
                let reserved = if self.tool == Tool::Pixelate {
                    104.0
                } else {
                    0.0
                };
                let controls = ui.available_rect_before_wrap();
                ScrollArea::vertical()
                    .max_height((controls.height() - reserved).max(100.0))
                    .id_salt("controls")
                    .show(ui, |ui| match self.tool {
                        Tool::Character => self.lpc_controls(ui, ctx),
                        Tool::Pixelate => self.pixel_controls(ui),
                        Tool::Background => self.background_controls(ui, ctx),
                        Tool::Layers => self.layer_controls(ui, ctx),
                        Tool::Export => self.export_controls(ui),
                    });
                if self.tool == Tool::Pixelate {
                    let footer = Rect::from_min_max(
                        pos2(controls.left(), controls.bottom() - reserved + 10.0),
                        controls.max,
                    );
                    let mut action_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(footer)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    d::divider(&mut action_ui);
                    action_ui.add_enabled_ui(
                        self.document.is_some() && self.selected_frame == 0,
                        |ui| {
                            if d::button(
                                ui,
                                "Appliquer le style",
                                Some(Icon::Grid),
                                B::Primary,
                                ui.available_width(),
                            )
                            .clicked()
                            {
                                self.apply_pixels(ctx);
                            }
                        },
                    );
                    d::caption(&mut action_ui, "Annulez à tout moment avec Cmd / Ctrl Z.");
                }
            });
    }
    fn lpc_controls(&mut self, ui: &mut Ui, ctx: &Context) {
        let Some(catalog) = &self.lpc_catalog else {
            if self.lpc_loading {
                d::eyebrow(ui, "BIBLIOTHÈQUE LPC HORS LIGNE");
                d::caption(
                    ui,
                    "Indexation des pièces en cours. Vous pouvez continuer à naviguer dans l’application.",
                );
                return;
            }
            d::caption(ui, "Ajoutez une copie locale du dépôt Universal-LPC-Spritesheet-Character-Generator. Pixelify ne télécharge ni ne redistribue ses assets.");
            ui.add_space(10.0);
            if d::button(
                ui,
                "Choisir la bibliothèque LPC…",
                Some(Icon::Folder),
                B::Primary,
                ui.available_width(),
            )
            .clicked()
            {
                self.import_lpc_library();
            }
            return;
        };
        let categories = catalog.categories();
        let all_items = catalog.items.clone();
        let library_path = catalog.root().display().to_string();
        let previous_body = self.lpc_body_type.clone();
        let previous_selections = self.lpc_selections.clone();
        let previous_animation = self.lpc_animation.clone();
        if !self.lpc_category.is_empty() && !categories.contains(&self.lpc_category) {
            self.lpc_category.clear();
        }
        let category_items = all_items
            .iter()
            .filter(|item| self.lpc_category.is_empty() || item.category == self.lpc_category)
            .collect::<Vec<_>>();
        let types = category_items
            .iter()
            .map(|item| item.type_name.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if !types.contains(&self.lpc_type) {
            self.lpc_type = types.first().cloned().unwrap_or_default();
        }
        d::eyebrow(ui, "BIBLIOTHÈQUE LPC HORS LIGNE");
        d::caption(
            ui,
            &format!("{} pièces indexées · {}", all_items.len(), library_path),
        );
        if d::button(
            ui,
            "Changer de bibliothèque…",
            Some(Icon::Folder),
            B::Quiet,
            ui.available_width(),
        )
        .clicked()
        {
            self.import_lpc_library();
            return;
        }
        d::divider(ui);
        ui.label("Morphologie");
        ComboBox::from_id_salt("lpc-body-type")
            .selected_text(&self.lpc_body_type)
            .show_ui(ui, |ui| {
                for body in ["male", "female", "teen", "child", "muscular", "pregnant"] {
                    ui.selectable_value(&mut self.lpc_body_type, body.to_owned(), body);
                }
            });
        ui.label("Animation en aperçu");
        let animation_label = standard_animation(&self.lpc_animation)
            .map(|animation| animation.label())
            .unwrap_or("Repos");
        ComboBox::from_id_salt("lpc-preview-animation")
            .selected_text(animation_label)
            .show_ui(ui, |ui| {
                for animation in standard_animations() {
                    ui.selectable_value(
                        &mut self.lpc_animation,
                        animation.id().to_owned(),
                        animation.label(),
                    );
                }
            });
        ui.horizontal(|ui| {
            let label = if self.lpc_playing { "Pause" } else { "Lire" };
            if d::button(ui, label, None, B::Secondary, 94.0).clicked() {
                self.lpc_playing = !self.lpc_playing;
                self.lpc_last_frame = Instant::now();
            }
            if d::button(ui, "Repartir", None, B::Quiet, 100.0).clicked() {
                self.lpc_frame_index = 0;
                self.lpc_last_frame = Instant::now();
                self.paint_lpc_preview(ctx);
            }
        });
        d::caption(
            ui,
            "L’aperçu montre simultanément toutes les directions disponibles de l’animation.",
        );
        ui.label("Dossier d’assets");
        ComboBox::from_id_salt("lpc-category")
            .selected_text(if self.lpc_category.is_empty() {
                "Toute la bibliothèque"
            } else {
                &self.lpc_category
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.lpc_category,
                    String::new(),
                    "Toute la bibliothèque",
                );
                for category in &categories {
                    ui.selectable_value(&mut self.lpc_category, category.clone(), category);
                }
            });
        ui.label("Emplacement du personnage");
        ComboBox::from_id_salt("lpc-type")
            .selected_text(&self.lpc_type)
            .show_ui(ui, |ui| {
                for item_type in &types {
                    ui.selectable_value(&mut self.lpc_type, item_type.clone(), item_type);
                }
            });
        ui.add(TextEdit::singleline(&mut self.lpc_query).hint_text("Rechercher une pièce…"));
        let query = self.lpc_query.to_lowercase();
        let matches = category_items
            .into_iter()
            .filter(|item| {
                item.type_name == self.lpc_type
                    && (query.is_empty() || item.name.to_lowercase().contains(&query))
            })
            .take(80)
            .cloned()
            .collect::<Vec<_>>();
        ui.add_space(4.0);
        d::caption(
            ui,
            "Une pièce remplace celle du même emplacement et met l’aperçu à jour immédiatement.",
        );
        let mut choose = None;
        ScrollArea::vertical()
            .id_salt("lpc-items")
            .max_height(210.0)
            .show(ui, |ui| {
                for item in &matches {
                    let selected = self
                        .lpc_selections
                        .get(&item.type_name)
                        .is_some_and(|selection| selection.item_id == item.id);
                    let compatible = item
                        .body_types
                        .iter()
                        .any(|body| body == &self.lpc_body_type);
                    let response =
                        ui.add_enabled(compatible, SelectableLabel::new(selected, &item.name));
                    if !compatible {
                        response.clone().on_hover_text(
                            "Cette pièce ne propose pas la morphologie sélectionnée.",
                        );
                    }
                    if response.clicked() {
                        choose = Some(item.clone());
                    }
                }
                if matches.is_empty() {
                    d::caption(ui, "Aucune pièce compatible dans cette catégorie.");
                }
            });
        if let Some(item) = choose {
            let variant = item
                .variants
                .first()
                .cloned()
                .unwrap_or_else(|| "base".into());
            self.lpc_selections.insert(
                item.type_name.clone(),
                LpcSelection {
                    item_id: item.id,
                    variant,
                },
            );
        }
        if let Some(current) = self.lpc_selections.get(&self.lpc_type).cloned() {
            if let Some(item) = all_items.iter().find(|item| item.id == current.item_id) {
                ui.add_space(6.0);
                ui.label(format!("Variante · {}", item.name));
                if let Some(selection) = self.lpc_selections.get_mut(&self.lpc_type) {
                    ComboBox::from_id_salt("lpc-variant")
                        .selected_text(&selection.variant)
                        .show_ui(ui, |ui| {
                            for variant in &item.variants {
                                ui.selectable_value(
                                    &mut selection.variant,
                                    variant.clone(),
                                    variant,
                                );
                            }
                        });
                }
            }
        }
        if previous_body != self.lpc_body_type || previous_selections != self.lpc_selections {
            self.refresh_lpc_preview(ctx);
        } else if previous_animation != self.lpc_animation {
            self.lpc_frame_index = 0;
            self.lpc_last_frame = Instant::now();
            self.paint_lpc_preview(ctx);
        }
        d::divider(ui);
        d::eyebrow(ui, "TENUE SÉLECTIONNÉE");
        let selected_names = self
            .lpc_selections
            .values()
            .filter_map(|selection| {
                all_items
                    .iter()
                    .find(|item| item.id == selection.item_id)
                    .map(|item| format!("{} · {}", item.name, selection.variant))
            })
            .collect::<Vec<_>>();
        d::caption(
            ui,
            &format!(
                "{} élément(s) : {}",
                selected_names.len(),
                selected_names.join(" · ")
            ),
        );
        ui.add_space(8.0);
        if d::button(
            ui,
            "Créer le personnage animé",
            Some(Icon::Grid),
            B::Primary,
            ui.available_width(),
        )
        .clicked()
        {
            self.create_lpc_character(ctx);
        }
        d::caption(ui, "Les animations LPC standard sont exportées sur une spritesheet. Les actions d’armes surdimensionnées restent séparées.");
    }
    fn pixel_controls(&mut self, ui: &mut Ui) {
        d::eyebrow(ui, "STYLE");
        ui.add_space(4.0);
        for (index, (label, detail)) in [
            ("Personnalisé", "Couleurs de l’image"),
            ("Game Boy", "4 couleurs · vert rétro"),
            ("PICO-8", "16 couleurs · arcade"),
            ("Monochrome", "2 couleurs · noir & blanc"),
        ]
        .iter()
        .enumerate()
        {
            let (r, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, *label));
            let selected = self.preset == index;
            ui.painter()
                .rect_filled(r, 8.0, if selected { d::TINT } else { Color32::WHITE });
            ui.painter().rect_stroke(
                r,
                8.0,
                Stroke::new(
                    1.0_f32,
                    if selected {
                        Color32::from_rgb(211, 216, 252)
                    } else {
                        d::LINE
                    },
                ),
                StrokeKind::Inside,
            );
            ui.painter().text(
                pos2(r.left() + 12.0, r.top() + 17.0),
                Align2::LEFT_CENTER,
                *label,
                FontId::proportional(12.5),
                if selected { d::BLUE } else { d::INK },
            );
            ui.painter().text(
                pos2(r.left() + 12.0, r.top() + 35.0),
                Align2::LEFT_CENTER,
                *detail,
                FontId::proportional(10.5),
                d::MUTED,
            );
            if selected {
                d::icon(
                    ui,
                    Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(14.0, 14.0)),
                    Icon::Check,
                    d::BLUE,
                );
            }
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.preset = index;
            }
        }
        d::divider(ui);
        ui.horizontal(|ui| {
            ui.label("Taille des pixels");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} px", self.block_size))
                        .color(d::BLUE)
                        .strong(),
                );
            });
        });
        ui.add(Slider::new(&mut self.block_size, 1..=64).show_value(false));
        d::caption(
            ui,
            "Plus la valeur est haute, plus les blocs sont visibles.",
        );
        if self.preset == 0 {
            ui.add_space(10.0);
            ui.label("Nombre de couleurs");
            ui.add(Slider::new(&mut self.colors, 0..=256).text(""));
            d::caption(ui, "0 conserve les couleurs originales.");
        } else {
            ui.add_space(6.0);
            d::eyebrow(ui, "PALETTE");
            ui.horizontal_wrapped(|ui| {
                for color in preset_palette(self.preset) {
                    let (r, _) = ui.allocate_exact_size(vec2(17.0, 17.0), Sense::hover());
                    ui.painter().rect_filled(
                        r,
                        4.0,
                        Color32::from_rgb(color[0], color[1], color[2]),
                    );
                }
            });
        }
    }
    fn background_controls(&mut self, ui: &mut Ui, ctx: &Context) {
        d::eyebrow(ui, "DÉTOURAGE PAR COULEUR");
        d::caption(ui, "Adapté aux fonds uniformes, reliés au bord de l’image.");
        ui.add_space(10.0);
        ui.label("Tolérance");
        ui.add(Slider::new(&mut self.tolerance, 1..=150));
        ui.add_enabled_ui(self.document.is_some() && self.selected_frame == 0, |ui| {
            if d::button(
                ui,
                "Retirer le fond",
                Some(Icon::Wand),
                B::Primary,
                ui.available_width(),
            )
            .clicked()
            {
                self.checkpoint();
                if let Some(layer) = self.document.as_mut().and_then(Document::active_layer_mut) {
                    layer.pixels = effects::remove_background(&layer.pixels, self.tolerance);
                }
                self.show_original = false;
                self.refresh(ctx);
                self.status = "Fond retiré · affinez avec le pinceau".into();
            }
        });
        d::divider(ui);
        d::eyebrow(ui, "RETOUCHE DU MASQUE");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (label, value) in [("Effacer", false), ("Restaurer", true)] {
                if ui.selectable_label(self.restore == value, label).clicked() {
                    self.restore = value;
                }
            }
        });
        ui.label("Rayon du pinceau");
        ui.add(Slider::new(&mut self.brush, 1..=80).suffix(" px"));
        d::caption(
            ui,
            "Dessinez directement sur le rendu. Une touche annule tout le trait.",
        );
    }
    fn layer_controls(&mut self, ui: &mut Ui, ctx: &Context) {
        if self.selected_frame != 0 {
            d::caption(ui, "Cette frame est une capture. Sélectionnez la frame 01 pour modifier les calques du rendu.");
            return;
        }
        let Some(doc) = &self.document else {
            d::caption(ui, "Importez une image pour créer votre premier calque.");
            return;
        };
        let mut edits = Vec::new();
        let mut active = doc.active_layer;
        for (index, layer) in doc.layers.iter().enumerate().rev() {
            let mut visible = layer.visible;
            let mut opacity = layer.opacity;
            Frame::NONE
                .fill(if index == active { d::TINT } else { d::BG })
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut visible, "");
                        if ui.selectable_label(index == active, &layer.name).clicked() {
                            active = index;
                        }
                    });
                    if index == active {
                        ui.add_space(4.0);
                        ui.label("Opacité");
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = (ui.available_width() - 72.0).max(80.0);
                            ui.add(Slider::new(&mut opacity, 0..=255).custom_formatter(
                                |value, _| format!("{} %", (value / 255.0 * 100.0).round()),
                            ));
                        });
                    }
                });
            if visible != layer.visible || opacity != layer.opacity {
                edits.push((index, visible, opacity));
            }
            ui.add_space(6.0);
        }
        let can_duplicate = doc.animation_frames.is_empty();
        let mut changed = !edits.is_empty();
        if changed {
            self.checkpoint();
        }
        if let Some(doc) = &mut self.document {
            doc.active_layer = active;
            for (index, visible, opacity) in edits {
                doc.layers[index].visible = visible;
                doc.layers[index].opacity = opacity;
            }
        }
        ui.add_space(8.0);
        let duplicate = ui
            .add_enabled_ui(can_duplicate, |ui| {
                d::button(
                    ui,
                    "Dupliquer le calque",
                    Some(Icon::Layers),
                    B::Secondary,
                    ui.available_width(),
                )
                .clicked()
            })
            .inner;
        if duplicate {
            self.checkpoint();
            if let Some(doc) = &mut self.document {
                doc.duplicate_active_layer();
            }
            changed = true;
        }
        if !can_duplicate {
            d::caption(
                ui,
                "La structure des calques est verrouillée après la capture de frames.",
            );
        }
        if changed {
            self.refresh(ctx);
        }
    }
    fn export_controls(&mut self, ui: &mut Ui) {
        Frame::NONE
            .fill(d::BG)
            .corner_radius(10)
            .inner_margin(14)
            .show(ui, |ui| {
                d::title(ui, "Image");
                d::caption(ui, "PNG, WebP, JPEG, BMP, TGA ou TIFF.");
                ui.add_space(8.0);
                ui.add_enabled_ui(self.document.is_some(), |ui| {
                    if d::button(
                        ui,
                        "Exporter l’image",
                        Some(Icon::Export),
                        B::Primary,
                        ui.available_width(),
                    )
                    .clicked()
                    {
                        self.export(false);
                    }
                });
            });
        if self
            .document
            .as_ref()
            .is_some_and(|doc| !doc.credits.trim().is_empty())
        {
            ui.add_space(14.0);
            Frame::NONE
                .fill(d::BG)
                .corner_radius(10)
                .inner_margin(14)
                .show(ui, |ui| {
                    d::title(ui, "Personnage LPC");
                    d::caption(
                        ui,
                        "Spritesheet PNG et crédits des assets utilisés, dans une même archive.",
                    );
                    ui.add_space(8.0);
                    if d::button(
                        ui,
                        "Exporter le personnage + crédits",
                        Some(Icon::Export),
                        B::Primary,
                        ui.available_width(),
                    )
                    .clicked()
                    {
                        self.export_lpc_zip();
                    }
                });
        }
        ui.add_space(14.0);
        Frame::NONE
            .fill(d::BG)
            .corner_radius(10)
            .inner_margin(14)
            .show(ui, |ui| {
                d::title(ui, "Aseprite");
                d::caption(ui, "Calques, transparence et frames capturées.");
                ui.add_space(8.0);
                ui.add_enabled_ui(self.document.is_some(), |ui| {
                    if d::button(
                        ui,
                        "Exporter .aseprite",
                        Some(Icon::Layers),
                        B::Secondary,
                        ui.available_width(),
                    )
                    .clicked()
                    {
                        self.export(true);
                    }
                });
            });
        d::divider(ui);
        d::eyebrow(ui, "BON À SAVOIR");
        d::caption(
            ui,
            "PNG est recommandé pour garder des pixels nets et un fond transparent.",
        );
        d::caption(
            ui,
            "Enregistrez aussi un projet .pixelify pour reprendre votre travail.",
        );
    }
    fn workspace(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                d::eyebrow(
                    ui,
                    &format!("STUDIO / {}", self.tool.label().to_uppercase()),
                );
                ui.add_space(2.0);
                let title = if self.tool == Tool::Character {
                    "Générateur de personnage"
                } else {
                    &self.name
                };
                ui.add(Label::new(RichText::new(title).size(24.0).strong()).truncate());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(114.0, 27.0), Sense::hover());
                ui.painter()
                    .rect_filled(r, 6.0, Color32::from_rgb(235, 246, 240));
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    "Traitement local",
                    FontId::proportional(10.5),
                    d::GREEN,
                );
            });
        });
        ui.add_space(18.0);
        let available = ui.available_size();
        let card = Rect::from_min_size(ui.cursor().min, available);
        ui.painter().rect_filled(card, 14.0, Color32::WHITE);
        ui.painter().rect_stroke(
            card,
            14.0,
            Stroke::new(1.0_f32, d::LINE),
            StrokeKind::Inside,
        );
        let mut card_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(card.shrink(16.0))
                .layout(Layout::top_down(Align::Min)),
        );
        card_ui.horizontal(|ui| {
            if self.tool == Tool::Character {
                d::caption(ui, "Aperçu animé · ordre : nord, ouest, sud, est");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if self.lpc_playing {
                            "Lecture · 8 i/s"
                        } else {
                            "En pause"
                        })
                        .size(11.0)
                        .color(d::MUTED),
                    );
                });
            } else {
                ui.add_enabled_ui(self.document.is_some(), |ui| {
                    for (label, original) in [("Rendu", false), ("Original", true)] {
                        let selected = self.show_original == original;
                        let response = ui.add_enabled(
                            !original || self.original.is_some(),
                            egui::Button::new(RichText::new(label).size(12.0).color(if selected {
                                d::BLUE
                            } else {
                                d::MUTED
                            }))
                            .fill(if selected {
                                d::TINT
                            } else {
                                Color32::TRANSPARENT
                            })
                            .stroke(Stroke::NONE),
                        );
                        if response.clicked() {
                            self.show_original = original;
                        }
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if d::button(ui, "Ajuster", None, B::Quiet, 60.0).clicked() {
                        self.zoom = 1.0;
                        self.pan = Vec2::ZERO;
                    }
                    ui.label(
                        RichText::new(format!("{:.0} %", self.zoom * 100.0))
                            .size(11.0)
                            .color(d::MUTED),
                    );
                    ui.add_enabled_ui(self.document.is_some(), |ui| {
                        ui.checkbox(&mut self.grid, "Grille");
                    });
                });
            }
        });
        let viewport = Rect::from_min_max(
            pos2(card.left() + 16.0, card.top() + 68.0),
            pos2(card.right() - 16.0, card.bottom() - 16.0),
        );
        if self.tool == Tool::Character {
            self.lpc_workspace(&mut card_ui, viewport);
        } else if self.texture.is_none() {
            self.empty_canvas(&mut card_ui, viewport, ctx);
        } else {
            self.image_canvas(&mut card_ui, viewport, ctx);
        }
        ui.allocate_rect(card, Sense::hover());
    }
    fn lpc_workspace(&self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 10.0, d::BG);
        let Some(texture) = &self.lpc_preview else {
            let center = rect.center();
            d::icon(
                ui,
                Rect::from_center_size(center - vec2(0.0, 36.0), vec2(42.0, 42.0)),
                Icon::Grid,
                d::BLUE,
            );
            p.text(
                center,
                Align2::CENTER_CENTER,
                "Sélectionnez une pièce pour voir votre personnage.",
                FontId::proportional(15.0),
                d::INK,
            );
            return;
        };
        let source = texture.size_vec2();
        let fit = ((rect.width() - 80.0) / source.x)
            .min((rect.height() - 105.0) / source.y)
            .max(1.0);
        let size = source * fit;
        let image_rect = Rect::from_center_size(rect.center() - vec2(0.0, 14.0), size);
        checkerboard(&p, image_rect, 14.0);
        p.image(
            texture.id(),
            image_rect,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        p.text(
            pos2(rect.center().x, rect.bottom() - 22.0),
            Align2::CENTER_CENTER,
            &self.lpc_preview_status,
            FontId::proportional(11.5),
            d::MUTED,
        );
    }
    fn empty_canvas(&mut self, ui: &mut Ui, rect: Rect, ctx: &Context) {
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 10.0, d::BG);
        let c = rect.center();
        let compact = rect.height() < 430.0;
        let illustration = Rect::from_center_size(
            c + vec2(0.0, if compact { -95.0 } else { -130.0 }),
            vec2(124.0, 94.0),
        );
        p.rect_filled(
            illustration.translate(vec2(9.0, -8.0)),
            12.0,
            Color32::from_rgb(226, 231, 255),
        );
        p.rect_filled(illustration, 12.0, Color32::WHITE);
        p.rect_stroke(
            illustration,
            12.0,
            Stroke::new(1.0_f32, Color32::from_rgb(208, 216, 244)),
            StrokeKind::Inside,
        );
        d::icon(ui, illustration.shrink(25.0), Icon::Image, d::BLUE);
        let y = if compact { c.y - 22.0 } else { c.y - 45.0 };
        p.text(
            pos2(c.x, y),
            Align2::CENTER_CENTER,
            "Donnez une autre dimension",
            FontId::proportional(21.0),
            d::INK,
        );
        p.text(
            pos2(c.x, y + 28.0),
            Align2::CENTER_CENTER,
            "à vos images.",
            FontId::proportional(21.0),
            d::INK,
        );
        p.text(
            pos2(c.x, y + 60.0),
            Align2::CENTER_CENTER,
            "Déposez une image ici pour commencer.",
            FontId::proportional(12.0),
            d::MUTED,
        );
        let mut actions = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(c.x - 158.0, y + 90.0),
                    vec2(316.0, 50.0),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        if d::button(
            &mut actions,
            "Importer une image",
            Some(Icon::Import),
            B::Primary,
            178.0,
        )
        .clicked()
        {
            self.import(ctx);
        }
        if d::button(&mut actions, "Voir un exemple", None, B::Secondary, 128.0).clicked() {
            self.demo(ctx);
        }
        p.text(
            pos2(c.x, y + 150.0),
            Align2::CENTER_CENTER,
            "PNG · JPG · WebP · TIFF · BMP · GIF",
            FontId::proportional(10.5),
            d::MUTED,
        );
    }
    fn image_canvas(&mut self, ui: &mut Ui, rect: Rect, ctx: &Context) {
        let texture = if self.show_original {
            self.original.as_ref().or(self.texture.as_ref())
        } else {
            self.texture.as_ref()
        };
        let Some(texture) = texture else {
            return;
        };
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 10.0, d::BG);
        if self.show_original {
            d::icon(
                ui,
                Rect::from_center_size(rect.right_top() + vec2(-20.0, 20.0), vec2(18.0, 18.0)),
                Icon::Eye,
                d::MUTED,
            );
        }
        let source = texture.size_vec2();
        let fit = ((rect.width() - 48.0) / source.x)
            .min((rect.height() - 48.0) / source.y)
            .min(1.0);
        let size = source * fit * self.zoom;
        let image_rect = Rect::from_center_size(rect.center() + self.pan, size);
        checkerboard(&p, image_rect.intersect(rect), 12.0);
        p.image(
            texture.id(),
            image_rect,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if self.grid && size.x / source.x >= 4.0 {
            let scale = size.x / source.x;
            for x in 0..=source.x as u32 {
                let px = image_rect.left() + x as f32 * scale;
                if rect.x_range().contains(px) {
                    p.vline(
                        px,
                        image_rect.y_range(),
                        Stroke::new(0.5_f32, Color32::from_black_alpha(45)),
                    );
                }
            }
            for y in 0..=source.y as u32 {
                let py = image_rect.top() + y as f32 * scale;
                if rect.y_range().contains(py) {
                    p.hline(
                        image_rect.x_range(),
                        py,
                        Stroke::new(0.5_f32, Color32::from_black_alpha(45)),
                    );
                }
            }
        }
        let response = ui.interact(rect, ui.id().with("image-drag"), Sense::click_and_drag());
        let paint =
            self.tool == Tool::Background && !self.show_original && self.selected_frame == 0;
        if paint {
            if let Some(pointer) = response.hover_pos().filter(|pos| image_rect.contains(*pos)) {
                p.circle_stroke(
                    pointer,
                    self.brush as f32 * fit * self.zoom,
                    Stroke::new(1.3_f32, d::BLUE),
                );
            }
            if response.drag_started() || response.clicked() {
                self.checkpoint();
            }
            if response.dragged() || response.clicked() {
                if let Some(pointer) = response
                    .interact_pointer_pos()
                    .filter(|pos| image_rect.contains(*pos))
                {
                    let relative = (pointer - image_rect.min) / image_rect.size();
                    if let Some(layer) = self.document.as_mut().and_then(Document::active_layer_mut)
                    {
                        effects::paint_mask(
                            &mut layer.pixels,
                            (relative.x * source.x) as i32,
                            (relative.y * source.y) as i32,
                            self.brush,
                            self.restore,
                        );
                    }
                    self.refresh(ctx);
                }
            }
        } else if response.dragged() {
            self.pan += response.drag_delta();
        }
        let scroll = ctx.input(|input| input.smooth_scroll_delta.y);
        if response.hovered() && scroll != 0.0 {
            self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.1, 16.0);
        }
    }
    fn footer(&mut self, ctx: &Context) {
        egui::TopBottomPanel::bottom("footer")
            .exact_height(34.0)
            .frame(
                Frame::NONE
                    .fill(d::BG)
                    .inner_margin(Margin::symmetric(22, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    d::caption(ui, &self.status);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some((w, h)) = self.document.as_ref().and_then(Document::dimensions)
                        {
                            d::caption(ui, &format!("{w} × {h} px"));
                        } else {
                            d::caption(ui, "Pixelify Studio");
                        }
                    });
                });
            });
        if self.document.is_some() {
            egui::TopBottomPanel::bottom("timeline")
                .exact_height(100.0)
                .frame(
                    Frame::NONE
                        .fill(Color32::WHITE)
                        .inner_margin(Margin::symmetric(22, 12)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            d::eyebrow(ui, "FRAMES");
                            ui.add(
                                DragValue::new(&mut self.frame_duration)
                                    .range(10..=5000)
                                    .suffix(" ms"),
                            );
                        });
                        ui.add_space(8.0);
                        let count = self
                            .document
                            .as_ref()
                            .map_or(0, |doc| doc.animation_frames.len())
                            + 1;
                        let mut select = None;
                        ScrollArea::horizontal()
                            .max_width(ui.available_width() - 142.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for index in 0..count {
                                        let (r, response) = ui
                                            .allocate_exact_size(vec2(66.0, 62.0), Sense::click());
                                        let selected = index == self.selected_frame;
                                        ui.painter().rect_filled(
                                            r,
                                            8.0,
                                            if selected { d::TINT } else { d::BG },
                                        );
                                        ui.painter().rect_stroke(
                                            r,
                                            8.0,
                                            Stroke::new(
                                                1.0_f32,
                                                if selected { d::BLUE } else { d::LINE },
                                            ),
                                            StrokeKind::Inside,
                                        );
                                        if let Some(thumb) = self.thumbnails.get(index) {
                                            ui.painter().image(
                                                thumb.id(),
                                                Rect::from_center_size(
                                                    r.center() - vec2(0.0, 7.0),
                                                    vec2(48.0, 36.0),
                                                ),
                                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                        ui.painter().text(
                                            pos2(r.center().x, r.bottom() - 12.0),
                                            Align2::CENTER_CENTER,
                                            format!("{:02}", index + 1),
                                            FontId::proportional(10.0),
                                            d::MUTED,
                                        );
                                        if response.clicked() {
                                            select = Some(index);
                                        }
                                    }
                                });
                            });
                        if d::button(ui, "Capturer", Some(Icon::Plus), B::Secondary, 116.0)
                            .on_hover_text("Ajouter une copie du rendu actuel à la timeline")
                            .clicked()
                        {
                            self.checkpoint();
                            if let Some(doc) = &mut self.document {
                                doc.capture_frame(self.frame_duration);
                            }
                            self.refresh(ctx);
                            self.status = "Frame capturée".into();
                        }
                        if let Some(index) = select {
                            self.selected_frame = index;
                            self.show_original = false;
                            self.refresh(ctx);
                        }
                    });
                });
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_lpc_library(ctx);
        if self.tool == Tool::Character {
            self.advance_lpc_preview(ctx);
        }
        for file in ctx.input(|input| input.raw.dropped_files.clone()) {
            if let Some(path) = file.path {
                self.load_path(ctx, path);
                break;
            }
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(Key::Z)) {
            if ctx.input(|input| input.modifiers.shift) {
                self.redo(ctx);
            } else {
                self.undo(ctx);
            }
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(Key::S)) {
            self.save();
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(Key::O)) {
            self.import(ctx);
        }
        self.header(ctx);
        self.sidebar(ctx);
        self.inspector(ctx);
        self.footer(ctx);
        if let Some(doc) = &mut self.document {
            doc.pixel_scale = self.block_size;
            doc.palette_size = self.colors;
        }
        CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(d::BG)
                    .inner_margin(Margin::symmetric(24, 22)),
            )
            .show(ctx, |ui| self.workspace(ui, ctx));
        if self.help {
            let mut open = true;
            Window::new("Aide & raccourcis")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    d::title(ui, "Votre atelier en quelques gestes");
                    d::caption(ui, "Déposez une image, choisissez un style puis exportez.");
                    d::divider(ui);
                    for (keys, action) in [
                        ("Cmd / Ctrl + O", "Importer"),
                        ("Cmd / Ctrl + S", "Enregistrer"),
                        ("Cmd / Ctrl + Z", "Annuler"),
                        ("Cmd / Ctrl + Shift + Z", "Rétablir"),
                        ("Molette", "Zoomer sur le canevas"),
                        ("Glisser", "Déplacer l’image hors du mode pinceau"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(keys);
                            d::caption(ui, action);
                        });
                    }
                });
            self.help = open;
        }
        ctx.request_repaint_after(if self.tool == Tool::Character && self.lpc_playing {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(250)
        });
    }
}
fn document_bytes(doc: &Document) -> usize {
    doc.layers
        .iter()
        .chain(doc.animation_frames.iter().flat_map(|f| &f.layers))
        .map(|l| l.pixels.as_raw().len())
        .sum()
}
fn set_texture(ctx: &Context, handle: &mut Option<TextureHandle>, name: &str, pixels: &RgbaImage) {
    let color = ColorImage::from_rgba_unmultiplied(
        [pixels.width() as usize, pixels.height() as usize],
        pixels.as_raw(),
    );
    if let Some(handle) = handle {
        handle.set(color, TextureOptions::NEAREST);
    } else {
        *handle = Some(ctx.load_texture(name, color, TextureOptions::NEAREST));
    }
}
fn checkerboard(p: &Painter, rect: Rect, side: f32) {
    for y in 0..(rect.height() / side).ceil() as usize {
        for x in 0..(rect.width() / side).ceil() as usize {
            let tile = Rect::from_min_size(
                rect.min + vec2(x as f32 * side, y as f32 * side),
                vec2(side, side),
            )
            .intersect(rect);
            p.rect_filled(
                tile,
                0.0,
                if (x + y) % 2 == 0 {
                    Color32::from_rgb(238, 240, 245)
                } else {
                    Color32::WHITE
                },
            );
        }
    }
}
fn preset_palette(index: usize) -> &'static [[u8; 3]] {
    match index {
        1 => &[[15, 56, 15], [48, 98, 48], [139, 172, 15], [155, 188, 15]],
        2 => &[
            [0, 0, 0],
            [29, 43, 83],
            [126, 37, 83],
            [0, 135, 81],
            [171, 82, 54],
            [95, 87, 79],
            [194, 195, 199],
            [255, 241, 232],
            [255, 0, 77],
            [255, 163, 0],
            [255, 236, 39],
            [0, 228, 54],
            [41, 173, 255],
            [131, 118, 156],
            [255, 119, 168],
            [255, 204, 170],
        ],
        _ => &[[24, 30, 47], [248, 249, 253]],
    }
}
/// A small, bundled procedural example so onboarding can be explored without a download.
fn demo_image() -> RgbaImage {
    RgbaImage::from_fn(640, 480, |x, y| {
        let x = x / 4 * 4;
        let y = y / 4 * 4;
        let t = y as f32 / 480.0;
        let mut rgb = [
            (237.0 - 95.0 * t) as u8,
            (160.0 - 70.0 * t) as u8,
            (159.0 + 32.0 * t) as u8,
        ];
        if (x as i32 - 440).pow(2) + (y as i32 - 124).pow(2) < 58 * 58 {
            rgb = [255, 220, 170];
        }
        let ridge = 210.0 + (x as f32 * 0.021).sin() * 36.0 + (x as f32 * 0.012).cos() * 18.0;
        if y as f32 > ridge {
            rgb = [105, 105, 153];
        }
        let near = 277.0 + (x as f32 * 0.018 + 1.3).sin() * 32.0;
        if y as f32 > near {
            rgb = [61, 75, 118];
        }
        if y > 344 {
            rgb = [71, 95, 130];
            if y % 20 < 4 && x > 330 + (y - 344) / 3 && x < 540 - (y - 344) / 3 {
                rgb = [185, 144, 147];
            }
        }
        if x < 112 && y > 250 + x / 2 || x > 572 && y > 294 {
            rgb = [32, 49, 82];
        }
        Rgba([rgb[0], rgb[1], rgb[2], 255])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_history_never_discards_the_open_document() {
        let ctx = Context::default();
        let mut studio = Studio::with_context(&ctx);
        studio.document = Some(Document::from_image("test", RgbaImage::new(8, 8)));
        studio.undo(&ctx);
        studio.redo(&ctx);
        assert!(studio.document.is_some());
    }
    #[test]
    fn undo_and_redo_restore_pixels() {
        let ctx = Context::default();
        let mut studio = Studio::with_context(&ctx);
        studio.document = Some(Document::from_image(
            "test",
            RgbaImage::from_pixel(8, 8, Rgba([20, 40, 60, 255])),
        ));
        studio.checkpoint();
        studio.document.as_mut().unwrap().layers[0]
            .pixels
            .put_pixel(0, 0, Rgba([90, 80, 70, 0]));
        studio.undo(&ctx);
        assert_eq!(
            studio.document.as_ref().unwrap().layers[0]
                .pixels
                .get_pixel(0, 0),
            &Rgba([20, 40, 60, 255])
        );
        studio.redo(&ctx);
        assert_eq!(
            studio.document.as_ref().unwrap().layers[0]
                .pixels
                .get_pixel(0, 0),
            &Rgba([90, 80, 70, 0])
        );
    }
}
