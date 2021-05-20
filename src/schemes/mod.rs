pub mod data_loader;
#[cfg(feature = "embedded")]
pub mod embedded;
pub mod filesystem;
#[cfg(feature = "in_memory")]
pub mod memory;
pub mod overlay;
pub mod symlink;

pub mod prelude {
	use super::*;
	pub use data_loader::*;
	#[cfg(feature = "embedded")]
	pub use embedded::*;
	pub use filesystem::prelude::*;
	#[cfg(feature = "in_memory")]
	pub use memory::*;
	pub use overlay::*;
	pub use symlink::*;
}
