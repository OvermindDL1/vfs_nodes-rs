use crate::scheme::NodeGetOptions;
use crate::{Node, Scheme, SchemeError, Vfs};
use futures_lite::{ready, AsyncRead, AsyncSeek, AsyncWrite};
use std::borrow::Cow;
use std::io::{IoSlice, SeekFrom};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::OpenOptions;
use url::Url;

#[derive(Debug)]
pub enum TokioFileSystemError {
	// Base64Failure(base64::DecodeError),
}

impl std::fmt::Display for TokioFileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// match self {
		// 	TokioFileSystemError::Base64Failure(_source) => f.write_str("base64 error"),
		// }
		f.write_str("IMPOSSIBLE-ERROR")
	}
}

impl std::error::Error for TokioFileSystemError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		// match self {
		// 	TokioFileSystemError::Base64Failure(source) => Some(source),
		// }
		None
	}
}

// TODO:  Maybe put all path lookups in a hashmap or btree or so with values as the weak node
// TODO:  then lock it on reading/writing?
pub struct TokioFileSystemScheme {
	root_path: PathBuf,
}

impl TokioFileSystemScheme {
	pub fn new(root_path: impl Into<PathBuf>) -> Self {
		Self {
			root_path: root_path.into(),
		}
	}

	pub fn fs_path_from_url<'a>(&self, url: &'a Url) -> Result<PathBuf, SchemeError<'a>> {
		Ok(url
			.path_segments()
			.ok_or(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))?
			.fold(self.root_path.clone(), |mut path, part| {
				path.push(part);
				path
			}))
	}
}

#[async_trait::async_trait]
impl Scheme for TokioFileSystemScheme {
	async fn get_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<Box<dyn Node>, SchemeError<'a>> {
		let path = self.fs_path_from_url(url)?;
		if options.get_create() {
			let parent_path = path
				.parent()
				.ok_or(SchemeError::UrlAccessError(Cow::Borrowed(&url)))?;
			tokio::fs::create_dir_all(parent_path).await?;
		}
		let file = OpenOptions::from(options).open(path).await?;
		let node = TokioFileSystemNode { file, seek: None };
		Ok(Box::new(node))
	}

	async fn remove_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		force: bool,
	) -> Result<(), SchemeError<'a>> {
		let path = self.fs_path_from_url(url)?;
		if path.exists() {
			if path.is_file() {
				tokio::fs::remove_file(&path).await?;
			} else if path.is_dir() {
				if force {
					tokio::fs::remove_dir_all(&path).await?;
				} else {
					tokio::fs::remove_dir(&path).await?;
				}
			}
		}
		Ok(())
	}
}

pub struct TokioFileSystemNode {
	file: tokio::fs::File,
	seek: Option<std::io::SeekFrom>,
}

#[async_trait::async_trait]
impl Node for TokioFileSystemNode {
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
		Some(self)
	}

	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
		Some(self)
	}

	// async fn read_write<'s>(
	// 	&'s mut self,
	// ) -> Option<(
	// 	ReadHalf<&'s mut dyn AsyncReadWriteUnpin>,
	// 	WriteHalf<&'s mut dyn AsyncReadWriteUnpin>,
	// )> {
	// 	Some(tokio::io::split(&mut self.file))
	// }

	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
		Some(self)
	}
}

impl AsyncRead for TokioFileSystemNode {
	fn poll_read(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		let mut buf = tokio::io::ReadBuf::new(buf);
		{
			let file = Pin::new(&mut self.file);
			ready!(tokio::io::AsyncRead::poll_read(file, cx, &mut buf))?;
		}
		Poll::Ready(Ok(buf.filled().len()))
	}
}

// Tokio's file does not implement AsyncBufRead
// impl AsyncBufRead for TokioFileSystemNode {
// 	fn poll_fill_buf(
// 		mut self: Pin<&mut Self>,
// 		cx: &mut Context<'_>,
// 	) -> Poll<std::io::Result<&[u8]>> {
//		let file = Pin::new(&mut self.file);
// 		tokio::io::AsyncBufRead::poll_fill_buf(file, cx)
// 	}
//
// 	fn consume(mut self: Pin<&mut Self>, amt: usize) {
//		let file = Pin::new(&mut self.file);
// 		tokio::io::AsyncBufRead::consume(amt)
// 	}
// }

impl AsyncWrite for TokioFileSystemNode {
	fn poll_write(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		let file = Pin::new(&mut self.file);
		tokio::io::AsyncWrite::poll_write(file, cx, buf)
	}

