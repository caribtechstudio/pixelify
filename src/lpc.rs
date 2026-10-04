//! Local reader and compositor for a checkout of the Universal LPC generator.
//!
//! Pixelify deliberately does not bundle upstream art or its GPL application
//! code. The user chooses their own local checkout; this module reads the
//! published JSON definitions and PNG files in that checkout.
use crate::{document::Layer, Document};
use image::{imageops, RgbaImage};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const SHEET_WIDTH: u32 = 13 * 64;
pub const SHEET_HEIGHT: u32 = 54 * 64;

#[derive(Debug, Error)]
pub enum LpcError {
    #[error("bibliothèque LPC introuvable : choisissez le dossier racine du dépôt ou son dossier spritesheets")]
    InvalidLibrary,
    #[error("aucune définition LPC exploitable n’a été trouvée")]
    EmptyCatalog,
    #[error("aucun élément sélectionné")]
    EmptySelection,
    #[error("le PNG requis est absent : {0}")]
    MissingSprite(String),
    #[error("impossible de lire {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("définition invalide {path}: {source}")]
    Definition {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

#[derive(Clone, Debug)]
pub struct Credit {
    pub file: String,
    pub notes: String,
    pub authors: Vec<String>,
    pub licenses: Vec<String>,
    pub urls: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CatalogItem {
    pub id: String,
    pub name: String,
    pub type_name: String,
    pub category: String,
    pub variants: Vec<String>,
    pub body_types: Vec<String>,
    layers: Vec<ItemLayer>,
    animations: Option<Vec<String>>,
    credits: Vec<Credit>,
    variant_filename: bool,
}

#[derive(Clone, Debug)]
struct ItemLayer {
    z_pos: i32,
    paths: BTreeMap<String, String>,
    custom_animation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub item_id: String,
    pub variant: String,
}

#[derive(Clone, Debug)]
pub struct Catalog {
    root: PathBuf,
    spritesheets: PathBuf,
    pub items: Vec<CatalogItem>,
}

#[derive(Deserialize)]
struct RawItem {
    name: Option<String>,
    type_name: Option<String>,
    #[serde(default)]
    variants: Vec<String>,
    animations: Option<Vec<String>>,
    #[serde(default)]
    credits: Vec<RawCredit>,
    #[serde(flatten)]
    fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct RawLayer {
    #[serde(rename = "zPos")]
    z_pos: Option<i32>,
    custom_animation: Option<String>,
    #[serde(flatten)]
    fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct RawCredit {
    file: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    licenses: Vec<String>,
    #[serde(default)]
    urls: Vec<String>,
}

impl Catalog {
    /// Opens the upstream repository root, or its `spritesheets` child.
    pub fn open(chosen: &Path) -> Result<Self, LpcError> {
        let root = if chosen.join("sheet_definitions").is_dir() {
            chosen.to_path_buf()
        } else if chosen
            .file_name()
            .is_some_and(|name| name == "spritesheets")
            && chosen
                .parent()
                .is_some_and(|parent| parent.join("sheet_definitions").is_dir())
        {
            chosen.parent().unwrap().to_path_buf()
        } else {
            return Err(LpcError::InvalidLibrary);
        };
        let definitions = root.join("sheet_definitions");
        let spritesheets = root.join("spritesheets");
        if !spritesheets.is_dir() {
            return Err(LpcError::InvalidLibrary);
        }
        let mut files = Vec::new();
        collect_json(&definitions, &mut files)?;
        let mut items = Vec::new();
        for file in files {
            if file
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("meta_"))
            {
                continue;
            }
            let content = fs::read_to_string(&file).map_err(|source| LpcError::Read {
                path: file.display().to_string(),
                source,
            })?;
            let raw: RawItem =
                serde_json::from_str(&content).map_err(|source| LpcError::Definition {
                    path: file.display().to_string(),
                    source,
                })?;
            let Some(name) = raw.name else { continue };
            let mut layers = Vec::new();
            let mut body_types = BTreeSet::new();
            for index in 1..=9 {
                let Some(value) = raw.fields.get(&format!("layer_{index}")) else {
                    continue;
                };
                let layer: RawLayer = serde_json::from_value(value.clone()).map_err(|source| {
                    LpcError::Definition {
                        path: file.display().to_string(),
                        source,
                    }
                })?;
                let paths = layer
                    .fields
                    .into_iter()
                    .filter_map(|(body, value)| {
                        value.as_str().map(|path| {
                            body_types.insert(body.clone());
                            (body, path.to_string())
                        })
                    })
                    .collect();
                layers.push(ItemLayer {
                    z_pos: layer.z_pos.unwrap_or(100),
                    paths,
                    custom_animation: layer.custom_animation,
                });
            }
            if layers.is_empty() {
                continue;
            }
            let relative = file.strip_prefix(&definitions).unwrap_or(&file);
            let id = relative
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let category = relative
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_string_lossy()
                .replace('\\', "/");
            let type_name = raw.type_name.unwrap_or_else(|| id.clone());
            let variant_filename = !raw.variants.is_empty();
            items.push(CatalogItem {
                id,
                name,
                type_name,
                category,
                variants: if raw.variants.is_empty() {
                    vec!["base".into()]
                } else {
                    raw.variants
                },
                body_types: body_types.into_iter().collect(),
                layers,
                animations: raw.animations,
                credits: raw
                    .credits
                    .into_iter()
                    .map(|credit| Credit {
                        file: credit.file,
                        notes: credit.notes,
                        authors: credit.authors,
                        licenses: credit.licenses,
                        urls: credit.urls,
                    })
                    .collect(),
                variant_filename,
            });
        }
        items.sort_by(|a, b| {
            a.type_name
                .cmp(&b.type_name)
                .then_with(|| a.name.cmp(&b.name))
        });
        if items.is_empty() {
            return Err(LpcError::EmptyCatalog);
        }
        Ok(Self {
            root,
            spritesheets,
            items,
        })
    }

    pub fn types(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| item.type_name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Folder-based groups from the upstream sheet definitions. They match the
    /// way the source library is organized (body, clothes, hair, weapons…).
    pub fn categories(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| item.category.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn item(&self, id: &str) -> Option<&CatalogItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn default_body(&self) -> Option<Selection> {
        self.items
            .iter()
            .find(|item| item.id == "body/body" || item.type_name == "body")
            .map(|item| Selection {
                item_id: item.id.clone(),
                variant: item
                    .variants
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "base".into()),
            })
    }

    pub fn compose(&self, selections: &[Selection], body_type: &str) -> Result<Document, LpcError> {
        if selections.is_empty() {
            return Err(LpcError::EmptySelection);
        }
        let mut selected = Vec::new();
        for selection in selections {
            if let Some(item) = self.item(&selection.item_id) {
                selected.push((item, selection));
            }
        }
        if selected.is_empty() {
            return Err(LpcError::EmptySelection);
        }
        let mut drawable = Vec::new();
        let mut credits = Vec::new();
        for (item, selection) in selected {
            credits.extend(item.credits.clone());
            for layer in &item.layers {
                if layer.custom_animation.is_some() {
                    continue;
                }
                let Some(base_path) = layer.paths.get(body_type) else {
                    continue;
                };
                let mut image = RgbaImage::new(SHEET_WIDTH, SHEET_HEIGHT);
                for animation in animations_for(item.animations.as_deref()) {
                    let path = if item.variant_filename {
                        self.spritesheets.join(format!(
                            "{}{}/{}.png",
                            base_path,
                            animation.folder,
                            selection.variant.replace(' ', "_")
                        ))
                    } else {
                        self.spritesheets
                            .join(format!("{}{}.png", base_path, animation.folder))
                    };
                    if !path.is_file() {
                        // A missing animation is normal for some compatible combinations.
                        continue;
                    }
                    let sprite = image::open(path)?.to_rgba8();
                    imageops::overlay(&mut image, &sprite, 0, i64::from(animation.y()));
                }
                drawable.push((
                    layer.z_pos,
                    Layer {
                        name: format!("{} · {}", item.name, selection.variant),
                        pixels: image,
                        visible: true,
                        opacity: 255,
                    },
                ));
            }
        }
        drawable.sort_by_key(|(z_pos, _)| *z_pos);
        let layers = drawable
            .into_iter()
            .map(|(_, layer)| layer)
            .collect::<Vec<_>>();
        if layers.is_empty() {
            return Err(LpcError::MissingSprite(
                "aucun PNG compatible avec cette morphologie et ces choix".into(),
            ));
        }
        Ok(Document {
            layers,
            active_layer: 0,
            pixel_scale: 1,
            palette_size: 0,
            dither: false,
            animation_frames: Vec::new(),
            credits: format_credits(&credits),
        })
    }
}

/// A standard animation location in the Universal LPC 13 × 54 frame sheet.
///
/// `row_count` is four for the usual north/west/south/east directions and
/// one for the directionless `hurt` and `climb` sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Animation {
    id: &'static str,
    label: &'static str,
    folder: &'static str,
    row: u32,
    row_count: u32,
    cycle: &'static [u32],
}

impl Animation {
    pub const fn id(self) -> &'static str {
        self.id
    }

    pub const fn label(self) -> &'static str {
        self.label
    }

    pub const fn row(self) -> u32 {
        self.row
    }

    pub const fn row_count(self) -> u32 {
        self.row_count
    }

    pub const fn cycle(self) -> &'static [u32] {
        self.cycle
    }

    const fn y(self) -> u32 {
        self.row * 64
    }
}

const SPELLCAST_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5, 6];
const THRUST_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7];
const WALK_CYCLE: &[u32] = &[1, 2, 3, 4, 5, 6, 7, 8];
const SLASH_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5];
const SHOOT_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
const HURT_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5];
const IDLE_CYCLE: &[u32] = &[0, 0, 1];
const JUMP_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 1];
const SIT_EMOTE_CYCLE: &[u32] = &[0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2];
const RUN_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7];
const BACKSLASH_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12];
const HALFSLASH_CYCLE: &[u32] = &[0, 1, 2, 3, 4, 5];

