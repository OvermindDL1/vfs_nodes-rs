use crate::node_implementation_helpers::*;
use crate::{ArcNode, CowArcNode, CowWeakNode, Node, WeakNode};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Components;
use std::sync::{Arc, RwLock, Weak};

#[derive(Debug)]
pub struct DirectoryNode {
	this: RwLock<WeakNode>,
	parent: WeakNode,
	children: RwLock<HashMap<OsString, ArcNode>>,
}

impl DirectoryNode {
	pub fn new(parent: WeakNode) -> ArcNode {
		let self_arc = Arc::new(Self {
			this: RwLock::new(Weak::<Self>::new()),
			parent,
			children: Default::default(),
		});
		let weak = Arc::downgrade(&self_arc);
		*self_arc.this.write().expect("impossible?") = weak;
		self_arc
	}
}

impl Node for DirectoryNode {
	fn get_child_node_at(&self, name: &OsStr, components: &mut Components) -> Option<CowArcNode> {
		let children = self.children.read().expect("poisoned lock");
		if let Some(child) = children.get(name) {
			match through_next_useful_component(components, child) {
				ThroughResult::PathEnd => Some(CowArcNode::Owned(child.clone())),
				ThroughResult::InvalidParent => None,
				ThroughResult::Name(name) => child
					.get_child_node_at(name, components)
					.map(|n| CowArcNode::Owned(n.into_owned())),
				ThroughResult::JustNode(parent) => Some(CowArcNode::Owned(parent.into_owned())),
				ThroughResult::NameNode(name, parent) => parent
					.get_child_node_at(name, components)
					.map(|n| CowArcNode::Owned(n.into_owned())),
			}
		} else {
			None
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
		match next_useful_component(components) {
			None => {
				// End of path, try to insert child
				let mut children = self.children.write().expect("poisoned lock");
				if children.contains_key(name) {
					//  But the child already exists, error
					return Err("child already exists, cannot insert");
				} else {
					// Child doesn't exist and no more path, so insert
					let node = constructor(self.this.read().expect("lock poisoned").clone());
					children.insert(name.to_owned(), node);
					return Ok(());
				}
			}
			Some(component) => {
				{
					let children = self.children.read().expect("poisoned lock");
					if let Some(child) = children.get(name) {
						match component.through_parents_of(components, child) {
							ThroughResult::PathEnd => {
								return Err("child node already exists, remove first")
							}
							ThroughResult::InvalidParent => return Err("ran out of parent nodes"),
							ThroughResult::Name(name) => {
								return child.set_child_node_at(name, components, constructor)
							}
							ThroughResult::JustNode(_parent) => {
								return Err("cannot insert as a parent node that already exists")
							}
							ThroughResult::NameNode(name, parent) => {
								// Ensure the lock isi dropped in case it ends up coming back up to self
								drop(children);
								return parent.set_child_node_at(name, components, constructor);
							}
						}
					} else {
						return Err("child node does not exist");
					}
				}
			}
		}
	}

	fn remove_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
	) -> Result<(), &'static str> {
		let mut children = self.children.write().expect("poisoned lock");
		if let Some(child) = children.get(name) {
			match through_next_useful_component(components, child) {
				ThroughResult::PathEnd => {
					children.remove(name);
					Ok(())
				}
				ThroughResult::InvalidParent => Err("ran out of parent nodes"),
				ThroughResult::Name(name) => child.remove_child_node_at(name, components),
				ThroughResult::JustNode(_parent) => Err("cannot remove a parent through a child"),
				ThroughResult::NameNode(name, parent) => {
					parent.remove_child_node_at(name, components)
				}
			}
		} else {
			Err("child node does not exist")
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::nodes::directory::DirectoryNode;
	use crate::System;

	#[test]
	fn read_write_dir() {
		let system = System::new();
		system.root().set_child("blah", DirectoryNode::new).unwrap();
		system
			.root()
			.set_child("blah/blorp", DirectoryNode::new)
			.unwrap();
		system
			.get_node_at_path("/blah")
			.unwrap()
			.set_child("bleep", DirectoryNode::new)
			.unwrap();
		system.get_node_at_path("/blah/blorp").unwrap();
		system.get_node_at_path("/blah/bleep").unwrap();
		system
			.get_node_at_path("/blah")
			.unwrap()
			.remove_child("blorp")
			.unwrap();
		assert!(system.get_node_at_path("/blah/blorp").is_none());
	}
}