	fn poll_write_vectored(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>],
	) -> Poll<std::io::Result<usize>> {
		let file = Pin::new(&mut self.file);
		tokio::io::AsyncWrite::poll_write_vectored(file, cx, bufs)
	}

	fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		let file = Pin::new(&mut self.file);
		tokio::io::AsyncWrite::poll_flush(file, cx)
	}

	fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		let file = Pin::new(&mut self.file);
		tokio::io::AsyncWrite::poll_shutdown(file, cx)
	}
}

impl AsyncSeek for TokioFileSystemNode {
	fn poll_seek(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		pos: SeekFrom,
	) -> Poll<std::io::Result<u64>> {
		if self.seek != Some(pos) {
			{
				let file = Pin::new(&mut self.file);
				tokio::io::AsyncSeek::start_seek(file, pos)?;
			}
			self.as_mut().seek = Some(pos);
		}
		let res = {
			let file = Pin::new(&mut self.file);
			ready!(tokio::io::AsyncSeek::poll_complete(file, cx))
		};
		self.as_mut().seek = None;
		Poll::Ready(res.map(|p| p as u64))
	}
}

#[cfg(test)]
mod tests_general {
	// Unique per test
	use crate::TokioFileSystemScheme as FileSystemScheme;
	use tokio::test as async_test;

	const FILE_CONTENT_TEST_LOC: &str = "fs:/test_node_writing_tokio.txt";
	const FILE_CONTENT_SEEK_TEST_LOC: &str = "fs:/test_node_seeking_tokio.txt";

	// Generic per test
	use crate::scheme::NodeGetOptions;
	use crate::Vfs;
	use futures_lite::io::SeekFrom;
	use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
	use url::Url;

	const FILE_TEST_CONTENT: &str = "Test content";

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[async_test]
	async fn scheme_access() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		assert!(
			vfs.get_node(&u("fs:/Cargo.toml"), &NodeGetOptions::new().read(true))
				.await
				.is_ok(),
			"file exists"
		);
		assert!(
			vfs.get_node(&u("fs:/target"), &NodeGetOptions::new().read(true))
				.await
				.is_ok(),
			"folder exists"
		);
	}

	#[async_test]
	async fn node_reading_vfs() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		let mut node = vfs
			.get_node_at("fs:/Cargo.toml", &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read()
			.await
			.unwrap()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
		assert!(buffer.starts_with("[package]"));
	}

	#[async_test]
	async fn node_writing() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap().join("target")),
		)
		.unwrap();
		let mut node = vfs
			.get_node(
				&u(FILE_CONTENT_TEST_LOC),
				&NodeGetOptions::new()
					.read(true)
					.write(true)
					.truncate(true)
					.create(true)
					.create_new(false),
			)
			.await
			.unwrap();
		let writer = node.write().await.unwrap();
		writer
			.write_all(FILE_TEST_CONTENT.as_bytes())
			.await
			.unwrap();
		writer.flush().await.unwrap();
		let mut node = vfs
			.get_node(&u(FILE_CONTENT_TEST_LOC), &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let reader = node.read().await.unwrap();
		let mut buffer = String::new();
		reader.read_to_string(&mut buffer).await.unwrap();
		vfs.remove_node(&u(FILE_CONTENT_TEST_LOC), false)
			.await
			.unwrap();
		assert_eq!(&buffer, FILE_TEST_CONTENT);
	}

	#[async_test]
	async fn node_seeking() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap().join("target")),
		)
		.unwrap();
		let mut node = vfs
			.get_node(
				&u(FILE_CONTENT_SEEK_TEST_LOC),
				&NodeGetOptions::new()
					.read(true)
					.write(true)
					.truncate(true)
					.create(true)
					.create_new(false),
			)
			.await
			.unwrap();
		let writer = node.write().await.unwrap();
		writer
			.write_all(FILE_TEST_CONTENT.as_bytes())
			.await
			.unwrap();
		// Always be sure to flush before seeking if any writes were performed, not necessary for
		// async_std, but it is for tokio, and it's good form anyway when seeking.
		writer.flush().await.unwrap();
		node.seek()
			.await
			.unwrap()
			.seek(SeekFrom::Start(0))
			.await
			.unwrap();
		let reader = node.read().await.unwrap();
		let mut buffer = String::new();
		reader.read_to_string(&mut buffer).await.unwrap();
		vfs.remove_node(&u(FILE_CONTENT_SEEK_TEST_LOC), false)
			.await
			.unwrap();
		assert_eq!(&buffer, FILE_TEST_CONTENT);
	}
}
