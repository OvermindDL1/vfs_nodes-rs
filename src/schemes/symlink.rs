#![allow(clippy::try_err)]

use crate::scheme::{NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{PinnedNode, Scheme, SchemeError, Vfs};
use std::borrow::Cow;
use std::collections::HashMap;
use url::Url;

const MAX_SYMLINK_PATH_SEGMENTS: usize = 16;
// Keep this end value in sync with the above since rust doesn't support const string concat yet without hacks
const MAX_SYMLINK_PATH_SEGMENTS_ERR: &str = "max symlink path segment depth exceeded, limit is 16";

#[derive(Default)]
pub struct SymLinkTreeNode {
	base_url: Option<Url>,
	children: HashMap<String, SymLinkTreeNode>,
}

impl SymLinkTreeNode {
	pub fn get_base_url(&self) -> Option<&Url> {
		self.base_url.as_ref()
	}
}

#[derive(Default)]
pub struct SymLinkScheme {
	base: SymLinkTreeNode,
}

impl SymLinkScheme {
	pub fn builder() -> SymLinkSchemeBuilder {
		SymLinkSchemeBuilder {
			scheme: Self::default(),
		}
	}

	fn validate_from_url_path(from: &str) -> Result<Url, SchemeError<'static>> {
		let from = Url::parse(&format!("x:{}", from))?;
		if from.path().ends_with('/') {
			Err("`from` path has trailing `/`")?
		} else if from.has_host() {
			Err("`from` path has host, must only be a path")?
		} else if from.fragment().is_some() {
			Err("`from` path has fragment, must only be a path")?
		} else if from.query().is_some() {
			Err("`from` path has query, must only be a path")?
		} else {
			Ok(from)
		}
	}

	pub fn link(&mut self, from: &str, to: Url) -> Result<(), SchemeError<'static>> {
		let from = Self::validate_from_url_path(from)?;
		if let Some(path_segments) = from.path_segments() {
			let mut depth = 0;
			let mut node = &mut self.base;
			for segment in path_segments {
				depth += 1;
				if depth >= MAX_SYMLINK_PATH_SEGMENTS {
					Err(MAX_SYMLINK_PATH_SEGMENTS_ERR)?;
				}
				node = node
					.children
					.entry(segment.to_owned())
					.or_insert_with(Default::default);
			}
			if node.base_url.is_some() {
				Err("url already set at link, remove it first")?;
			} else {
				node.base_url = Some(to);
			}
		} else if from.path().is_empty() {
			// Set the root node
			if self.base.base_url.is_some() {
				Err("url already set at link, remove it first")?;
			} else {
				self.base.base_url = Some(to)
			}
		} else {
			Err("relative symlink is not allowed")?;
		}
		Ok(())
	}

	fn merge_urls(base_url: &Url, url: &Url, url_path: &str) -> Result<Url, SchemeError<'static>> {
		let path = format!("{}{}", base_url.path(), url_path);
		let mut new_url = base_url.clone();
		new_url.set_path(&path);
		if let Some(host) = url.host_str() {
			if base_url.has_host() {
				Err("unable to override host in url in symlink")?;
			}
			new_url.set_host(Some(host))?;
		}
		let username = url.username();
		if !username.is_empty() {
			if !base_url.username().is_empty() {
				Err("unable to override username in url in symlink")?;
			}
			new_url
				.set_username(username)
				.map_err(|()| "failed to rewrite username in symlink url")?;
		}
		if let Some(password) = url.password() {
			if base_url.password().is_some() {
				Err("unable to override password in url in symlink")?;
			}
			new_url
				.set_password(Some(password))
				.map_err(|()| "failed to rewrite password in symlink url")?;
		}
		if let Some(port) = url.port() {
			if base_url.port().is_some() {
				Err("unable to override port in url in symlink")?;
			}
			new_url
				.set_port(Some(port))
				.map_err(|()| "failed to rewrite port in symlink url")?;
		}
		if let Some(fragment) = url.fragment() {
			if base_url.fragment().is_some() {
				Err("unable to override fragment in url in symlink")?;
			}
			new_url.set_fragment(Some(fragment));
		}
		if let Some(query) = url.query() {
			// TODO:  Should queries be mergeable, should adding already defined ones be allowed?
			if let Some(base_query) = base_url.query() {
				if base_query.is_empty() {
					new_url.set_query(Some(query));
				} else {
					new_url.set_query(Some(&format!("{}&{}", base_query, query)));
				}
			} else {
				new_url.set_query(Some(query));
			}
		}
		Ok(new_url)
	}

	pub fn get_symlink_dest<'a>(&self, url: &'a Url) -> Result<Url, SchemeError<'a>> {
		if let Some(path_segments) = url.path_segments() {
			let mut cur_node = &self.base;
			let mut cur_path = [""; MAX_SYMLINK_PATH_SEGMENTS];
			let mut valid_node = if cur_node.base_url.is_some() {
				Some(cur_node)
			} else {
				None
			};
			let mut valid_node_path = [""; MAX_SYMLINK_PATH_SEGMENTS];
			let mut valid_path_len = 0;
			for (idx, segment) in path_segments.enumerate().take(MAX_SYMLINK_PATH_SEGMENTS) {
				if let Some(node) = cur_node.children.get(segment) {
					cur_node = node;
					cur_path[idx] = segment;
					if node.base_url.is_some() {
						valid_node = Some(node);
						valid_node_path[valid_path_len..idx + 1]
							.copy_from_slice(&cur_path[valid_path_len..idx + 1]);
						valid_path_len = idx + 1;
					}
				} else {
					// no more
					break;
				}
			}
			if let Some(Some(base_url)) = valid_node.map(|n| &n.base_url) {
				let url_path = valid_node_path
					.iter()
					.take(valid_path_len)
					.fold(url.path(), |path, segment| {
						// The +1 for the postfix `/` for this segment
						&path[segment.len() + 1..]
					})
					.trim_start_matches('/');
				Self::merge_urls(base_url, url, url_path)
			} else {
				return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.as_str())));
			}
		} else {
			// Data paths are only supported on base
			if let Some(base_url) = &self.base.base_url {
				Self::merge_urls(base_url, url, url.path())
			} else {
				Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.as_str())))
			}
		}
	}
}