const ANIMATIONS: [Animation; 15] = [
    Animation {
        id: "spellcast",
        label: "Incantation",
        folder: "spellcast",
        row: 0,
        row_count: 4,
        cycle: SPELLCAST_CYCLE,
    },
    Animation {
        id: "thrust",
        label: "Estoc",
        folder: "thrust",
        row: 4,
        row_count: 4,
        cycle: THRUST_CYCLE,
    },
    Animation {
        id: "walk",
        label: "Marche",
        folder: "walk",
        row: 8,
        row_count: 4,
        cycle: WALK_CYCLE,
    },
    Animation {
        id: "slash",
        label: "Attaque",
        folder: "slash",
        row: 12,
        row_count: 4,
        cycle: SLASH_CYCLE,
    },
    Animation {
        id: "shoot",
        label: "Tir",
        folder: "shoot",
        row: 16,
        row_count: 4,
        cycle: SHOOT_CYCLE,
    },
    Animation {
        id: "hurt",
        label: "Impact",
        folder: "hurt",
        row: 20,
        row_count: 1,
        cycle: HURT_CYCLE,
    },
    Animation {
        id: "climb",
        label: "Escalade",
        folder: "climb",
        row: 21,
        row_count: 1,
        cycle: HURT_CYCLE,
    },
    Animation {
        id: "idle",
        label: "Repos",
        folder: "idle",
        row: 22,
        row_count: 4,
        cycle: IDLE_CYCLE,
    },
    Animation {
        id: "jump",
        label: "Saut",
        folder: "jump",
        row: 26,
        row_count: 4,
        cycle: JUMP_CYCLE,
    },
    Animation {
        id: "sit",
        label: "Assis",
        folder: "sit",
        row: 30,
        row_count: 4,
        cycle: SIT_EMOTE_CYCLE,
    },
    Animation {
        id: "emote",
        label: "Émote",
        folder: "emote",
        row: 34,
        row_count: 4,
        cycle: SIT_EMOTE_CYCLE,
    },
    Animation {
        id: "run",
        label: "Course",
        folder: "run",
        row: 38,
        row_count: 4,
        cycle: RUN_CYCLE,
    },
    Animation {
        id: "combat",
        label: "Garde",
        folder: "combat_idle",
        row: 42,
        row_count: 4,
        cycle: IDLE_CYCLE,
    },
    Animation {
        id: "1h_backslash",
        label: "Revers à une main",
        folder: "backslash",
        row: 46,
        row_count: 4,
        cycle: BACKSLASH_CYCLE,
    },
    Animation {
        id: "1h_halfslash",
        label: "Demi-attaque à une main",
        folder: "halfslash",
        row: 50,
        row_count: 4,
        cycle: HALFSLASH_CYCLE,
    },
];

