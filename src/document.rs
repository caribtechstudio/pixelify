use image::RgbaImage;

/// A compositing layer. Pixels are always stored as straight-alpha RGBA.
#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    pub pixels: RgbaImage,
    pub visible: bool,
    pub opacity: u8,
}
#[derive(Clone, Debug)]
pub struct AnimationFrame {
    pub layers: Vec<Layer>,
    pub duration_ms: u16,
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub pixel_scale: u32,
    pub palette_size: u16,
    pub dither: bool,
    pub animation_frames: Vec<AnimationFrame>,
    /// Attribution text carried with a document imported from a licensed asset set.
    pub credits: String,
}

impl Document {
    pub fn from_image(name: impl Into<String>, pixels: RgbaImage) -> Self {
        Self {
            layers: vec![Layer {
                name: name.into(),
                pixels,
                visible: true,
                opacity: 255,
            }],
            active_layer: 0,
            pixel_scale: 8,
            palette_size: 0,
            dither: false,
            animation_frames: Vec::new(),
            credits: String::new(),
        }
    }

    pub fn dimensions(&self) -> Option<(u32, u32)> {
        self.layers.first().map(|layer| layer.pixels.dimensions())
    }

    /// Ensures that all layers can be safely composited into a single canvas.
    pub fn validate(&self) -> Result<(), String> {
        let Some(dimensions) = self.dimensions() else {
            return Err("le document ne contient aucun calque".into());
        };
        if dimensions.0 == 0 || dimensions.1 == 0 {
            return Err("le canevas ne peut pas être vide".into());
        }
        if self
            .layers
            .iter()
            .any(|layer| layer.pixels.dimensions() != dimensions)
        {
            return Err("tous les calques doivent avoir la même taille".into());
        }
        if self.animation_frames.iter().any(|frame| {
            frame.layers.len() != self.layers.len()
                || frame
                    .layers
                    .iter()
                    .any(|layer| layer.pixels.dimensions() != dimensions)
        }) {
            return Err("les frames n’ont pas la même structure que le canevas".into());
        }
        Ok(())
    }

    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        self.layers.get_mut(self.active_layer)
    }

    pub fn active_layer(&self) -> Option<&Layer> {
        self.layers.get(self.active_layer)
    }

    /// Alpha-composite visible layers, from bottom to top.
    pub fn flattened(&self) -> Option<RgbaImage> {
        if self.validate().is_err() {
            return None;
        }
        let (width, height) = self.dimensions()?;
        let mut output = RgbaImage::new(width, height);
        for layer in self.layers.iter().filter(|layer| layer.visible) {
            for (x, y, pixel) in layer.pixels.enumerate_pixels() {
                let src_a = (u16::from(pixel[3]) * u16::from(layer.opacity)) / 255;
                if src_a == 0 {
                    continue;
                }
                let dst = output.get_pixel_mut(x, y);
                let dst_a = u16::from(dst[3]);
                let out_a = src_a + (dst_a * (255 - src_a)) / 255;
                if out_a == 0 {
                    continue;
                }
                for channel in 0..3 {
                    let src = u32::from(pixel[channel]);
                    let dst_value = u32::from(dst[channel]);
                    let numerator = src * u32::from(src_a) * 255
                        + dst_value * u32::from(dst_a) * u32::from(255 - src_a);
                    dst[channel] = (numerator / (u32::from(out_a) * 255)) as u8;
                }
                dst[3] = out_a as u8;
            }
        }
        Some(output)
    }

    pub fn duplicate_active_layer(&mut self) {
        if let Some(layer) = self.active_layer().cloned() {
            let mut copy = layer;
            copy.name.push_str(" copy");
            self.layers.insert(self.active_layer + 1, copy);
            self.active_layer += 1;
        }
    }
    pub fn capture_frame(&mut self, duration_ms: u16) {
        self.animation_frames.push(AnimationFrame {
            layers: self.layers.clone(),
            duration_ms: duration_ms.max(10),
        });
    }
}
