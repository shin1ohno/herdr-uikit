//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582), Apache-2.0; see NOTICE.md.
//! Source: metal_atlas.rs. Modified for UIKit, checked allocation, and iOS storage.
use super::Error;
use anyhow::Result;
use etagere::BucketedAtlasAllocator;
use gpui::{
    AtlasBackend, AtlasKey, AtlasState, AtlasTextureId, AtlasTextureKind, AtlasTextureList,
    AtlasTile, Bounds, DevicePixels, PlatformAtlas, Point, Size,
};
use metal::{Device, MTLPixelFormat};
use parking_lot::Mutex;
use std::borrow::Cow;

pub(super) struct MetalAtlas(Mutex<AtlasState<MetalAtlasTextures>>);

impl MetalAtlas {
    pub(super) fn new(device: Device) -> Self {
        MetalAtlas(Mutex::new(AtlasState::new(MetalAtlasTextures {
            device,
            monochrome_textures: Default::default(),
            polychrome_textures: Default::default(),
        })))
    }

    pub(super) fn metal_texture(&self, id: AtlasTextureId) -> Option<metal::Texture> {
        self.0
            .lock()
            .backend
            .texture(id)
            .map(|texture| texture.metal_texture.clone())
    }
}

struct MetalAtlasTextures {
    device: Device,
    monochrome_textures: AtlasTextureList<MetalAtlasTexture>,
    polychrome_textures: AtlasTextureList<MetalAtlasTexture>,
}

impl PlatformAtlas for MetalAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.0.lock().get_or_insert_with(key, build)
    }

    fn remove(&self, key: &AtlasKey) {
        self.0.lock().remove(key);
    }
}

impl AtlasBackend for MetalAtlasTextures {
    fn insert(
        &mut self,
        kind: AtlasTextureKind,
        size: Size<DevicePixels>,
        bytes: &[u8],
    ) -> Result<AtlasTile> {
        if kind == AtlasTextureKind::Subpixel {
            return Err(Error::SubpixelUnsupported.into());
        }
        let expected = super::limits::atlas_byte_count(
            size.width.0,
            size.height.0,
            kind == AtlasTextureKind::Monochrome,
        )
        .ok_or(Error::AtlasDimensions {
            width: size.width.0,
            height: size.height.0,
        })?;
        if bytes.len() != expected {
            return Err(Error::AtlasByteCount {
                expected,
                actual: bytes.len(),
            }
            .into());
        }
        let tile = self.allocate(size, kind)?;
        let texture = self
            .texture(tile.texture_id)
            .ok_or(Error::AtlasTextureMissing)?;
        texture.upload(tile.bounds, bytes);
        Ok(tile)
    }

    fn remove(&mut self, tile: AtlasTile) {
        let id = tile.texture_id;

        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            AtlasTextureKind::Subpixel => {
                super::warn_subpixel();
                return;
            }
        };

        let Some(texture_slot) = textures
            .textures
            .iter_mut()
            .find(|texture| texture.as_ref().is_some_and(|v| v.id == id))
        else {
            return;
        };

        if let Some(mut texture) = texture_slot.take() {
            texture.allocator.deallocate(tile.tile_id.into());
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                textures.free_list.push(id.index as usize);
            } else {
                *texture_slot = Some(texture);
            }
        }
    }
}

impl MetalAtlasTextures {
    fn allocate(
        &mut self,
        size: Size<DevicePixels>,
        texture_kind: AtlasTextureKind,
    ) -> super::Result<AtlasTile> {
        {
            let textures = match texture_kind {
                AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
                AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
                AtlasTextureKind::Subpixel => return Err(Error::SubpixelUnsupported),
            };

            if let Some(tile) = textures
                .iter_mut()
                .rev()
                .find_map(|texture| texture.allocate(size))
            {
                return Ok(tile);
            }
        }

        let texture = self.push_texture(size, texture_kind)?;
        texture.allocate(size).ok_or(Error::AtlasAllocation)
    }

