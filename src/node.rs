use crate::as_any_cast;
use futures_io::{AsyncRead, AsyncSeek, AsyncWrite};
//use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadHalf, WriteHalf};

// pub type CowArcNode<'a> = Cow<'a, ArcNode>;
// pub type ArcNode = Arc<dyn Node>;

pub trait AsyncReadWriteUnpin: AsyncRead + AsyncWrite + Unpin {}
impl<T> AsyncReadWriteUnpin for T where T: AsyncRead + AsyncWrite + Unpin {}

// TODO:  Should we go through the overwhelming pain to make alloc-less async traits?
// Can follow tokio's model, maybe a crate like`async-trait-ext` can help, or just do it manually?
#[async_trait::async_trait]
pub trait Node: as_any_cast::AsAnyCast + Send + Sync + 'static {
	// fn poll_open_read(
	// 	&self,
	// 	ctx: &mut Context,
	// ) -> Poll<Option<Pin<Box<dyn AsyncRead + Unpin + 'static>>>>;
	//async fn read<'s>(&'s mut self) -> Option<Pin<Box<dyn AsyncRead + Unpin + 's>>>;
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)>;
	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)>;
	// async fn read_write<'s>(
	// 	&'s mut self,
	// ) -> Option<(
	// 	ReadHalf<&'s mut dyn AsyncReadWriteUnpin>,
	// 	WriteHalf<&'s mut dyn AsyncReadWriteUnpin>,
	// )>;
	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)>;
	// async fn open_write(&self) -> Option<Pin<Box<dyn AsyncWrite + Unpin>>>;
}

impl dyn Node {
	pub fn downcast_ref<T: Node>(&self) -> Option<&T> {
		self.as_any().downcast_ref()
	}

	pub fn downcast_mut<T: Node>(&mut self) -> Option<&mut T> {
		self.as_any_mut().downcast_mut()
	}

	// pub fn open_read(&self) -> Node_OpenRead {
	// 	Node_OpenRead(self)
	// }
}

// // Non-camel-case-type to act more as a marker of don't use this directly.
// #[allow(non_camel_case_types)]
// pub struct Node_OpenRead<'a>(&'a dyn Node);
//
// impl<'a> Future for Node_OpenRead<'a> {
// 	type Output = Option<Pin<Box<dyn AsyncRead + Unpin + 'static>>>;
//
// 	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
// 		self.0.poll_open_read(cx)
// 	}
// }

#[cfg(test)]
pub(crate) mod tests {
	use crate::tests::*;

	#[test]
	fn node_access() {
		let mut vfs = Vfs::with_capacity(10);
		vfs.add_default_schemes().unwrap();
	}
}
