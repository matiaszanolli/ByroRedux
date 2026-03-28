//! Built-in engine components.

pub mod camera;
pub mod name;
pub mod transform;

pub use camera::{ActiveCamera, Camera};
pub use name::Name;
pub use transform::Transform;
