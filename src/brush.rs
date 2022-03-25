use std::{borrow::Cow, hash::BuildHasher};

use glyph_brush::{
    ab_glyph::{Font, FontArc, FontRef, InvalidFont},
    BrushAction, BrushError, DefaultSectionHasher, Extra, Section,
};
use wgpu::CommandBuffer;

use crate::pipeline::{Pipeline, Vertex};

/// Wrapper over [`glyph_brush::GlyphBrush`]. Draws text.
///
/// Used for queuing and rendering text with [`TextBrush::queue`] and [`TextBrush::draw_queued`].
pub struct TextBrush<F = FontArc, H = DefaultSectionHasher> {
    inner: glyph_brush::GlyphBrush<Vertex, Extra, F, H>,
    pipeline: Pipeline,
}

impl<F: Font + Sync, H: BuildHasher> TextBrush<F, H> {
    /// Queues section for drawing. This should be called every frame for every section that is going to be drawn.
    ///
    /// This can be called multiple times for different sections that want to use the
    /// same font and gpu cache.
    #[inline]
    pub fn queue<'a, S>(&mut self, section: S)
    where
        S: Into<Cow<'a, Section<'a>>>,
    {
        self.inner.queue(section);
    }

    /// Draws all queued text and sections with [`queue`](#method.queue) function.
    pub fn draw_queued(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        queue: &wgpu::Queue,
    ) -> CommandBuffer {
        let mut brush_action;

        loop {
            brush_action = self.inner.process_queued(
                |rect, data| self.pipeline.update_texture(rect, data, queue),
                Vertex::to_vertex,
            );

            match brush_action {
                Ok(_) => break,

                // If texture is too small use BrushBuilder::initial_cache_size
                // because resizing texture should be avoided.
                Err(BrushError::TextureTooSmall { suggested }) => {
                    if log::log_enabled!(log::Level::Warn) {
                        log::warn!("Resizing cache texture! This should be avoided by building \
                        TextBrush with BrushBuilder::initial_cache_size() and providing cache texture dimensions.");
                    }
                    let max_image_dimension = device.limits().max_texture_dimension_2d;
                    let (width, height) = if (suggested.0 > max_image_dimension
                        || suggested.1 > max_image_dimension)
                        && (self.inner.texture_dimensions().0 < max_image_dimension
                            || self.inner.texture_dimensions().1 < max_image_dimension)
                    {
                        (max_image_dimension, max_image_dimension)
                    } else {
                        suggested
                    };
                    self.pipeline.resize_texture(device, width, height);
                    self.inner.resize_texture(width, height);
                }
            }
        }

        match brush_action.unwrap() {
            BrushAction::Draw(vertices) => self.pipeline.update(vertices, device, queue),
            BrushAction::ReDraw => (),
        }

        self.pipeline.draw(device, view)
    }

    /// Resizes text rendering pipeline.
    ///
    /// Run this function whenever the surface is resized.
    /// _width_ and _height_ should be **surfaces** dimensions.
    pub fn resize(&mut self, width: f32, height: f32, queue: &wgpu::Queue) {
        self.pipeline.resize(width, height, queue);
    }
}

/// Builder for [`TextBrush`].
pub struct BrushBuilder<F, H = DefaultSectionHasher> {
    inner: glyph_brush::GlyphBrushBuilder<F, H>,
}

impl BrushBuilder<()> {
    /// Creates a [`BrushBuilder`] with [`Font`].
    #[inline]
    pub fn using_font<F: Font>(font: F) -> BrushBuilder<F> {
        BrushBuilder::using_fonts(vec![font])
    }

    /// Creates a [`BrushBuilder`] with font byte data.
    #[inline]
    pub fn using_font_bytes(data: &[u8]) -> Result<BrushBuilder<FontRef>, InvalidFont> {
        let font = FontRef::try_from_slice(data)?;
        Ok(BrushBuilder::using_fonts(vec![font]))
    }

    /// Creates a [`BrushBuilder`] with multiple fonts byte data.
    #[inline]
    pub fn using_font_bytes_vec(data: &[u8]) -> Result<BrushBuilder<FontRef>, InvalidFont> {
        let font = FontRef::try_from_slice(data)?;
        Ok(BrushBuilder::using_fonts(vec![font]))
    }

    /// Creates a [`BrushBuilder`] with multiple [`Font`].
    pub fn using_fonts<F: Font>(fonts: Vec<F>) -> BrushBuilder<F> {
        BrushBuilder {
            inner: glyph_brush::GlyphBrushBuilder::using_fonts(fonts),
        }
    }
}

impl<F: Font, H: BuildHasher> BrushBuilder<F, H> {
    glyph_brush::delegate_glyph_brush_builder_fns!(inner);

    /// Builds a [`TextBrush`] consuming [`BrushBuilder`].
    pub fn build(
        self,
        device: &wgpu::Device,
        render_format: wgpu::TextureFormat,
        width: f32,
        height: f32,
    ) -> TextBrush<F, H> {
        let inner = self.inner.build();
        let pipeline = Pipeline::new(
            device,
            render_format,
            inner.texture_dimensions(),
            (width, height),
        );
        TextBrush { inner, pipeline }
    }
}
