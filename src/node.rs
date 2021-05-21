use crate::as_any_cast;
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite};
use std::task::Poll;

// TODO:  Should we go through the pain to make alloc-less async traits?
// Can follow tokio's model, maybe a crate like`async-trait-ext` can help, or just do it manually?
#[async_trait::async_trait]
pub trait Node:
	AsyncRead + AsyncWrite + AsyncSeek + as_any_cast::AsAnyCast + Send + 'static
{
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
	// async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)>;
	// async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)>;
	// async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)>;
	fn is_reader(&self) -> bool;
	fn is_writer(&self) -> bool;
	fn is_seeker(&self) -> bool;
}

impl dyn Node {
	pub fn downcast_ref<T: Node>(&self) -> Option<&T> {
		self.as_any().downcast_ref()
	}

	pub fn downcast_mut<T: Node>(&mut self) -> Option<&mut T> {
		self.as_any_mut().downcast_mut()
	}
}

pub fn poll_io_err<T>() -> Poll<std::io::Result<T>> {
	Poll::Ready(Err(std::io::Error::from_raw_os_error(13)))
}

pub trait IsAllowed: Sized {
	fn allowed(self) -> bool;

	fn else_poll_io_error(self) -> Result<(), Poll<std::io::Result<()>>> {
		if self.allowed() {
			Ok(())
		} else {
			Err(poll_io_err())
		}
	}

	fn into_poll_io<T>(self, ret: T) -> Poll<std::io::Result<T>> {
		if self.allowed() {
			Poll::Ready(Ok(ret))
		} else {
			poll_io_err()
		}
	}

	fn into_poll_io_of<T, F: FnOnce() -> T>(self, ret: F) -> Poll<std::io::Result<T>> {
		if self.allowed() {
			Poll::Ready(Ok(ret()))
		} else {
			poll_io_err()
		}
	}

	fn into_poll_io_then<T, F: FnOnce() -> Poll<std::io::Result<T>>>(
		self,
		ret: F,
	) -> Poll<std::io::Result<T>> {
		if self.allowed() {
			ret()
		} else {
			poll_io_err()
		}
	}
}

impl IsAllowed for bool {
	fn allowed(self) -> bool {
		self
	}
}
