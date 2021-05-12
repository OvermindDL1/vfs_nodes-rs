use crate::{Node, NodeError, Scheme, SchemeError};
// use std::collections::HashMap;
use crate::node::CowArcNode;
use std::borrow::Cow;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use url::Url;

#[derive(Debug)]
pub enum FileSystemError {
	Base64Failure(base64::DecodeError),
}

impl std::fmt::Display for FileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			FileSystemError::Base64Failure(_source) => f.write_str("base64 error"),
		}
	}
}

impl std::error::Error for FileSystemError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			FileSystemError::Base64Failure(source) => Some(source),
		}
	}
}

// TODO:  Maybe put all path lookups in a hashmap or btree or so with values as the weak node
// TODO:  then lock it on reading/writing?
pub struct FileSystemScheme {
	root_path: PathBuf,
}

impl FileSystemScheme {
	pub fn new(root_path: impl Into<PathBuf>) -> Self {
		Self {
			root_path: root_path.into(),
		}
	}
}

#[async_trait::async_trait]
impl Scheme for FileSystemScheme {
	async fn create_node<'a>(&self, url: &'a Url) -> Result<Box<dyn Node>, SchemeError<'a>> {
		todo!()
	}

	async fn get_node<'a>(&self, url: &'a Url) -> Result<Box<dyn Node>, SchemeError<'a>> {
		let mut path = self.root_path.clone();
		url.path_segments()
			.ok_or(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))?
			.for_each(|part| path.push(part));
		if !path.exists()
			|| !path
				.parent()
				.ok_or(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))?
				.exists()
		{
			return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
		}

		let file = tokio::fs::File::open(path).await?;

		let mut node = FileSystemNode { file };
		Ok(Box::new(node))
	}
}

pub struct FileSystemNode {
	file: tokio::fs::File,
}

#[async_trait::async_trait]
impl Node for FileSystemNode {
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
		Some(&mut self.file)
	}
	// async fn read<'s>(&'s mut self) -> Option<Pin<Box<dyn AsyncRead + Unpin + 's>>> {
	// 	Some(Box::pin(&mut self.file))
	// }
}

#[cfg(test)]
mod tests {
	use crate::{DataLoaderScheme, FileSystemScheme, Scheme, Vfs};
	use tokio::io::AsyncReadExt;
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn scheme_access() {
		let scheme: &mut dyn Scheme = &mut FileSystemScheme::new(std::env::current_dir().unwrap());
		assert!(
			scheme.get_node(&u("fs:/Cargo.toml")).await.is_ok(),
			"file exists"
		);
		assert!(
			scheme.get_node(&u("fs:/target")).await.is_ok(),
			"folder exists"
		);
	}

	#[tokio::test]
	async fn node_reading() {
		{
			let scheme: &mut dyn Scheme =
				&mut FileSystemScheme::new(std::env::current_dir().unwrap());
			let mut node = scheme.get_node(&u("fs:/Cargo.toml")).await.unwrap();
			let mut reader = node.read().await.unwrap();
			let mut buffer = String::new();
			reader.read_to_string(&mut buffer).await.unwrap();
			assert!(buffer.starts_with("[package]"));
		}
		{
			let mut vfs = Vfs::default();
			vfs.add_scheme(
				"fs".to_owned(),
				FileSystemScheme::new(std::env::current_dir().unwrap()),
			)
			.unwrap();
			let mut node = vfs.get_node_at("fs:/Cargo.toml").await.unwrap();
			let mut buffer = String::new();
			node.read()
				.await
				.unwrap()
				.read_to_string(&mut buffer)
				.await
				.unwrap();
			assert!(buffer.starts_with("[package]"));
		}
	}
}
