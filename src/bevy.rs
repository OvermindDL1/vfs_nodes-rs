pub mod plugin {
	// use crate::bevy::BevyNodeController;
	use bevy_app::{AppBuilder, Plugin, StartupStage};
	use bevy_ecs::system::IntoExclusiveSystem;
	use bevy_ecs::world::World;

	pub struct BevyNodesPlugin;

	impl Plugin for BevyNodesPlugin {
		fn build(&self, app: &mut AppBuilder) {
			app.add_startup_system_to_stage(StartupStage::PreStartup, init.exclusive_system());
		}
	}

	fn init(_world: &mut World) {
		// if !world.contains_resource::<BevyNodeController>() {
		//     let mut entity = world.spawn();
		//     let con = BevyNodeController::new(entity.id());
		//     let root = con.root().clone();
		//     entity.insert(root);
		//     world.insert_resource(con);
		// }
	}
}
