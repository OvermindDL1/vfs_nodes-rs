use std::any::Any;
use std::sync::Arc;

pub trait AsAnyCast: Any + Send + Sync {
	fn type_name(&self) -> &'static str;
	fn as_any(&self) -> &dyn Any;
	fn as_any_mut(&mut self) -> &mut dyn Any;
	fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl<T: Any + Send + Sync> AsAnyCast for T {
	fn type_name(&self) -> &'static str {
		std::any::type_name::<T>()
	}

	fn as_any(&self) -> &dyn Any {
		self
	}

	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}

	fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
		self
	}
}
