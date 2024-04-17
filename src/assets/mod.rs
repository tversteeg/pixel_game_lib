//! Custom asset loading.
//!
//! Assets will be embedded into the binary when the `hot-reloading-assets` feature flag is disabled or a WASM build is made, since there's no filesystem there.
//!
//! Asset loading is done through the various calls in [`crate::Context`].

mod audio;
mod image;
mod loader;

use std::{
    any::{Any, TypeId},
    cell::{Cell, UnsafeCell},
    ops::Deref,
    rc::Rc,
};

/// Asset manager files re-exported for the macro.
#[doc(hidden)]
pub use assets_manager::source;
/// Re-export [`assets_manager`] types for loading and defining custom assets.
pub use assets_manager::{asset::*, loader::*, BoxedError};
pub use audio::{Audio, AudioLoader};
pub use image::{Image, ImageLoader};

use assets_manager::{source::Empty, AssetReadGuard, LocalAssetCache};
use hashbrown::HashMap;
use loader::Loader;
use nanoserde::DeJson;
use smol_str::SmolStr;

use crate::{font::Font, sprite::Sprite};

/// Asset ID.
///
/// When the string is smaller than 23 bytes this will be stored on the stack.
pub type Id = SmolStr;

/// Reference to a single asset.
pub type AssetRef<'a, T> = AssetReadGuard<'a, T>;

/// Define the asset types when hot-reloading is enabled.
#[cfg(not(any(
    target_arch = "wasm32",
    not(feature = "hot-reloading-assets"),
    not(doctest)
)))]
#[doc(hidden)]
pub type AssetCacheSource = assets_manager::source::FileSystem;

/// Define the asset types when all assets should be embedded.
#[cfg(any(
    target_arch = "wasm32",
    not(feature = "hot-reloading-assets"),
    not(doctest)
))]
#[doc(hidden)]
pub type AssetCacheSource = assets_manager::source::Embedded<'static>;

/// Any asset that's loadable from any amount of binary files.
pub trait Loadable {
    /// Convert a file object to this type.
    fn load(asset_source: &AssetSource) -> Self
    where
        Self: Sized;
}

/*
/// Type erased storage for any type implementing [`Storable`].
pub(crate) struct Untyped {
    /// Value on the heap which can contain anything.
    ///
    /// We can't make this `Box<dyn Storable>` because `Storable: Sized`.
    value: Box<dyn Loadable + 'static>,
    type_id: TypeId,
}

impl Untyped {
    /// Create storage for any type implementing [`Storable`].
    pub(crate) fn new<T: Loadable>(value: T) -> Self {
        let value = Box::new(value);
        let type_id = TypeId::of::<T>();

        Self { value, type_id }
    }

    /// Get the asset.
    ///
    /// # Panics
    ///
    /// - When type used to insert item differs from type used to get it.
    #[inline]
    #[track_caller]
    pub(crate) fn get<T: Loadable>(&self) -> &T {
        if self.type_id != TypeId::of::<T>() {
            panic!(
                "Type mismatch in asset storage, got {:?} but requested {:?}",
                self.type_id,
                TypeId::of::<T>()
            );
        }

        self.value.as_any().downcast_ref::<T>()
    }
}
*/

/// Global asset manager for a single type known at compile time.
///
/// When hot-reloading is enabled all assets are loaded from disk, otherwise all assets are embedded in the binary.
///
/// Improves performance because the types don't need to be boxed inside the vector.
pub(crate) struct AssetManager<T: Loadable> {
    /// All loaded assets.
    assets: HashMap<Id, Rc<T>>,
}

impl<T: Loadable> AssetManager<T> {
    /// Get an asset or throw an exception.
    #[inline]
    #[track_caller]
    pub(crate) fn get(&mut self, id: &Id, source: &AssetSource) -> Rc<T> {
        if let Some(asset) = self.assets.get(id) {
            // Asset found, return it
            asset.clone()
        } else {
            // Load the asset
            let asset = Rc::new(T::load(source));

            // Store the asset so it can be accessed later again
            self.assets.insert(id.to_owned(), asset.clone());

            asset
        }
    }
}

impl<T: Loadable> Default for AssetManager<T> {
    fn default() -> Self {
        let assets = HashMap::new();

        Self { assets }
    }
}

/// Assets for all types used in- and outside the engine.
pub(crate) struct AssetsManager {
    /// Sprite assets.
    pub(crate) sprites: AssetManager<Sprite>,
    /// Font assets.
    pub(crate) fonts: AssetManager<Font>,
    /// Audio assets.
    pub(crate) audio: AssetManager<Audio>,
    /// Source for all un-loaded assets.
    pub(crate) source: AssetSource,
}

impl AssetsManager {
    /// Create from an asset source.
    pub(crate) fn new(source: AssetSource) -> Self {
        let sprites = AssetManager::default();
        let fonts = AssetManager::default();
        let audio = AssetManager::default();

        Self {
            sprites,
            fonts,
            audio,
            source,
        }
    }

    /// Get or load a sprite.
    ///
    /// # Panics
    ///
    /// - When sprite asset could not be loaded.
    #[inline]
    pub(crate) fn sprite(&mut self, id: impl Into<Id>) -> Rc<Sprite> {
        self.sprites.get(&id.into(), &self.source)
    }

    /// Get or load a font.
    ///
    /// # Panics
    ///
    /// - When font asset could not be loaded.
    #[inline]
    pub(crate) fn font(&mut self, id: impl Into<Id>) -> Rc<Font> {
        self.fonts.get(&id.into(), &self.source)
    }

    /// Get or load an audio file.
    ///
    /// # Panics
    ///
    /// - When audio asset could not be loaded.
    #[inline]
    pub(crate) fn audio(&mut self, id: impl Into<Id>) -> Rc<Audio> {
        self.audio.get(&id.into(), &self.source)
    }
}

/// Asset source for all assets embedded in the binary.
#[doc(hidden)]
pub struct AssetSource {
    /// All loaded assets by parsed path.
    pub assets: &'static [StaticRawAsset],
}

impl AssetSource {
    /// Get an asset.
    #[track_caller]
    pub fn asset(&self, id: &Id, extension: &str) -> Option<&'static [u8]> {
        self.assets.iter().find_map(|raw_asset| {
            if raw_asset.id == id && raw_asset.extension == extension {
                Some(raw_asset.bytes)
            } else {
                None
            }
        })
    }
}

/// Single embedded asset in the binary.
#[doc(hidden)]
pub struct StaticRawAsset {
    /// Parsed ID, excludes the file extension.
    pub id: &'static str,
    /// File extension excluding the first `'.'`.
    pub extension: &'static str,
    /// Raw bytes of the asset.
    pub bytes: &'static [u8],
}