pub struct SymLinkSchemeBuilder {
	scheme: SymLinkScheme,
}

impl SymLinkSchemeBuilder {
	pub fn build(self) -> SymLinkScheme {
		self.scheme
	}

	pub fn link(mut self, from: &str, to: Url) -> Self {
		self.scheme
			.link(from, to)
			.expect("SymLinkSchemeBuilder links must have unique `from` paths");
		self
	}
}

#[async_trait::async_trait]
impl Scheme for SymLinkScheme {
	async fn get_node<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<PinnedNode, SchemeError<'a>> {
		let url = self.get_symlink_dest(url)?;
		let fut = vfs.get_node(&url, options);
		// Split the `await` from the `fut` so `url` can drop or else lifetime annoyance
		Ok(fut.await?)
	}

	async fn remove_node<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
		force: bool,
	) -> Result<(), SchemeError<'a>> {
		let url = self.get_symlink_dest(url)?;
		let fut = vfs.remove_node(&url, force);
		// Split the `await` from the `fut` so `url` can drop or else lifetime annoyance
		Ok(fut.await?)
	}

	async fn metadata<'a>(&self, vfs: &Vfs, url: &'a Url) -> Result<NodeMetadata, SchemeError<'a>> {
		let url = self.get_symlink_dest(url)?;
		let fut = vfs.metadata(&url);
		// Split the `await` from the `fut` so `url` can drop or else lifetime annoyance
		Ok(fut.await?)
	}

	async fn read_dir<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
	) -> Result<ReadDirStream, SchemeError<'a>> {
		let url = self.get_symlink_dest(url)?;
		let fut = vfs.read_dir(&url);
		// Split the `await` from the `fut` so `url` can drop or else lifetime annoyance
		Ok(fut.await?)
	}
}

#[cfg(test)]
mod tests {
	use crate::SymLinkScheme;
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[test]
	fn valid_link_paths() {
		let url = u("does:/not/exist");
		SymLinkScheme::default()
			.link("", url.clone())
			.expect("empty data path is allowed");
		SymLinkScheme::default()
			.link("rel", url.clone())
			.expect_err("relative path is not allowed");
		SymLinkScheme::default()
			.link("rel/path/here", url.clone())
			.expect_err("deep relative path is not allowed");
		SymLinkScheme::default()
			.link("/", url.clone())
			.expect_err("trailing slash (even as root) is not allowed");
		SymLinkScheme::default()
			.link("/child/", url.clone())
			.expect_err("trailing slash is not allowed");
		SymLinkScheme::default()
			.link("/deep/child/path/", url.clone())
			.expect_err("deep trailing slash is not allowed");
		SymLinkScheme::default()
			.link("/child", url.clone())
			.expect("child path must be accepted");
		SymLinkScheme::default()
			.link("/deep/child/path", url.clone())
			.expect("deep child path must be accepted");
		let _ = url;
	}
}

#[cfg(test)]
#[cfg(feature = "backend_tokio")]
mod async_tokio_tests {
	use crate::scheme::NodeGetOptions;
	use crate::{SymLinkScheme, TokioFileSystemScheme, Vfs};
	use futures_lite::AsyncReadExt;
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	async fn get_read_node(vfs: &Vfs, uri: &str) -> String {
		let mut test_node = vfs
			.get_node_at(uri, &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		test_node.read_to_string(&mut buffer).await.unwrap();
		buffer
	}

	#[tokio::test]
	async fn node_get() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"sl",
			SymLinkScheme::builder()
				.link("", u("data:"))
				.link("/data", u("data:"))
				.link("/fs", u("fs:/"))
				.link("/fst", u("fs:/target/"))
				.link("/fsc.toml", u("fs:/Cargo.toml"))
				.build(),
		)
		.unwrap();
		vfs.add_scheme(
			"fs",
			TokioFileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();

		assert_eq!(&get_read_node(&vfs, "sl:test%20stuff").await, "test stuff");
		assert_eq!(
			&get_read_node(&vfs, "sl:/data/test%20stuff").await,
			"test stuff"
		);
		assert_eq!(
			get_read_node(&vfs, "sl:/fsc.toml")
				.await
				.lines()
				.next()
				.unwrap(),
			"[package]"
		);
	}
}
