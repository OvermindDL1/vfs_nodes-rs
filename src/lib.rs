mod as_any_cast;
pub mod errors;
pub mod node;
pub mod nodes;
pub mod scheme;
pub mod schemes;

pub use crate::node::Node;
pub use crate::nodes::*;
pub use crate::scheme::Scheme;
pub use crate::schemes::*;
pub use errors::*;

use crate::node::CowArcNode;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use url::Url;

pub struct Vfs {
	schemes: HashMap<String, Box<dyn Scheme>>,
}

impl Default for Vfs {
	fn default() -> Self {
		let mut vfs = Self::with_capacity(10);
		vfs.add_default_schemes()
			.expect("failed adding default schemes to an empty VFS");
		vfs
	}
}

impl Vfs {
	pub fn with_capacity(capacity: usize) -> Self {
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
		scheme_name: String,
		scheme: impl Scheme,
	) -> Result<&mut Self, VfsError<'static>> {
		match self.schemes.entry(scheme_name.clone()) {
			Entry::Occupied(_entry) => Err(VfsError::SchemeAlreadyExists(scheme_name)),
			Entry::Vacant(entry) => {
				entry.insert(Box::new(scheme));
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

	pub async fn get_node<'s, 'a>(&'s self, url: &'a Url) -> Result<Box<dyn Node>, VfsError<'a>> {
		let scheme = self.get_scheme(url.scheme())?;
		Ok(scheme.get_node(&url).await?)
	}

	pub async fn get_node_at<'s>(&'s self, uri: &str) -> Result<Box<dyn Node>, VfsError<'static>> {
		self.get_node(&Url::parse(uri)?)
			.await
			.map_err(VfsError::into_owned)
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use crate::node::CowArcNode;
	pub use crate::*;

	#[test]
	fn schema_access() {
		let mut vfs = Vfs::with_capacity(10);
		assert!(vfs.get_scheme("data").is_err());
		vfs.add_scheme("data".to_owned(), DataLoaderScheme::default())
			.unwrap();
		vfs.get_scheme("data").unwrap();
		vfs.get_scheme("data").unwrap();
		vfs.get_scheme_mut("data").unwrap();
		let _: &DataLoaderScheme = vfs.get_scheme_as::<DataLoaderScheme>("data").unwrap();
		let _: &mut DataLoaderScheme = vfs.get_scheme_mut_as::<DataLoaderScheme>("data").unwrap();
	}

	#[tokio::test]
	async fn node_access() {
		let mut vfs = Vfs::with_capacity(10);
		vfs.add_default_schemes();
		vfs.get_node_at("data:blah").await.unwrap();
	}
}