/// All standard animations that can be displayed from the generated sheet.
pub fn standard_animations() -> &'static [Animation] {
    &ANIMATIONS
}

/// Finds a standard sheet animation by its upstream identifier.
pub fn standard_animation(id: &str) -> Option<Animation> {
    ANIMATIONS
        .iter()
        .copied()
        .find(|animation| animation.id == id)
}

/// Extracts one animation moment as a horizontal strip of its available
/// directions. This matches the upstream preview order: north, west, south,
/// east. `hurt` and `climb` contain one direction only in the LPC layout.
pub fn animation_preview(
    sheet: &RgbaImage,
    animation_id: &str,
    cycle_index: usize,
) -> Option<RgbaImage> {
    let animation = standard_animation(animation_id)?;
    let cycle = animation.cycle();
    let frame = *cycle.get(cycle_index % cycle.len())?;
    let source_y = animation.row() * 64;
    if sheet.width() < (frame + 1) * 64 || sheet.height() < source_y + animation.row_count() * 64 {
        return None;
    }
    let mut preview = RgbaImage::new(64 * animation.row_count(), 64);
    for direction in 0..animation.row_count() {
        let frame =
            imageops::crop_imm(sheet, frame * 64, source_y + direction * 64, 64, 64).to_image();
        imageops::overlay(&mut preview, &frame, i64::from(direction * 64), 0);
    }
    Some(preview)
}