    fn push_texture(
        &mut self,
        min_size: Size<DevicePixels>,
        kind: AtlasTextureKind,
    ) -> super::Result<&mut MetalAtlasTexture> {
        const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(1024),
            height: DevicePixels(1024),
        };
        // Max texture size on all modern Apple GPUs. Anything bigger than that crashes in validateWithDevice.
        const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(16384),
            height: DevicePixels(16384),
        };
        let size = min_size.min(&MAX_ATLAS_SIZE).max(&DEFAULT_ATLAS_SIZE);
        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(size.width.into());
        texture_descriptor.set_height(size.height.into());
        let pixel_format;
        let usage;
        match kind {
            AtlasTextureKind::Monochrome => {
                pixel_format = MTLPixelFormat::A8Unorm;
                usage = metal::MTLTextureUsage::ShaderRead;
            }
            AtlasTextureKind::Polychrome => {
                pixel_format = MTLPixelFormat::BGRA8Unorm;
                usage = metal::MTLTextureUsage::ShaderRead;
            }
            AtlasTextureKind::Subpixel => return Err(Error::SubpixelUnsupported),
        }
        texture_descriptor.set_pixel_format(pixel_format);
        texture_descriptor.set_usage(usage);
        texture_descriptor.set_storage_mode(metal::MTLStorageMode::Shared);
        let metal_texture = super::resources::texture(&self.device, &texture_descriptor)?;

        let texture_list = match kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            AtlasTextureKind::Subpixel => return Err(Error::SubpixelUnsupported),
        };

        let index = texture_list.free_list.pop();

        let atlas_texture = MetalAtlasTexture {
            id: AtlasTextureId {
                index: index.unwrap_or(texture_list.textures.len()) as u32,
                kind,
            },
            allocator: BucketedAtlasAllocator::new(size_to_etagere(size)),
            metal_texture,
            bytes_per_pixel: if kind == AtlasTextureKind::Monochrome {
                1
            } else {
                4
            },
            live_atlas_keys: 0,
        };

        if let Some(ix) = index {
            texture_list.textures[ix] = Some(atlas_texture);
            texture_list.textures.get_mut(ix)
        } else {
            texture_list.textures.push(Some(atlas_texture));
            texture_list.textures.last_mut()
        }
        .and_then(Option::as_mut)
        .ok_or(Error::AtlasAllocation)
    }

    fn texture(&self, id: AtlasTextureId) -> Option<&MetalAtlasTexture> {
        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &self.monochrome_textures,
            AtlasTextureKind::Polychrome => &self.polychrome_textures,
            AtlasTextureKind::Subpixel => {
                super::warn_subpixel();
                return None;
            }
        };
        textures
            .textures
            .get(id.index as usize)
            .and_then(Option::as_ref)
    }
}

struct MetalAtlasTexture {
    id: AtlasTextureId,
    allocator: BucketedAtlasAllocator,
    metal_texture: metal::Texture,
    bytes_per_pixel: u8,
    live_atlas_keys: u32,
}

impl MetalAtlasTexture {
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let allocation = self.allocator.allocate(size_to_etagere(size))?;
        let tile = AtlasTile {
            texture_id: self.id,
            tile_id: allocation.id.into(),
            bounds: Bounds {
                origin: point_from_etagere(allocation.rectangle.min),
                size,
            },
            padding: 0,
        };
        self.live_atlas_keys += 1;
        Some(tile)
    }

    fn upload(&self, bounds: Bounds<DevicePixels>, bytes: &[u8]) {
        let region = metal::MTLRegion::new_2d(
            bounds.origin.x.into(),
            bounds.origin.y.into(),
            bounds.size.width.into(),
            bounds.size.height.into(),
        );
        self.metal_texture.replace_region(
            region,
            0,
            bytes.as_ptr() as *const _,
            bounds.size.width.to_bytes(self.bytes_per_pixel) as u64,
        );
    }

    fn decrement_ref_count(&mut self) {
        self.live_atlas_keys -= 1;
    }

    fn is_unreferenced(&mut self) -> bool {
        self.live_atlas_keys == 0
    }
}

fn size_to_etagere(size: Size<DevicePixels>) -> etagere::Size {
    etagere::Size::new(size.width.into(), size.height.into())
}

fn point_from_etagere(value: etagere::Point) -> Point<DevicePixels> {
    Point {
        x: DevicePixels::from(value.x),
        y: DevicePixels::from(value.y),
    }
}
