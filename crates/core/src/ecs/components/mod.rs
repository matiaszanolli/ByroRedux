//! Built-in engine components.

pub mod animated;
pub mod bsx;
pub mod camera;
pub mod collision;
pub mod form_id;
pub mod global_transform;
pub mod hierarchy;
pub mod light;
pub mod material;
pub mod mesh;
pub mod name;
pub mod scene_flags;
pub mod skinned_mesh;
pub mod texture;
pub mod transform;
pub mod world_bound;

pub use animated::{AnimatedAlpha, AnimatedColor, AnimatedVisibility};
pub use bsx::{BSBound, BSXFlags};
pub use camera::{ActiveCamera, Camera};
pub use collision::{CollisionShape, MotionType, RigidBodyData};
pub use form_id::FormIdComponent;
pub use global_transform::GlobalTransform;
pub use hierarchy::{Children, Parent};
pub use light::LightSource;
pub use material::Material;
pub use mesh::MeshHandle;
pub use name::Name;
pub use scene_flags::SceneFlags;
pub use skinned_mesh::{SkinnedMesh, MAX_BONES_PER_MESH};
pub use texture::TextureHandle;
pub use transform::Transform;
pub use world_bound::WorldBound;
