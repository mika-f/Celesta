//! Everything that calls Media Foundation or COM: Windows only.

pub(crate) mod audio;
pub(crate) mod decoder;
pub(crate) mod enumerate;
pub mod gate;
pub(crate) mod media_type;
pub(crate) mod mft;
pub(crate) mod provider;
pub(crate) mod startup;
pub(crate) mod video;
