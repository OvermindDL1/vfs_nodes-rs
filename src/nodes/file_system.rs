use crate::node_implementation_helpers::*;
use crate::{ArcNode, CowArcNode, CowWeakNode, Node, WeakNode};
use std::ffi::OsStr;
use std::path::{Components, PathBuf};
use std::sync::{Arc, RwLock, Weak};

#[derive(Debug)]
pub struct FileSystemDirectoryNode {
	this: RwLock<WeakNode>,
	parent: WeakNode,
	root_path: PathBuf,
	read_only: bool,
}

enum Either<L, R> {
	Left(L),
	Right(R),
}

impl FileSystemDirectoryNode {
	pub fn new(parent: WeakNode, root_path: PathBuf, read_only: bool) -> ArcNode {
		let self_arc = Arc::new(Self {
			this: RwLock::new(Weak::<Self>::new()),
			parent,
			root_path,
			read_only,
		});
		let weak = Arc::downgrade(&self_arc);
		*self_arc.this.write().expect("impossible?") = weak;
		self_arc
	}

	fn get_node_or_path<
		R,
		Op: FnOnce(ArcNode, &OsStr, &mut Components) -> R,
		W: FnOnce(Option<CowArcNode<'static>>) -> R,
	>(
		root_path: &PathBuf,
		mut path: PathBuf,
		parent: &WeakNode,
		components: &mut Components,
		wrap: W,
		op: Op,
	) -> Either<PathBuf, R> {
		while let Some(component) = next_useful_component(components) {
			match component {
				UsefulComponent::ParentDir => {
					if &path == root_path {
						if let Some(mut parent) = parent.upgrade() {
							while let Some(component) = next_useful_component(components) {
								match component {
									UsefulComponent::ParentDir => {
										parent = if let Some(parent) =
											parent.get_parent_node().upgrade()
										{
											parent
										} else {
											return Either::Right(wrap(None));
										};
									}
									UsefulComponent::Normal(name) => {
										return Either::Right(op(parent, name, components))
									}
								}
							}
							return Either::Right(wrap(Some(CowArcNode::Owned(parent))));
						} else {
							return Either::Right(wrap(None));
						}
					} else {
						path.pop();
					}
				}
				UsefulComponent::Normal(name) => path.push(name),
			}
		}
		Either::Left(path)
	}
}

impl Node for FileSystemDirectoryNode {
	fn get_child_node_at(&self, name: &OsStr, components: &mut Components) -> Option<CowArcNode> {
		match FileSystemDirectoryNode::get_node_or_path(
			&self.root_path,
			self.root_path.join(name),
			&self.parent,
			components,
			|opt| opt,
			|node, name, components| {
				node.get_child_node_at(name, components)
					.map(|n| CowArcNode::Owned(n.into_owned()))
			},
		) {
			Either::Right(ret) => ret,
			Either::Left(path) => {
				if path.is_dir() {
					Some(CowArcNode::Owned(FileSystemDirectoryNode::new(
						self.this.read().expect("poisoned lock").clone(),
						path,
						self.read_only.clone(),
					)))
				} else if path.is_file() {
					todo!()
				} else {
					None
				}
			}
		}
	}

	fn get_parent_node(&self) -> CowWeakNode {
		CowWeakNode::Borrowed(&self.parent)
	}

	fn set_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
		constructor: &mut dyn FnMut(WeakNode) -> ArcNode,
	) -> Result<(), &'static str> {
		match FileSystemDirectoryNode::get_node_or_path(
			&self.root_path,
			self.root_path.join(name),
			&self.parent,
			components,
			|opt| {
				opt.ok_or("invalid path")
					.and_then(|_| Err("node already exists, cannot overwrite, remove it first"))
			},
			|node, name, components| node.set_child_node_at(name, components, constructor),
		) {
			Either::Right(ret) => return ret,
			Either::Left(path) => {
				// Still within our root, thus:
				if self.read_only {
					return Err("read-only, cannot create");
				}
				if path.exists() {
					return Err("node already exists");
				}
				let node = constructor(self.this.read().expect("poisoned lock").clone());
				if let Ok(_fsd) = node.downcast_ref::<FileSystemDirectoryNode>() {
					std::fs::create_dir(path).map_err(|_e| "failed creating directory")
				} else {
					todo!("create FileSystemFileNode")
				}
			}
		}
	}

	fn remove_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
	) -> Result<(), &'static str> {
		match FileSystemDirectoryNode::get_node_or_path(
			&self.root_path,
			self.root_path.join(name),
			&self.parent,
			components,
			|opt| {
				opt.ok_or("invalid path")
					.and_then(|_| Err("node already exists, cannot overwrite, remove it first"))
			},
			|node, name, components| node.remove_child_node_at(name, components),
		) {
			Either::Right(ret) => return ret,
			Either::Left(path) => {
				// Still within our root, thus:
				if self.read_only {
					return Err("read-only, cannot remove");
				}
				if !path.exists() {
					return Err("path does not exist");
				}
				if path.is_dir() {
					std::fs::remove_dir(path).map_err(|_e| "unable to remove the directory")
				} else if path.is_file() {
					std::fs::remove_file(path).map_err(|_e| "unable to remove the file")
				} else {
					Err("not a file or directory")
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::nodes::file_system::FileSystemDirectoryNode;
	use crate::System;

	#[test]
	fn read_write_dir() {
		let system = System::new();
		system
			.root()
			.set_child("read_write", |w| {
				FileSystemDirectoryNode::new(w, std::env::current_dir().unwrap(), false)
			})
			.unwrap();
		// Targets Directory does exist
		assert!(system.get_node_at_path("/read_write/targets").is_none());
		// Single depth
		system.get_node_at_path("/read_write/target").unwrap();
		// Multiple depth
		let debug = system.get_node_at_path("/read_write/target/debug").unwrap();
		// Make certain that the test directory does not yet exist
		let dir = &debug
			.downcast_ref::<FileSystemDirectoryNode>()
			.unwrap()
			.root_path;
		assert!(dir.starts_with(std::env::current_dir().unwrap())); // Make certain the directory exists in the current_dir
		let _ = std::fs::remove_dir(dir.join("test_dir"));
		// Verify that the test_dir directory does not yet exist
		assert!(system
			.get_node_at_path("/read_write/target/debug/test_dir")
			.is_none());
		// Create directory
		debug
			.set_child("test_dir", |p| {
				FileSystemDirectoryNode::new(p, "".into(), false)
			})
			.unwrap();
		// Verify that created directory exists
		assert!(system
			.get_node_at_path("/read_write/target/debug/test_dir")
			.is_some());
		debug.remove_child("test_dir").unwrap();
		// Verify that the test_dir directory does not exist again
		assert!(system
			.get_node_at_path("/read_write/target/debug/test_dir")
			.is_none());
	}
}