// // #[cfg(feature = "bevy")]
// // pub mod bevy;
// pub mod nodes;
// mod style_test;
//
// use crate::node_implementation_helpers::{
// 	next_useful_component, through_next_useful_component, ThroughResult, UsefulComponent,
// };
// use crate::nodes::directory::DirectoryNode;
// use crate::nodes::noop::NoopNode;
// use std::borrow::Cow;
// use std::ffi::OsStr;
// use std::fmt::Debug;
// use std::path::{Components, Path};
// use std::sync::{Arc, Weak};
//
// #[derive(Debug)]
// pub struct System {
// 	root: ArcNode,
// }
//
// impl System {
// 	pub fn new() -> Self {
// 		let root = DirectoryNode::new(Weak::<NoopNode>::new());
// 		Self { root }
// 	}
//
// 	pub fn new_with_root<T, CFn>(construct: CFn) -> Self
// 	where
// 		T: Node,
// 		CFn: FnOnce(WeakNode) -> T,
// 	{
// 		let root = Arc::new(construct(Weak::<NoopNode>::new()));
// 		Self { root }
// 	}
//
// 	pub fn root(&self) -> &ArcNode {
// 		&self.root
// 	}
//
// 	pub fn get_node_at_path(&self, path: impl AsRef<Path>) -> Option<CowArcNode> {
// 		let mut components = path.as_ref().components();
// 		match next_useful_component(&mut components) {
// 			None => Some(CowArcNode::Borrowed(&self.root)),
// 			Some(UsefulComponent::ParentDir) => None,
// 			Some(UsefulComponent::Normal(name)) => {
// 				self.root.get_child_node_at(name, &mut components)
// 			}
// 		}
// 	}
//
// 	pub fn set_node_at_path(
// 		&self,
// 		path: impl AsRef<Path>,
// 		constructor: impl FnMut(WeakNode) -> ArcNode,
// 	) -> Result<(), &str> {
// 		self.root.set_child(path, constructor)
// 	}
//
// 	pub fn remove_node_at_path(&self, path: impl AsRef<Path>) -> Result<(), &str> {
// 		self.root.remove_child(path)
// 	}
// }
//
// pub mod node_implementation_helpers {
// 	use crate::{ArcNode, CowArcNode};
// 	use std::ffi::OsStr;
// 	use std::path::{Component, Components};
//
// 	pub enum ThroughResult<'a, 'b> {
// 		PathEnd,
// 		InvalidParent,
// 		Name(&'a OsStr),
// 		JustNode(CowArcNode<'b>),
// 		NameNode(&'a OsStr, CowArcNode<'b>),
// 	}
//
// 	pub enum UsefulComponent<'a> {
// 		ParentDir,
// 		Normal(&'a OsStr),
// 	}
//
// 	impl<'a> UsefulComponent<'a> {
// 		// Returns tuple of maybe next child name and maybe next noe (parent to call it on, or returns an error string
// 		// (None, None) means it ran out of parents, error
// 		// (Some, None) means it is a child node with the given name
// 		// (None, Some) means it is a parent node and ran out of path
// 		// (Some, Some) means it is a parent node and has a child path on it
// 		//
// 		// None
// 		pub fn through_parents_of<'b>(
// 			self,
// 			components: &mut Components<'b>,
// 			this: &ArcNode,
// 		) -> ThroughResult<'b, 'static>
// 		where
// 			'a: 'b,
// 		{
// 			match self {
// 				UsefulComponent::Normal(name) => return ThroughResult::Name(name),
// 				UsefulComponent::ParentDir => {
// 					let mut parent: ArcNode = match this.get_parent_node().upgrade() {
// 						None => return ThroughResult::InvalidParent,
// 						Some(parent) => parent,
// 					};
// 					loop {
// 						if let Some(component) = next_useful_component(components) {
// 							match component {
// 								UsefulComponent::ParentDir => {
// 									if let Some(p) = parent.get_parent_node().upgrade() {
// 										parent = p;
// 									} else {
// 										return ThroughResult::InvalidParent;
// 									}
// 								}
// 								UsefulComponent::Normal(name) => {
// 									return ThroughResult::NameNode(
// 										name,
// 										CowArcNode::Owned(parent),
// 									);
// 								}
// 							}
// 						} else {
// 							return ThroughResult::JustNode(CowArcNode::Owned(parent));
// 						}
// 					}
// 				}
// 			}
// 		}
// 	}
//
// 	pub fn next_useful_component<'a>(
// 		components: &mut Components<'a>,
// 	) -> Option<UsefulComponent<'a>> {
// 		while let Some(component) = components.next() {
// 			match component {
// 				Component::Prefix(_) => {
// 					// Ignore prefix's
// 				}
// 				Component::RootDir => {
// 					// Ignore root's
// 				}
// 				Component::CurDir => {
// 					// ignore current directories
// 				}
// 				Component::ParentDir => return Some(UsefulComponent::ParentDir),
// 				Component::Normal(name) => return Some(UsefulComponent::Normal(name)),
// 			}
// 		}
// 		None
// 	}
//
// 	pub fn through_next_useful_component<'a, 'b>(
// 		components: &mut Components<'a>,
// 		this: &'b ArcNode,
// 	) -> ThroughResult<'a, 'b> {
// 		match next_useful_component(components) {
// 			None => ThroughResult::PathEnd,
// 			Some(component) => component.through_parents_of(components, this),
// 		}
// 	}
//
// 	// pub fn next_useful_call<R, F: FnOnce() -> R>(components: &mut Components, parent: &CowWeakNode, ) -> Option<R> {
// 	//     while let Some(component) = next_useful_component(components) {
// 	//         match component {
// 	//             UsefulComponent::ParentDir => {}
// 	//             UsefulComponent::Normal(_) => {}
// 	//         }
// 	//     }
// 	//     todo!()
// 	// }
//
// 	// pub fn iter_to_next_useful_component<Ret, F: FnOnce(&OsStr) -> Ret>(
// 	//     node: CowArcNode,
// 	//     mut components: &mut Components,
// 	//     f: F,
// 	// ) -> Result<Ret, &'static str> {
// 	//     while let Some(component) = components.next() {
// 	//         match component {
// 	//             Component::Prefix(_) => panic!("prefix's are unsupported"),
// 	//             // Can only happen at start of parsing
// 	//             Component::RootDir => panic!("unexpected root directory parse")
// 	//             Component::CurDir => {}
// 	//             Component::ParentDir => {}
// 	//             Component::Normal(_) => {}
// 	//         }
// 	//     }
// 	//     todo!()
// 	// }
// }
//
// type ArcNode = Arc<dyn Node>;
// type WeakNode = Weak<dyn Node>;
// type CowArcNode<'a> = Cow<'a, Arc<dyn Node>>;
// type CowWeakNode<'a> = Cow<'a, Weak<dyn Node>>;
// pub trait Node: 'static + Send + Sync + as_any_cast::AsAnyCast + Debug {
// 	fn get_child_node_at(&self, name: &OsStr, components: &mut Components) -> Option<CowArcNode>;
// 	fn get_parent_node(&self) -> CowWeakNode;
// 	fn set_child_node_at(
// 		&self,
// 		name: &OsStr,
// 		components: &mut Components,
// 		constructor: &mut dyn FnMut(WeakNode) -> ArcNode,
// 	) -> Result<(), &'static str>;
// 	fn remove_child_node_at(
// 		&self,
// 		name: &OsStr,
// 		components: &mut Components,
// 	) -> Result<(), &'static str>;
// 	//fn get_capability(&self, std::any::TypeId::)
// }
//
// impl dyn Node {
// 	pub fn downcast_ref<T: Node>(&self) -> Result<&T, &'static str> {
// 		self.as_any().downcast_ref().ok_or_else(|| self.type_name())
// 	}
//
// 	pub fn downcast_arc<T: Node>(self: &Arc<Self>) -> Result<Arc<T>, &'static str> {
// 		if self.as_ref().as_any().is::<T>() {
// 			Ok(self.clone().into_arc_any().downcast::<T>().unwrap())
// 		} else {
// 			Err(self.as_ref().type_name())
// 		}
// 	}
//
// 	// TODO:  Maybe change ArcNode to an owned arc so we can make a `downcast_arc_ref`?
//
// 	pub fn downcast_capability<'s, T: ?Sized>(self: &'s Arc<Self>) -> Option<&'s T> {
// 		todo!()
// 	}
//
// 	pub fn get_child<'a>(self: &'a Arc<Self>, path: impl AsRef<Path>) -> Option<CowArcNode<'a>> {
// 		let mut components = path.as_ref().components();
// 		match through_next_useful_component(&mut components, &self) {
// 			ThroughResult::PathEnd => Some(CowArcNode::Owned(self.clone())),
// 			ThroughResult::InvalidParent => None,
// 			ThroughResult::Name(name) => self.get_child_node_at(name, &mut components),
// 			ThroughResult::JustNode(parent) => Some(parent),
// 			ThroughResult::NameNode(name, parent) => parent
// 				.get_child_node_at(name, &mut components)
// 				.map(|n| CowArcNode::Owned(n.into_owned())),
// 		}
// 	}
//
// 	pub fn set_child(
// 		self: &Arc<Self>,
// 		path: impl AsRef<Path>,
// 		mut constructor: impl FnMut(WeakNode) -> ArcNode,
// 	) -> Result<(), &'static str> {
// 		let mut components = path.as_ref().components();
// 		match through_next_useful_component(&mut components, &self) {
// 			ThroughResult::PathEnd => Err("node exists"),
// 			ThroughResult::InvalidParent => Err("ran out of parents"),
// 			ThroughResult::Name(name) => {
// 				self.set_child_node_at(name, &mut components, &mut constructor)
// 			}
// 			ThroughResult::JustNode(_parent) => Err("cannot set child as a parent of a child"),
// 			ThroughResult::NameNode(name, parent) => {
// 				parent.set_child_node_at(name, &mut components, &mut constructor)
// 			}
// 		}
// 	}
//
// 	pub fn remove_child(self: &Arc<Self>, path: impl AsRef<Path>) -> Result<(), &'static str> {
// 		let mut components = path.as_ref().components();
// 		match through_next_useful_component(&mut components, &self) {
// 			ThroughResult::PathEnd => Err("cannot remove self"),
// 			ThroughResult::InvalidParent => Err("ran out of parents"),
// 			ThroughResult::Name(name) => self.remove_child_node_at(name, &mut components),
// 			ThroughResult::JustNode(_parent) => Err("cannot remove a parent of a child"),
// 			ThroughResult::NameNode(name, parent) => {
// 				parent.remove_child_node_at(name, &mut components)
// 			}
// 		}
// 	}
// }
//
// pub mod as_any_cast {
// 	use std::any::Any;
// 	use std::sync::Arc;
//
// 	pub trait AsAnyCast: Any + Send + Sync {
// 		fn type_name(&self) -> &'static str;
// 		fn as_any(&self) -> &dyn Any;
// 		fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
// 	}
//
// 	impl<T: Any + Send + Sync> AsAnyCast for T {
// 		fn type_name(&self) -> &'static str {
// 			std::any::type_name::<T>()
// 		}
//
// 		fn as_any(&self) -> &dyn Any {
// 			self
// 		}
//
// 		fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
// 			self
// 		}
// 	}
// }
//
// //
//
// //
//
// //
//
// // struct INode<Data> {
// //     data: Data,
// //     node: Box<dyn Node>
// // }
// //
// // pub struct NodeSystem<Data> {
// //     data: Vec<Option<INode<Data>>>,
// //     root: Arc<PNode<Data>>,
// // }
// //
// // impl<Data> NodeSystem<Data> {
// //     pub fn new(data: Data) -> Self {
// //         let node = Arc::new(nodes::noop::NoopNode);
// //         NodeSystem {
// //             data: vec![None], // index 0 is always empty
// //             root: Arc::new(PNode::new("/", node, data)),
// //         }
// //     }
// //
// //     pub fn root(&self) -> &Arc<PNode<Data>> {
// //         &self.root
// //     }
// // }
// //
// // pub struct PNode<Data> {
// //     parent: Arc<PNode<Data>,
// //     inode: usize,
// //     node: Arc<dyn Node>,
// //     nodes: RwLock<BTreeMap<String, Arc<PNode<Data>>>>,
// // }
// //
// // impl<Data> PNode<Data> {
// //     fn new(path: impl Into<PathBuf>, node: Arc<dyn Node>, data: Data) -> Self {
// //         Self {
// //             data,
// //             path: path.into(),
// //             node,
// //             nodes: RwLock::new(BTreeMap::default()),
// //         }
// //     }
// //
// //     pub fn data(&self) -> &Data {
// //         &self.data
// //     }
// //
// //     pub fn path(&self) -> &Path {
// //         &self.path
// //     }
// //
// //     pub fn node_ref(&self) -> &dyn Node {
// //         self.node.as_ref()
// //     }
// //
// //     pub fn node_downcast_ref<T: Node>(&self) -> Result<&T, &'static str> {
// //         self.node.downcast_ref::<T>()
// //     }
// //
// //     pub fn node_arc(&self) -> Arc<dyn Node> {
// //         self.node.clone()
// //     }
// //
// //     pub fn node_downcast_arc<T: Node>(&self) -> Result<Arc<T>, &'static str> {
// //         self.node.clone().downcast_arc::<T>()
// //     }
// //
// //     pub fn add_node(
// //         &self,
// //         path_element: &str,
// //         node: impl Node,
// //         data: Data,
// //     ) -> Result<(), &'static str> {
// //         let mut nodes = self.nodes.write().expect("poisoned `nodes` lock");
// //         if nodes.contains_key(path_element) {
// //             return Err("path already exists");
// //         }
// //         if path_element.contains('/') {
// //             return Err("invalid character in path element");
// //         }
// //         let path = self.path.join(path_element);
// //         let node_container = Self::new(path, Arc::new(node), data);
// //         match nodes.insert(path_element.to_owned(), Arc::new(node_container)) {
// //             None => Ok(()),
// //             Some(_) => Err("unable to insert new node due to existing entry"),
// //         }
// //     }
// //
// //     pub fn node_at_path(
// //         &self,
// //         path: impl AsRef<Path>,
// //     ) -> Result<&Arc<PNode<Data>>, &'static str> {
// //         let path = path.as_ref();
// //         let nodes = self.nodes.read().expect("poisoned `nodes` lock");
// //         todo!("Finish `node_at_path` by popping `/` if any at start of path then looking up the rest if any, if not any or its missing in `nodes` then return an error")
// //     }
// // }
// //
// // pub mod as_any_cast {
// //     use std::any::Any;
// //     use std::sync::Arc;
// //
// //     pub trait AsAnyCast: Any + Send + Sync {
// //         fn type_name(&self) -> &'static str;
// //         fn as_any(&self) -> &dyn Any;
// //         fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
// //     }
// //
// //     impl<T: Any + Send + Sync> AsAnyCast for T {
// //         fn type_name(&self) -> &'static str {
// //             std::any::type_name::<T>()
// //         }
// //
// //         fn as_any(&self) -> &dyn Any {
// //             self
// //         }
// //
// //         fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
// //             self
// //         }
// //     }
// // }
// //
// // pub trait Node: 'static + Send + Sync + as_any_cast::AsAnyCast + Debug {}
// //
// // impl dyn Node {
// //     pub fn downcast_ref<T: Node>(&self) -> Result<&T, &'static str> {
// //         self.as_any().downcast_ref().ok_or_else(|| self.type_name())
// //     }
// //
// //     pub fn downcast_arc<T: Node>(self: Arc<Self>) -> Result<Arc<T>, &'static str> {
// //         if self.as_ref().as_any().is::<T>() {
// //             Ok(self.clone().into_arc_any().downcast::<T>().unwrap())
// //         } else {
// //             Err(self.as_ref().type_name())
// //         }
// //     }
// // }
//
// #[cfg(test)]
// mod tests {
// 	use crate::nodes::directory::DirectoryNode;
// 	use crate::nodes::file_system::FileSystemDirectoryNode;
// 	use crate::nodes::noop::NoopNode;
// 	use crate::{CowArcNode, System};
// 	use std::io::{BufReader, Write};
// 	use std::sync::Arc;
//
// 	#[test]
// 	fn root_node() {
// 		let system = System::new();
// 		system
// 			.root()
// 			.set_child("noop", |w| NoopNode::new(w))
// 			.unwrap();
// 		assert_eq!(
// 			system.get_node_at_path("/").unwrap().as_ref().as_ref() as *const _,
// 			system.root().as_ref() as *const _
// 		);
// 		assert_eq!(
// 			system.get_node_at_path("/noop").unwrap().as_ref().as_ref() as *const _,
// 			system.root().get_child("noop").unwrap().as_ref().as_ref() as *const _
// 		);
// 		assert!(system.get_node_at_path("/").is_some());
// 		assert!(system.get_node_at_path("/none").is_none());
// 		let node_root: CowArcNode = system.get_node_at_path("/").unwrap();
// 		// Make sure we can cat the node_root into its type
// 		let _node_root_dir: Arc<DirectoryNode> = node_root.downcast_arc::<DirectoryNode>().unwrap();
// 		let _node_root_dir_ref: &DirectoryNode = node_root.downcast_ref::<DirectoryNode>().unwrap();
// 	}
//
// 	#[test]
// 	fn create_remove_nodes() {
// 		let system = System::new();
// 		let root = system.root();
// 		// Node does not exist yet
// 		assert!(root.get_child("/blah").is_none());
// 		assert!(system.get_node_at_path("/blah").is_none());
// 		// Create note
// 		root.set_child("blah", |w| NoopNode::new(w)).unwrap();
// 		// Node exists
// 		root.get_child("blah").unwrap();
// 		system.get_node_at_path("/blah").unwrap();
// 		// Remove node
// 		root.remove_child("blah").unwrap();
// 		// Node no longer exists
// 		assert!(root.get_child("/blah").is_none());
// 		assert!(system.get_node_at_path("/blah").is_none());
// 	}
//
// 	#[test]
// 	fn system_calls() {
// 		let system = System::new();
// 		system.get_node_at_path("/").unwrap();
// 		assert!(system.get_node_at_path("/noop").is_none());
// 		system.set_node_at_path("/noop", NoopNode::new).unwrap();
// 		system.get_node_at_path("/noop").unwrap();
// 		system.remove_node_at_path("/noop").unwrap();
// 		assert!(system.get_node_at_path("/noop").is_none());
// 	}
//
// 	#[test]
// 	fn multiple_paths() {
// 		let system = System::new();
// 		system.set_node_at_path("/noop", NoopNode::new).unwrap();
// 		system
// 			.set_node_at_path("/dirs", DirectoryNode::new)
// 			.unwrap();
// 		system
// 			.set_node_at_path("/rw", |w| {
// 				FileSystemDirectoryNode::new(
// 					w,
// 					std::env::current_dir().unwrap().join("target"),
// 					false,
// 				)
// 			})
// 			.unwrap();
// 		// Create `testing` directory in target if it doesn't already exist, this creates a real directory on the filesystem.
// 		if system.get_node_at_path("/rw/testing").is_none() {
// 			// Creating a FileSystemDirectoryNode within a FileSystemDirectoryNode will create a real directory.
// 			// Casting to the FileSystemDirectoryNode would give more direct and efficient access, but this works fine.
// 			system
// 				.set_node_at_path("/rw/testing", |w| {
// 					FileSystemDirectoryNode::new(w, "".into(), false)
// 				})
// 				.unwrap();
// 		}
// 		// This `testing` directory definitely exists now
// 		system.get_node_at_path("/rw/testing").unwrap();
// 		system
// 			.set_node_at_path("/ro", |w| {
// 				FileSystemDirectoryNode::new(
// 					w,
// 					std::env::current_dir()
// 						.unwrap()
// 						.join("target")
// 						.join("testing"),
// 					true,
// 				)
// 			})
// 			.unwrap();
// 		// Can't create directories in a read-only FileSystemDirectoryNode
// 		assert!(system
// 			.set_node_at_path("/ro/blah", |w| FileSystemDirectoryNode::new(
// 				w,
// 				"".into(),
// 				false
// 			))
// 			.is_err());
// 		// No escaping your root
// 		assert!(system.get_node_at_path("/ro/blah/../../../boop").is_none());
// 	}
//
// 	#[test]
// 	fn cast_nodes() {
// 		let system = System::new();
// 		system.set_node_at_path("/noop", NoopNode::new).unwrap();
// 		let node = system.get_node_at_path("/noop").unwrap();
// 		// Useful temporary usage reference
// 		let _noop: &NoopNode = node.downcast_ref().unwrap();
// 		// Or fully promote it for long-term use, though storing it as a Weak<NoopNode> would be
// 		// obviously better for lifetime reasons in the vast majority of cases.
// 		let _noop: Arc<NoopNode> = node.downcast_arc().unwrap();
// 	}
//
// 	#[test]
// 	fn parent_paths() {
// 		let system = System::new();
// 		system.set_node_at_path("/noop", NoopNode::new).unwrap();
// 		system.set_node_at_path("/noop2", NoopNode::new).unwrap();
// 		let node = system.get_node_at_path("/noop").unwrap();
// 		system.get_node_at_path("/noop/../noop").unwrap();
// 		system.get_node_at_path("/noop/../noop2").unwrap();
// 		node.get_child("../noop").unwrap();
// 		node.get_child("../noop2").unwrap();
// 	}
//
// 	#[test]
// 	fn downcast_caps() {
// 		let system = System::new();
// 		system.set_node_at_path("/noop", NoopNode::new).unwrap();
// 		system
// 			.set_node_at_path("/ro", |p| {
// 				FileSystemDirectoryNode::new(p, std::env::current_dir().unwrap(), true)
// 			})
// 			.unwrap();
// 		let cargo_toml = system.get_node_at_path("/ro/Cargo.toml").unwrap();
// 		// let reader = cargo_toml.downcast_capability::<dyn Write>();
// 	}
// }
