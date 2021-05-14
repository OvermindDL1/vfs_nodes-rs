use crate::scheme::NodeGetOptions;
use crate::{Node, Scheme, SchemeError};
use std::borrow::Cow;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
// use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadHalf, WriteHalf};
use futures_io::{AsyncRead, AsyncSeek, AsyncWrite};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};
use url::Url;

#[derive(Debug)]
pub enum TokioFileSystemError {
	Base64Failure(base64::DecodeError),
}

impl std::fmt::Display for TokioFileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TokioFileSystemError::Base64Failure(_source) => f.write_str("base64 error"),
		}
	}
}

impl std::error::Error for TokioFileSystemError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			TokioFileSystemError::Base64Failure(source) => Some(source),
		}
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
		url: &'a Url,
		options: NodeGetOptions,
	) -> Result<Box<dyn Node>, SchemeError<'a>> {
		let path = self.fs_path_from_url(url)?;
		if options.get_create() {
			let parent_path = path
				.parent()
				.ok_or(SchemeError::URLAccessError(Cow::Borrowed(&url)))?;
			tokio::fs::create_dir_all(parent_path).await?;
		}
		let file = OpenOptions::from(options).open(path).await?;
		let node = FileSystemNode {
			file: file.compat(),
		};
		Ok(Box::new(node))
	}

	async fn remove_node<'a>(&self, url: &'a Url, force: bool) -> Result<(), SchemeError<'a>> {
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

pub struct FileSystemNode {
	file: Compat<tokio::fs::File>,
}

#[async_trait::async_trait]
impl Node for FileSystemNode {
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
		Some(&mut self.file)
	}

	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
		Some(&mut self.file)
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
		// Some(&mut self.file)
		None
	}
}

#[cfg(test)]
mod tests {
	use crate::scheme::NodeGetOptions;
	use crate::{Scheme, TokioFileSystemScheme, Vfs};
	use tokio::io::{AsyncReadExt, AsyncWriteExt};
	use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn scheme_access() {
		let scheme: &mut dyn Scheme =
			&mut TokioFileSystemScheme::new(std::env::current_dir().unwrap());
		assert!(
			scheme
				.get_node(&u("fs:/Cargo.toml"), NodeGetOptions::new().read(true))
				.await
				.is_ok(),
			"file exists"
		);
		assert!(
			scheme
				.get_node(&u("fs:/target"), NodeGetOptions::new().read(true))
				.await
				.is_ok(),
			"folder exists"
		);
	}

	#[tokio::test]
	async fn node_reading_scheme() {
		let scheme: &mut dyn Scheme =
			&mut TokioFileSystemScheme::new(std::env::current_dir().unwrap());
		let mut node = scheme
			.get_node(&u("fs:/Cargo.toml"), NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let reader = node.read().await.unwrap();
		let mut buffer = String::new();
		reader.compat().read_to_string(&mut buffer).await.unwrap();
		assert!(buffer.starts_with("[package]"));
	}

	#[tokio::test]
	async fn node_reading_vfs() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs".to_owned(),
			TokioFileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		let mut node = vfs
			.get_node_at("fs:/Cargo.toml", NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read()
			.await
			.unwrap()
			.compat()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
		assert!(buffer.starts_with("[package]"));
	}

	#[tokio::test]
	async fn node_writing() {
		let scheme: &mut dyn Scheme =
			&mut TokioFileSystemScheme::new(std::env::current_dir().unwrap().join("target"));
		let mut node = scheme
			.get_node(
				&u("fs:/test_node_writing.txt"),
				NodeGetOptions::new()
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
			.compat_write()
			.write_all("test content".as_bytes())
			.await
			.unwrap();
		// Close and re-open file because tokio-util::compat doesn't have a Compat with AsyncSeek...
		// node.seek()
		// 	.await
		// 	.unwrap()
		// 	.compat()
		// 	.seek(SeekFrom::Start(0))
		// 	.await
		// 	.unwrap();
		let mut node = scheme
			.get_node(
				&u("fs:/test_node_writing.txt"),
				NodeGetOptions::new().read(true),
			)
			.await
			.unwrap();
		let reader = node.read().await.unwrap();
		let mut buffer = String::new();
		reader.compat().read_to_string(&mut buffer).await.unwrap();
		scheme
			.remove_node(&u("fs:/test_node_writing.txt"), false)
			.await
			.unwrap();
		assert_eq!(&buffer, "test content");
	}
}
