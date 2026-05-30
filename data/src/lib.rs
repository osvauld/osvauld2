//! The data layer.
//!
//! Each file is **one CRDT document** — a *layer* — and the layer is the unit of
//! sync and (later) of sharing. A layer is a `loro::LoroDoc` persisted into
//! `storage::Store`. The folder/file *namespace* shown on top is not authoritative:
//! it's a separate index layer over the flat set of content layers (files point at
//! their parent folder; folders never enumerate their children).

mod error;
mod layer;

pub use error::DataError;
pub use layer::Layer;
