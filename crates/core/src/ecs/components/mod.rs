//! Built-in engine components.

pub mod camera;
pub mod form_id;
pub mod light;
pub mod mesh;
pub mod name;
pub mod texture;
pub mod transform;

pub use camera::{ActiveCamera, Camera};
pub use form_id::FormIdComponent;
pub use light::LightSource;
pub use mesh::MeshHandle;
pub use name::Name;
pub use texture::TextureHandle;
pub use transform::Transform;
