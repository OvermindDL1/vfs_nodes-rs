use crate::node::CowArcNode;
use crate::{as_any_cast, Node, SchemeError};
use std::pin::Pin;
use tokio::io::AsyncRead;
use url::Url;

#[async_trait::async_trait]
pub trait Scheme: as_any_cast::AsAnyCast + Sync + 'static {
	async fn create_node<'a>(&self, url: &'a Url) -> Result<Box<dyn Node>, SchemeError<'a>>;
	async fn get_node<'a>(&self, url: &'a Url) -> Result<Box<dyn Node>, SchemeError<'a>>;
}

impl dyn Scheme {
	pub fn downcast_ref<T: Scheme>(&self) -> Option<&T> {
		self.as_any().downcast_ref()
	}

	pub fn downcast_mut<T: Scheme>(&mut self) -> Option<&mut T> {
		self.as_any_mut().downcast_mut()
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use crate::tests::*;

	#[test]
	fn node_access() {
		let mut vfs = Vfs::with_capacity(10);
		vfs.add_default_schemes();
	}
}