fn animations_for(configured: Option<&[String]>) -> Vec<Animation> {
    let defaults = ["spellcast", "thrust", "walk", "slash", "shoot", "hurt"];
    let supported: Vec<&str> = configured
        .map(|values| values.iter().map(String::as_str).collect())
        .unwrap_or_else(|| defaults.to_vec());
    ANIMATIONS
        .into_iter()
        .filter(|animation| {
            supported.contains(&animation.id)
                || (animation.id == "1h_backslash" && supported.contains(&"1h_slash"))
        })
        .collect()
}

fn collect_json(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), LpcError> {
    for entry in fs::read_dir(directory).map_err(|source| LpcError::Read {
        path: directory.display().to_string(),
        source,
    })? {
        let path = entry
            .map_err(|source| LpcError::Read {
                path: directory.display().to_string(),
                source,
            })?
            .path();
        if path.is_dir() {
            collect_json(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn format_credits(credits: &[Credit]) -> String {
    let mut unique = HashMap::<String, &Credit>::new();
    for credit in credits {
        unique.entry(credit.file.clone()).or_insert(credit);
    }
    let mut values = unique.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| a.file.cmp(&b.file));
    let mut output = String::from("Character assets from the Universal LPC Spritesheet Character Generator.\nhttps://github.com/LiberatedPixelCup/Universal-LPC-Spritesheet-Character-Generator\n\n");
    for credit in values {
        output.push_str(&format!(
            "Asset: {}\nAuthors: {}\nLicenses: {}\n",
            credit.file,
            credit.authors.join(", "),
            credit.licenses.join(", ")
        ));
        if !credit.notes.is_empty() {
            output.push_str(&format!("Notes: {}\n", credit.notes));
        }
        if !credit.urls.is_empty() {
            output.push_str(&format!("Sources: {}\n", credit.urls.join(" | ")));
        }
        output.push('\n');
    }
    output
}
