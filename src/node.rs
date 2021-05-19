use crate::as_any_cast;
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite};

// TODO:  Should we go through the pain to make alloc-less async traits?
// Can follow tokio's model, maybe a crate like`async-trait-ext` can help, or just do it manually?
#[async_trait::async_trait]
pub trait Node: as_any_cast::AsAnyCast + Send + Sync + 'static {
	// fn poll_read(
	// 	&self,
	// 	ctx: &mut Context,
	// ) -> Poll<Option<Pin<Box<dyn AsyncRead + Unpin + 'static>>>>;
	// fn poll_write(
	// 	&self,
	// 	ctx: &mut Context,
	// ) -> Poll<Option<Pin<Box<dyn AsyncWrite + Unpin + 'static>>>>;
	// fn poll_seek(
	// 	&self,
	// 	ctx: &mut Context,
	// ) -> Poll<Option<Pin<Box<dyn AsyncSeek + Unpin + 'static>>>>;
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)>;
	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)>;
	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)>;
}

impl dyn Node {
	pub fn downcast_ref<T: Node>(&self) -> Option<&T> {
		self.as_any().downcast_ref()
	}

	pub fn downcast_mut<T: Node>(&mut self) -> Option<&mut T> {
		self.as_any_mut().downcast_mut()
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use crate::tests::*;

	#[test]
	fn node_access() {
		let mut vfs = Vfs::empty_with_capacity(10);
		vfs.add_default_schemes().unwrap();
	}
}
