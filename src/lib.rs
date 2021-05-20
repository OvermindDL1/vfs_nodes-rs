mod as_any_cast;
pub mod errors;
pub mod node;
pub mod scheme;
pub mod schemes;

pub use crate::node::Node;
pub use crate::scheme::Scheme;
pub use crate::schemes::prelude::*;
pub use errors::*;

use crate::scheme::NodeGetOptions;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use url::Url;

pub struct Vfs {
	schemes: HashMap<String, Box<dyn Scheme>>,
}

impl Default for Vfs {
	fn default() -> Self {
		let mut vfs = Self::empty_with_capacity(10);
		vfs.add_default_schemes()
			.expect("failed adding default schemes to an empty VFS");
		vfs
	}
}

impl Vfs {
	pub fn empty() -> Self {
		Self::empty_with_capacity(0)
	}

	pub fn empty_with_capacity(capacity: usize) -> Self {
		Self {
			schemes: HashMap::with_capacity(capacity),
		}
	}

	pub fn add_default_schemes(&mut self) -> Result<(), VfsError<'static>> {
		// self.schemes.insert("data".to_owned(), DataNode::default());
		self.add_scheme("data".to_owned(), DataLoaderScheme::default())?;
		Ok(())
	}

	pub fn add_scheme(
		&mut self,
		scheme_name: impl Into<String>,
		scheme: impl Scheme,
	) -> Result<&mut Self, VfsError<'static>> {
		self.add_boxed_scheme(scheme_name, Box::new(scheme))
	}

	pub fn add_boxed_scheme(
		&mut self,
		scheme_name: impl Into<String>,
		scheme: Box<dyn Scheme>,
	) -> Result<&mut Self, VfsError<'static>> {
		let scheme_name = scheme_name.into();
		match self.schemes.entry(scheme_name.clone()) {
			Entry::Occupied(_entry) => Err(VfsError::SchemeAlreadyExists(scheme_name)),
			Entry::Vacant(entry) => {
				entry.insert(scheme.into());
				Ok(self)
			}
		}
	}

	pub fn get_scheme<'a>(&self, scheme_name: &'a str) -> Result<&dyn Scheme, VfsError<'a>> {
		self.schemes
			.get(scheme_name)
			.map(|s| &**s)
			.ok_or(VfsError::SchemeNotFound(Cow::Borrowed(scheme_name)))
	}

	pub fn get_scheme_mut<'a>(
		&mut self,
		scheme_name: &'a str,
	) -> Result<&mut dyn Scheme, VfsError<'a>> {
		self.schemes
			.get_mut(scheme_name)
			.map(|n| &mut **n)
			.ok_or(VfsError::SchemeNotFound(Cow::Borrowed(scheme_name)))
	}

	pub fn get_scheme_as<'a, T: Scheme>(&self, scheme_name: &'a str) -> Result<&T, VfsError<'a>> {
		self.get_scheme(scheme_name)?.downcast_ref().ok_or_else(|| {
			VfsError::SchemeWrongType(Cow::Borrowed(scheme_name), std::any::type_name::<T>())
		})
	}

	pub fn get_scheme_mut_as<'a, T: Scheme>(
		&mut self,
		scheme_name: &'a str,
	) -> Result<&mut T, VfsError<'a>> {
		self.get_scheme_mut(scheme_name)?
			.downcast_mut()
			.ok_or_else(|| {
				VfsError::SchemeWrongType(Cow::Borrowed(scheme_name), std::any::type_name::<T>())
			})
	}

	pub async fn get_node<'a>(
		&self,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<Box<dyn Node>, VfsError<'a>> {
		let scheme = self.get_scheme(url.scheme())?;
		Ok(scheme.get_node(self, url, options).await?)
	}

	pub async fn get_node_at(
		&self,
		uri: &str,
		options: &NodeGetOptions,
	) -> Result<Box<dyn Node>, VfsError<'static>> {
		self.get_node(&Url::parse(uri)?, options)
			.await
			.map_err(VfsError::into_owned)
	}

	pub async fn remove_node<'a>(&self, url: &'a Url, force: bool) -> Result<(), VfsError<'a>> {
		let scheme = self.get_scheme(url.scheme())?;
		Ok(scheme.remove_node(self, url, force).await?)
	}

	pub async fn remove_node_at(&self, uri: &str, force: bool) -> Result<(), VfsError<'static>> {
		self.remove_node(&Url::parse(uri)?, force)
			.await
			.map_err(VfsError::into_owned)
	}
}

#[cfg(test)]
pub(crate) mod tests {
	pub use crate::*;

	#[test]
	fn schema_access() {
		let mut vfs = Vfs::empty_with_capacity(10);
		assert!(vfs.get_scheme("data").is_err());
		vfs.add_scheme("data".to_owned(), DataLoaderScheme::default())
			.unwrap();
		vfs.get_scheme("data").unwrap();
		vfs.get_scheme("data").unwrap();
		vfs.get_scheme_mut("data").unwrap();
		let _: &DataLoaderScheme = vfs.get_scheme_as::<DataLoaderScheme>("data").unwrap();
		let _: &mut DataLoaderScheme = vfs.get_scheme_mut_as::<DataLoaderScheme>("data").unwrap();
	}
}

#[cfg(test)]
#[cfg(feature = "backend_tokio")]
mod tests_async_tokio {
	use crate::scheme::NodeGetOptions;
	use crate::Vfs;

	#[tokio::test]
	async fn node_access() {
		let mut vfs = Vfs::empty_with_capacity(10);
		vfs.add_default_schemes().unwrap();
		vfs.get_node_at("data:blah", &NodeGetOptions::new().read(true))
			.await
			.unwrap();
	}

	#[tokio::test]
	async fn node_does_not_exist() {
		let vfs = Vfs::default();
		assert!(vfs.get_scheme("nadda").is_err());
		assert!(vfs
			.get_node_at("nadda:/nadda", &NodeGetOptions::new())
			.await
			.is_err());
		assert!(vfs.remove_node_at("nadda:/nadda", true).await.is_err());
	}
}
